//! Windows host for the SPK-008 spike: one top-level window, one WebView2
//! controller with a transparent default background, the bundled-origin
//! policy from SPK-001, and the `--smoke` sequence:
//!
//! page report -> memory sample -> UI Automation walk (worker thread) ->
//! memory sample -> CapturePreview PNG + screen BMP (with `--out`) -> JSON.
//!
//! Everything WebView2 runs on one STA UI thread; shared state lives in a
//! thread-local and borrows are never held across calls that pump messages.

use std::{
    cell::RefCell,
    ffi::c_void,
    fmt,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{OnceLock, mpsc},
    time::{Duration, Instant},
};

use webview2_com::{
    CapturePreviewCompletedHandler, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, Microsoft::Web::WebView2::Win32::*,
    NavigationCompletedEventHandler, NavigationStartingEventHandler,
    NewWindowRequestedEventHandler, PermissionRequestedEventHandler,
    WebMessageReceivedEventHandler, WebResourceRequestedEventHandler, take_pwstr,
};
use windows::{
    Win32::{
        Foundation::{
            CloseHandle, E_POINTER, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
        },
        Graphics::{
            Dwm::{
                DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
                DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
            },
            Gdi::{
                BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, ClientToScreen,
                CreateCompatibleBitmap, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject,
                GetDC, GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
            },
        },
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED,
                CoCreateInstance, CoInitializeEx, CoUninitialize, IStream, STREAM_SEEK_SET,
            },
            LibraryLoader::GetModuleHandleW,
            ProcessStatus::{
                K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
            },
            Threading::{GetCurrentProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
        },
        UI::{
            Accessibility::{CUIAutomation, IUIAutomation, TreeScope_Descendants},
            Controls::MARGINS,
            HiDpi::{
                AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                GetDpiForWindow, SetProcessDpiAwarenessContext,
            },
            Shell::SHCreateMemStream,
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
                GetForegroundWindow, GetMessageW, HWND_TOPMOST, IDC_ARROW, KillTimer, LoadCursorW,
                MSG, PostMessageW, PostQuitMessage, RegisterClassExW, SW_SHOW, SW_SHOWNOACTIVATE,
                SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetForegroundWindow, SetTimer,
                SetWindowPos, ShowWindow, TranslateMessage, WM_APP, WM_CLOSE, WM_DESTROY, WM_SIZE,
                WM_TIMER, WNDCLASSEXW, WS_CLIPCHILDREN, WS_EX_NOREDIRECTIONBITMAP,
                WS_OVERLAPPEDWINDOW,
            },
        },
    },
    core::{BOOL, HSTRING, Interface, PCWSTR, PWSTR, w},
};

use crate::logic::{self, EXPECTED_A11Y, HostMessage, ORIGIN, Options, json_string};

const WM_UIA_DONE: u32 = WM_APP + 1;
const TIMER_TIMEOUT: usize = 1;
static START: OnceLock<Instant> = OnceLock::new();

/// Typed failure; permission-denied does not occur on these code paths.
#[derive(Debug)]
pub enum SpikeError {
    /// A required system component is missing (the WebView2 Runtime).
    Unavailable { what: &'static str, detail: String },
    /// A Win32 or WebView2 call failed.
    Failure { what: &'static str, detail: String },
}

impl fmt::Display for SpikeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable { what, detail } => write!(f, "unavailable: {what}: {detail}"),
            Self::Failure { what, detail } => write!(f, "failure: {what}: {detail}"),
        }
    }
}

fn failure<E: fmt::Debug>(what: &'static str) -> impl FnOnce(E) -> SpikeError {
    move |err| SpikeError::Failure {
        what,
        detail: format!("{err:?}"),
    }
}

fn ms() -> u128 {
    START.get().map_or(0, |start| start.elapsed().as_millis())
}

fn log(line: &str) {
    eprintln!("[t+{:>5}ms] {line}", ms());
}

/// One UI Automation element as seen from the host.
#[derive(Debug, Clone, Default)]
struct UiaNode {
    name: String,
    control_type: i32,
    localized_type: String,
    aria_role: String,
}

#[derive(Debug, Default)]
struct UiaResult {
    attempts: u32,
    ms: u128,
    nodes: Vec<UiaNode>,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct ProcMem {
    kind: &'static str,
    pid: u32,
    working_set: usize,
    private: usize,
}

#[derive(Default)]
struct State {
    hwnd: HWND,
    out_dir: Option<PathBuf>,
    smoke: bool,
    env: Option<ICoreWebView2Environment>,
    controller: Option<ICoreWebView2Controller>,
    webview: Option<ICoreWebView2>,
    runtime_version: String,
    environment_version: String,
    dpi_awareness: String,
    window_dpi: u32,
    client_px: (i32, i32),
    transparent_background: String,
    dwm_backdrop: String,
    dwm_extend_frame: String,
    navigation: Option<(u128, bool, i32)>,
    report: Option<String>,
    report_ms: Option<u128>,
    foreground_at_report: bool,
    served_requests: u32,
    blocked: Vec<String>,
    rejected_messages: u32,
    memory_at_report: Vec<ProcMem>,
    memory_after_uia: Vec<ProcMem>,
    uia: Option<UiaResult>,
    uia_rx: Option<mpsc::Receiver<UiaResult>>,
    captures: Vec<String>,
    finished: bool,
    exit: u8,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

// ---------------------------------------------------------------- entry ----

pub fn run(options: Options) -> ExitCode {
    let _ = START.set(Instant::now());
    state(|s| {
        s.smoke = options.smoke_seconds.is_some();
        s.out_dir = options.out_dir.clone();
    });
    match run_inner(&options) {
        Ok(()) => ExitCode::from(state(|s| s.exit)),
        Err(err) => {
            eprintln!("error: {err}");
            match err {
                SpikeError::Unavailable { .. } => ExitCode::from(3),
                SpikeError::Failure { .. } => ExitCode::from(1),
            }
        }
    }
}

fn run_inner(options: &Options) -> Result<(), SpikeError> {
    let awareness =
        unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    state(|s| {
        s.dpi_awareness = match awareness {
            Ok(()) => "per-monitor-v2".to_owned(),
            Err(err) => format!("unchanged ({err})"),
        }
    });
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .map_err(failure("CoInitializeEx"))?;

    let runtime = webview2_runtime_version()?;
    log(&format!("WebView2 Runtime {runtime}"));
    state(|s| s.runtime_version = runtime);

    let hwnd = create_window()?;
    state(|s| s.hwnd = hwnd);
    apply_dwm_backdrop(hwnd);
    if let Some(dir) = &options.out_dir {
        // What is on screen where the client area will be, before the window
        // exists visually: the reference for judging see-through regions.
        let result = std::fs::create_dir_all(dir)
            .map_err(|e| e.to_string())
            .and_then(|()| write_screen_bmp(hwnd, &dir.join("behind.bmp")));
        record_capture(result);
    }
    unsafe {
        if options.activate {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        if options.smoke_seconds.is_some() {
            // Topmost during the smoke run so occlusion never throttles rAF.
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            );
        }
    }

    let env = create_environment()?;
    let version = browser_version(&env);
    state(|s| {
        s.environment_version = version;
        s.env = Some(env.clone());
    });
    create_view(&env, hwnd)?;

    if let Some(seconds) = options.smoke_seconds {
        unsafe { SetTimer(Some(hwnd), TIMER_TIMEOUT, seconds * 1000, None) };
        log(&format!("smoke mode: timeout after {seconds}s"));
    } else {
        log("interactive: close the window to exit");
    }

    let mut msg = MSG::default();
    loop {
        match unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 {
            0 => break,
            -1 => return Err(failure("GetMessageW")(windows::core::Error::from_thread())),
            _ => unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            },
        }
    }
    Ok(())
}

fn webview2_runtime_version() -> Result<String, SpikeError> {
    let mut version = PWSTR::null();
    let result =
        unsafe { GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut version) };
    let text = take_pwstr(version);
    match result {
        Ok(()) if !text.is_empty() => Ok(text),
        other => Err(SpikeError::Unavailable {
            what: "Microsoft Edge WebView2 Runtime",
            detail: format!(
                "no installed runtime found ({}); install the Evergreen runtime",
                other
                    .err()
                    .map_or_else(|| "empty version".to_owned(), |e| e.to_string())
            ),
        }),
    }
}

fn browser_version(env: &ICoreWebView2Environment) -> String {
    let mut version = PWSTR::null();
    match unsafe { env.BrowserVersionString(&mut version) } {
        Ok(()) => take_pwstr(version),
        Err(err) => format!("unknown ({err})"),
    }
}

// ------------------------------------------------------------- window ----

fn create_window() -> Result<HWND, SpikeError> {
    unsafe {
        let module = GetModuleHandleW(None).map_err(failure("GetModuleHandleW"))?;
        let instance = HINSTANCE(module.0);
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_ARROW).map_err(failure("LoadCursorW"))?,
            lpszClassName: w!("GlintSpikeWebView2Baseline"),
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 {
            return Err(failure("RegisterClassExW")(
                windows::core::Error::from_thread(),
            ));
        }
        // WS_EX_NOREDIRECTIONBITMAP: no GDI redirection surface, so DWM's
        // system backdrop can show through wherever WebView2 draws alpha 0.
        let hwnd = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP,
            w!("GlintSpikeWebView2Baseline"),
            w!("Glint SPK-008 WebView2 baseline"),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            40,
            40,
            100,
            100,
            None,
            None,
            Some(instance),
            None,
        )
        .map_err(failure("CreateWindowExW"))?;
        let dpi = GetDpiForWindow(hwnd);
        let (w, h) = logic::WINDOW_SIZE_DIP;
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: logic::scale(w, dpi),
            bottom: logic::scale(h, dpi),
        };
        let _ = AdjustWindowRectExForDpi(
            &mut rect,
            WS_OVERLAPPEDWINDOW,
            false,
            WS_EX_NOREDIRECTIONBITMAP,
            dpi,
        );
        let _ = SetWindowPos(
            hwnd,
            None,
            40,
            40,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOACTIVATE,
        );
        state(|s| s.window_dpi = dpi);
        Ok(hwnd)
    }
}

/// Asks DWM for the transient-window (acrylic) system backdrop behind the
/// whole client area. Results are recorded, never assumed.
fn apply_dwm_backdrop(hwnd: HWND) {
    let dark = BOOL(1);
    let backdrop = DWMSBT_TRANSIENTWINDOW;
    let margins = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    let (dark_hr, backdrop_hr, extend_hr) = unsafe {
        (
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark as *const BOOL as *const c_void,
                std::mem::size_of::<BOOL>() as u32,
            ),
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &backdrop as *const _ as *const c_void,
                std::mem::size_of_val(&backdrop) as u32,
            ),
            DwmExtendFrameIntoClientArea(hwnd, &margins),
        )
    };
    let fmt_hr = |r: windows::core::Result<()>| match r {
        Ok(()) => "ok".to_owned(),
        Err(err) => format!("error {err}"),
    };
    state(|s| {
        s.dwm_backdrop = format!(
            "DWMSBT_TRANSIENTWINDOW: {}; immersive dark mode: {}",
            fmt_hr(backdrop_hr),
            fmt_hr(dark_hr)
        );
        s.dwm_extend_frame = fmt_hr(extend_hr);
    });
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_SIZE => {
            fit_view(hwnd);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_TIMEOUT => {
            unsafe {
                let _ = KillTimer(Some(hwnd), TIMER_TIMEOUT);
            }
            log("timeout: the smoke sequence did not finish in time");
            state(|s| s.exit = 4);
            finish();
            LRESULT(0)
        }
        WM_UIA_DONE => {
            let result = state(|s| s.uia_rx.take()).and_then(|rx| rx.recv().ok());
            on_uia_done(result.unwrap_or_else(|| UiaResult {
                error: Some("UI Automation worker vanished".to_owned()),
                ..Default::default()
            }));
            LRESULT(0)
        }
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            if let Some(controller) = state(|s| {
                s.webview = None;
                s.controller.take()
            }) {
                let _ = unsafe { controller.Close() };
            }
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn fit_view(hwnd: HWND) {
    let Ok(controller) = STATE.with(|s| s.try_borrow().map(|s| s.controller.clone())) else {
        return;
    };
    if let Some(controller) = controller {
        let mut rect = RECT::default();
        unsafe {
            let _ = GetClientRect(hwnd, &mut rect);
            let _ = controller.SetBounds(rect);
        }
    }
}

// ------------------------------------------------------------ webview ----

fn create_environment() -> Result<ICoreWebView2Environment, SpikeError> {
    // Spike-only profile directory; product hosts choose their own (WIN-*).
    let data_dir = std::env::temp_dir()
        .join("glint-spike-webview2-baseline")
        .join("webview2-data");
    let data_dir = HSTRING::from(data_dir.as_os_str());
    let (tx, rx) = mpsc::channel();
    CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            CreateCoreWebView2EnvironmentWithOptions(
                PCWSTR::null(),
                &data_dir,
                None::<&ICoreWebView2EnvironmentOptions>,
                &handler,
            )
            .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(move |result, environment| {
            result?;
            let _ = tx.send(environment.ok_or_else(|| windows::core::Error::from(E_POINTER)));
            Ok(())
        }),
    )
    .map_err(failure("CreateCoreWebView2EnvironmentWithOptions"))?;
    rx.recv()
        .map_err(failure("environment channel"))?
        .map_err(failure("CreateCoreWebView2EnvironmentWithOptions"))
}

fn create_view(env: &ICoreWebView2Environment, parent: HWND) -> Result<(), SpikeError> {
    let (tx, rx) = mpsc::channel();
    let creator = env.clone();
    CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            creator
                .CreateCoreWebView2Controller(parent, &handler)
                .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(move |result, controller| {
            result?;
            let _ = tx.send(controller.ok_or_else(|| windows::core::Error::from(E_POINTER)));
            Ok(())
        }),
    )
    .map_err(failure("CreateCoreWebView2Controller"))?;
    let controller = rx
        .recv()
        .map_err(failure("controller channel"))?
        .map_err(failure("CreateCoreWebView2Controller"))?;
    let webview = unsafe { controller.CoreWebView2() }.map_err(failure("CoreWebView2"))?;

    // Alpha 0: the page's transparent html/body shows whatever is behind the
    // controller (the DWM backdrop when the window allows it).
    let transparent = controller
        .cast::<ICoreWebView2Controller2>()
        .and_then(|c| unsafe {
            c.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                A: 0,
                R: 0,
                G: 0,
                B: 0,
            })
        });
    state(|s| {
        s.transparent_background = match transparent {
            Ok(()) => "ok (ICoreWebView2Controller2, alpha 0)".to_owned(),
            Err(err) => format!("error {err}"),
        }
    });

    configure_settings(&webview).map_err(failure("ICoreWebView2Settings"))?;
    install_handlers(env, &webview).map_err(failure("install handlers"))?;

    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(parent, &mut rect);
        controller.SetBounds(rect).map_err(failure("SetBounds"))?;
        controller
            .SetIsVisible(true)
            .map_err(failure("SetIsVisible"))?;
    }
    state(|s| {
        s.client_px = (rect.right - rect.left, rect.bottom - rect.top);
        s.controller = Some(controller);
        s.webview = Some(webview.clone());
    });
    let url = HSTRING::from(format!("{ORIGIN}index.html"));
    unsafe { webview.Navigate(&url) }.map_err(failure("Navigate"))?;
    Ok(())
}

fn configure_settings(webview: &ICoreWebView2) -> windows::core::Result<()> {
    let dev = cfg!(debug_assertions);
    unsafe {
        let settings = webview.Settings()?;
        settings.SetAreDevToolsEnabled(dev)?;
        settings.SetAreDefaultContextMenusEnabled(false)?;
        settings.SetIsStatusBarEnabled(false)?;
        settings.SetAreHostObjectsAllowed(false)?;
        settings.SetIsZoomControlEnabled(false)?;
        settings.SetAreDefaultScriptDialogsEnabled(false)?;
        settings.SetIsBuiltInErrorPageEnabled(false)?;
        settings.SetIsWebMessageEnabled(true)?;
        if let Ok(settings) = settings.cast::<ICoreWebView2Settings4>() {
            settings.SetIsPasswordAutosaveEnabled(false)?;
            settings.SetIsGeneralAutofillEnabled(false)?;
        }
    }
    Ok(())
}

/// The SPK-001 policy: only the bundled origin, served from memory; no new
/// windows, no permissions, no network.
fn install_handlers(
    env: &ICoreWebView2Environment,
    webview: &ICoreWebView2,
) -> windows::core::Result<()> {
    let mut token = 0i64;
    let handler = NavigationStartingEventHandler::create(Box::new(|_, args| {
        let Some(args) = args else { return Ok(()) };
        let mut uri = PWSTR::null();
        unsafe { args.Uri(&mut uri)? };
        let uri = take_pwstr(uri);
        if !logic::is_bundled_url(&uri) {
            unsafe { args.SetCancel(true)? };
            state(|s| s.blocked.push(format!("navigation {uri}")));
        }
        Ok(())
    }));
    unsafe { webview.add_NavigationStarting(&handler, &mut token)? };

    let handler = NewWindowRequestedEventHandler::create(Box::new(|_, args| {
        if let Some(args) = args {
            unsafe { args.SetHandled(true)? };
            state(|s| s.blocked.push("new-window".to_owned()));
        }
        Ok(())
    }));
    unsafe { webview.add_NewWindowRequested(&handler, &mut token)? };

    let handler = PermissionRequestedEventHandler::create(Box::new(|_, args| {
        if let Some(args) = args {
            unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)? };
            state(|s| s.blocked.push("permission".to_owned()));
        }
        Ok(())
    }));
    unsafe { webview.add_PermissionRequested(&handler, &mut token)? };

    unsafe {
        webview.AddWebResourceRequestedFilter(w!("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?
    };
    let env = env.clone();
    let handler = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        let mut uri = PWSTR::null();
        unsafe { args.Request()?.Uri(&mut uri)? };
        let uri = take_pwstr(uri);
        let (status, reason, body, headers): (i32, &str, &[u8], String) = match logic::asset_for_url(
            &uri,
        ) {
            Some(asset) => {
                state(|s| s.served_requests += 1);
                (
                    200,
                    "OK",
                    asset.body,
                    format!(
                        "Content-Type: {}\r\nContent-Security-Policy: {}\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store",
                        asset.mime,
                        logic::CSP
                    ),
                )
            }
            None => {
                state(|s| s.blocked.push(format!("request {uri}")));
                let (status, reason) = if logic::is_bundled_url(&uri) {
                    (404, "Not Found")
                } else {
                    (403, "Forbidden")
                };
                (
                    status,
                    reason,
                    b"",
                    "Content-Type: text/plain; charset=utf-8".to_owned(),
                )
            }
        };
        let stream = unsafe { SHCreateMemStream(Some(body)) };
        let response = unsafe {
            env.CreateWebResourceResponse(
                stream.as_ref(),
                status,
                &HSTRING::from(reason),
                &HSTRING::from(headers),
            )?
        };
        unsafe { args.SetResponse(&response)? };
        Ok(())
    }));
    unsafe { webview.add_WebResourceRequested(&handler, &mut token)? };

    let handler = NavigationCompletedEventHandler::create(Box::new(|_, args| {
        let Some(args) = args else { return Ok(()) };
        let mut success = BOOL(0);
        let mut status = COREWEBVIEW2_WEB_ERROR_STATUS(0);
        unsafe {
            args.IsSuccess(&mut success)?;
            args.WebErrorStatus(&mut status)?;
        }
        log(&format!(
            "NavigationCompleted success={} status={}",
            success.as_bool(),
            status.0
        ));
        state(|s| {
            s.navigation
                .get_or_insert((ms(), success.as_bool(), status.0));
        });
        Ok(())
    }));
    unsafe { webview.add_NavigationCompleted(&handler, &mut token)? };

    let handler = WebMessageReceivedEventHandler::create(Box::new(|_, args| {
        let Some(args) = args else { return Ok(()) };
        let mut source = PWSTR::null();
        unsafe { args.Source(&mut source)? };
        let mut raw = PWSTR::null();
        let message = (logic::is_bundled_url(&take_pwstr(source))
            && unsafe { args.TryGetWebMessageAsString(&mut raw) }.is_ok())
        .then(|| take_pwstr(raw))
        .and_then(|raw| HostMessage::parse(&raw));
        match message {
            Some(HostMessage::Report(json)) => on_report(json),
            None => state(|s| s.rejected_messages += 1),
        }
        Ok(())
    }));
    unsafe { webview.add_WebMessageReceived(&handler, &mut token)? };
    Ok(())
}

// -------------------------------------------------------------- smoke ----

fn on_report(json: String) {
    let first = state(|s| {
        if s.report.is_some() {
            return false;
        }
        log(&format!("page report received ({} bytes)", json.len()));
        s.report = Some(json);
        s.report_ms = Some(ms());
        s.foreground_at_report = unsafe { GetForegroundWindow() } == s.hwnd;
        true
    });
    if !first || !state(|s| s.smoke) {
        return;
    }
    let memory = sample_memory();
    let hwnd = state(|s| {
        s.memory_at_report = memory;
        s.hwnd
    });
    // UI Automation runs on its own MTA thread: a UIA client must not block
    // the UI thread of a window it inspects, and this thread pumps WebView2.
    let (tx, rx) = mpsc::channel();
    state(|s| s.uia_rx = Some(rx));
    let raw = hwnd.0 as isize;
    std::thread::spawn(move || {
        let result = walk_uia(raw);
        let _ = tx.send(result);
        let _ = unsafe {
            PostMessageW(
                Some(HWND(raw as *mut c_void)),
                WM_UIA_DONE,
                WPARAM(0),
                LPARAM(0),
            )
        };
    });
}

fn walk_uia(raw_hwnd: isize) -> UiaResult {
    let start = Instant::now();
    let mut result = UiaResult::default();
    if let Err(err) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok() {
        result.error = Some(format!("CoInitializeEx(MTA): {err}"));
        return result;
    }
    let outcome = (|| -> windows::core::Result<()> {
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)? };
        let root = unsafe { automation.ElementFromHandle(HWND(raw_hwnd as *mut c_void))? };
        let condition = unsafe { automation.CreateTrueCondition()? };
        // Chromium builds its accessibility tree lazily, on the first client
        // request; retry until every expected name shows up or 10 attempts.
        for attempt in 1..=10 {
            result.attempts = attempt;
            let found = unsafe { root.FindAll(TreeScope_Descendants, &condition)? };
            let count = unsafe { found.Length()? };
            let mut nodes = Vec::with_capacity(count.max(0) as usize);
            for index in 0..count {
                let Ok(element) = (unsafe { found.GetElement(index) }) else {
                    continue;
                };
                unsafe {
                    nodes.push(UiaNode {
                        name: element
                            .CurrentName()
                            .map(|b| b.to_string())
                            .unwrap_or_default(),
                        control_type: element
                            .CurrentControlType()
                            .map(|t| t.0)
                            .unwrap_or_default(),
                        localized_type: element
                            .CurrentLocalizedControlType()
                            .map(|b| b.to_string())
                            .unwrap_or_default(),
                        aria_role: element
                            .CurrentAriaRole()
                            .map(|b| b.to_string())
                            .unwrap_or_default(),
                    });
                }
            }
            let complete = EXPECTED_A11Y
                .iter()
                .all(|(name, _)| nodes.iter().any(|n| n.name == *name));
            result.nodes = nodes;
            if complete {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        Ok(())
    })();
    if let Err(err) = outcome {
        result.error = Some(format!("UI Automation: {err}"));
    }
    result.ms = start.elapsed().as_millis();
    unsafe { CoUninitialize() };
    result
}

fn on_uia_done(result: UiaResult) {
    log(&format!(
        "UI Automation: {} nodes after {} attempt(s) in {}ms{}",
        result.nodes.len(),
        result.attempts,
        result.ms,
        result
            .error
            .as_deref()
            .map(|e| format!(" ({e})"))
            .unwrap_or_default()
    ));
    let memory = sample_memory();
    let (out_dir, webview, hwnd) = state(|s| {
        s.uia = Some(result);
        s.memory_after_uia = memory;
        (s.out_dir.clone(), s.webview.clone(), s.hwnd)
    });
    let Some(dir) = out_dir else {
        finish();
        return;
    };
    if let Err(err) = std::fs::create_dir_all(&dir) {
        log(&format!("cannot create {}: {err}", dir.display()));
        finish();
        return;
    }
    record_capture(write_screen_bmp(hwnd, &dir.join("screen.bmp")));
    let Some(webview) = webview else {
        finish();
        return;
    };
    let stream = unsafe { SHCreateMemStream(None) };
    let Some(stream) = stream else {
        record_capture(Err("SHCreateMemStream failed".to_owned()));
        finish();
        return;
    };
    let target = dir.join("webview.png");
    let keep = stream.clone();
    let handler = CapturePreviewCompletedHandler::create(Box::new(move |result| {
        record_capture(
            result
                .map_err(|e| e.to_string())
                .and_then(|()| write_stream(&keep, &target)),
        );
        finish();
        Ok(())
    }));
    if let Err(err) = unsafe {
        webview.CapturePreview(
            COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
            &stream,
            &handler,
        )
    } {
        record_capture(Err(format!("CapturePreview: {err}")));
        finish();
    }
}

fn record_capture(result: Result<String, String>) {
    let line = result.unwrap_or_else(|e| format!("error: {e}"));
    log(&format!("capture: {line}"));
    state(|s| s.captures.push(line));
}

fn write_stream(stream: &IStream, path: &Path) -> Result<String, String> {
    let mut bytes = Vec::new();
    unsafe {
        stream
            .Seek(0, STREAM_SEEK_SET, None)
            .map_err(|e| e.to_string())?;
        let mut chunk = vec![0u8; 64 * 1024];
        loop {
            let mut read = 0u32;
            stream
                .Read(
                    chunk.as_mut_ptr().cast(),
                    chunk.len() as u32,
                    Some(&mut read),
                )
                .ok()
                .map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read as usize]);
        }
    }
    std::fs::write(path, &bytes).map_err(|e| e.to_string())?;
    Ok(format!(
        "webview.png ({} bytes, CapturePreview)",
        bytes.len()
    ))
}

/// Copies the screen pixels under the window's client area (what DWM
/// composed, including any system backdrop) into a 32-bit top-down BMP.
fn write_screen_bmp(hwnd: HWND, path: &Path) -> Result<String, String> {
    let mut rect = RECT::default();
    let mut origin = POINT::default();
    unsafe {
        GetClientRect(hwnd, &mut rect).map_err(|e| e.to_string())?;
        let _ = ClientToScreen(hwnd, &mut origin);
    }
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    if width <= 0 || height <= 0 {
        return Err("empty client area".to_owned());
    }
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let lines = unsafe {
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let previous = SelectObject(memory, bitmap.into());
        let blit = BitBlt(
            memory,
            0,
            0,
            width,
            height,
            Some(screen),
            origin.x,
            origin.y,
            SRCCOPY | CAPTUREBLT,
        );
        SelectObject(memory, previous);
        let lines = GetDIBits(
            memory,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        blit.map_err(|e| e.to_string())?;
        lines
    };
    if lines != height {
        return Err(format!("GetDIBits copied {lines} of {height} lines"));
    }
    let mut file = Vec::with_capacity(54 + pixels.len());
    let size = (54 + pixels.len()) as u32;
    file.extend_from_slice(b"BM");
    file.extend_from_slice(&size.to_le_bytes());
    file.extend_from_slice(&0u32.to_le_bytes());
    file.extend_from_slice(&54u32.to_le_bytes());
    file.extend_from_slice(&40u32.to_le_bytes());
    file.extend_from_slice(&width.to_le_bytes());
    file.extend_from_slice(&(-height).to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&32u16.to_le_bytes());
    file.extend_from_slice(&[0u8; 24]);
    file.extend_from_slice(&pixels);
    std::fs::write(path, &file).map_err(|e| e.to_string())?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    Ok(format!(
        "{name} ({width}x{height} at screen {},{}, BitBlt)",
        origin.x, origin.y
    ))
}

fn memory_of(process: HANDLE) -> Option<(usize, usize)> {
    let mut counters = PROCESS_MEMORY_COUNTERS_EX {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..Default::default()
    };
    unsafe {
        K32GetProcessMemoryInfo(
            process,
            &mut counters as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
            counters.cb,
        )
    }
    .as_bool()
    .then_some((counters.WorkingSetSize, counters.PrivateUsage))
}

fn sample_memory() -> Vec<ProcMem> {
    let mut out = Vec::new();
    if let Some((working_set, private)) = memory_of(unsafe { GetCurrentProcess() }) {
        out.push(ProcMem {
            kind: "host",
            pid: std::process::id(),
            working_set,
            private,
        });
    }
    let infos = state(|s| s.env.clone())
        .and_then(|env| env.cast::<ICoreWebView2Environment8>().ok())
        .and_then(|env| unsafe { env.GetProcessInfos() }.ok());
    let Some(infos) = infos else {
        return out;
    };
    let mut count = 0u32;
    let _ = unsafe { infos.Count(&mut count) };
    for index in 0..count {
        let Ok(info) = (unsafe { infos.GetValueAtIndex(index) }) else {
            continue;
        };
        let (mut pid, mut kind) = (0i32, COREWEBVIEW2_PROCESS_KIND(0));
        unsafe {
            let _ = info.ProcessId(&mut pid);
            let _ = info.Kind(&mut kind);
        }
        let kind = match kind {
            COREWEBVIEW2_PROCESS_KIND_BROWSER => "browser",
            COREWEBVIEW2_PROCESS_KIND_RENDERER => "renderer",
            COREWEBVIEW2_PROCESS_KIND_GPU => "gpu",
            COREWEBVIEW2_PROCESS_KIND_UTILITY => "utility",
            _ => "other",
        };
        let sample = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid as u32) }
            .ok()
            .and_then(|handle| {
                let sample = memory_of(handle);
                let _ = unsafe { CloseHandle(handle) };
                sample
            });
        let (working_set, private) = sample.unwrap_or((0, 0));
        out.push(ProcMem {
            kind,
            pid: pid as u32,
            working_set,
            private,
        });
    }
    out
}

// ------------------------------------------------------------- report ----

fn mib(bytes: usize) -> String {
    format!("{:.1}", bytes as f64 / (1024.0 * 1024.0))
}

fn memory_json(samples: &[ProcMem]) -> String {
    let rows: Vec<String> = samples
        .iter()
        .map(|p| {
            format!(
                "{{\"kind\":\"{}\",\"pid\":{},\"workingSetMiB\":{},\"privateMiB\":{}}}",
                p.kind,
                p.pid,
                mib(p.working_set),
                mib(p.private)
            )
        })
        .collect();
    let ws: usize = samples.iter().map(|p| p.working_set).sum();
    let private: usize = samples.iter().map(|p| p.private).sum();
    format!(
        "{{\"processes\":[{}],\"totalWorkingSetMiB\":{},\"totalPrivateMiB\":{}}}",
        rows.join(","),
        mib(ws),
        mib(private)
    )
}

fn uia_json(uia: Option<&UiaResult>) -> String {
    let Some(uia) = uia else {
        return "null".to_owned();
    };
    let expected: Vec<String> = EXPECTED_A11Y
        .iter()
        .map(|(name, role)| {
            let node = uia.nodes.iter().find(|n| n.name == *name);
            format!(
                "{{\"name\":{},\"authoredRole\":{},\"found\":{},\"controlTypeId\":{},\"localizedControlType\":{},\"ariaRole\":{}}}",
                json_string(name),
                json_string(role),
                node.is_some(),
                node.map_or(0, |n| n.control_type),
                json_string(node.map_or("", |n| n.localized_type.as_str())),
                json_string(node.map_or("", |n| n.aria_role.as_str())),
            )
        })
        .collect();
    let nodes: Vec<String> = uia
        .nodes
        .iter()
        .filter(|n| !n.name.is_empty() || !n.aria_role.is_empty())
        .take(120)
        .map(|n| {
            format!(
                "{{\"name\":{},\"controlTypeId\":{},\"localizedControlType\":{},\"ariaRole\":{}}}",
                // Long text nodes (the on-page results) are cut to 120 chars.
                json_string(&n.name.chars().take(120).collect::<String>()),
                n.control_type,
                json_string(&n.localized_type),
                json_string(&n.aria_role)
            )
        })
        .collect();
    format!(
        "{{\"attempts\":{},\"ms\":{},\"nodeCount\":{},\"error\":{},\"expected\":[{}],\"namedNodes\":[{}]}}",
        uia.attempts,
        uia.ms,
        uia.nodes.len(),
        uia.error.as_deref().map_or("null".to_owned(), json_string),
        expected.join(","),
        nodes.join(",")
    )
}

/// Prints the combined JSON to stdout (and report.json with `--out`), then
/// closes the window. Idempotent: the timeout and the normal path may race.
fn finish() {
    let (json, hwnd, out_dir) = state(|s| {
        if s.finished {
            return (None, s.hwnd, None);
        }
        s.finished = true;
        let navigation = s.navigation.map_or("null".to_owned(), |(at, ok, status)| {
            format!("{{\"ms\":{at},\"success\":{ok},\"webErrorStatus\":{status}}}")
        });
        let blocked: Vec<String> = s.blocked.iter().map(|b| json_string(b)).collect();
        let captures: Vec<String> = s.captures.iter().map(|c| json_string(c)).collect();
        let report_ok = s.report.is_some() && s.navigation.is_some_and(|n| n.1);
        if !report_ok && s.exit == 0 {
            s.exit = 1;
        }
        let json = format!(
            "{{\"schema\":\"glint-spk008-host/1\",\"host\":{{\"webview2Runtime\":{},\"environmentVersion\":{},\"dpiAwareness\":{},\"windowDpi\":{},\"clientPx\":\"{}x{}\",\"defaultBackground\":{},\"dwmBackdrop\":{},\"dwmExtendFrame\":{},\"navigation\":{},\"reportMs\":{},\"foregroundAtReport\":{},\"servedRequests\":{},\"blocked\":[{}],\"rejectedMessages\":{},\"captures\":[{}],\"exitCode\":{}}},\"memory\":{{\"atReport\":{},\"afterUia\":{}}},\"uia\":{},\"page\":{}}}",
            json_string(&s.runtime_version),
            json_string(&s.environment_version),
            json_string(&s.dpi_awareness),
            s.window_dpi,
            s.client_px.0,
            s.client_px.1,
            json_string(&s.transparent_background),
            json_string(&s.dwm_backdrop),
            json_string(&s.dwm_extend_frame),
            navigation,
            s.report_ms.map_or("null".to_owned(), |v| v.to_string()),
            s.foreground_at_report,
            s.served_requests,
            blocked.join(","),
            s.rejected_messages,
            captures.join(","),
            s.exit,
            memory_json(&s.memory_at_report),
            memory_json(&s.memory_after_uia),
            uia_json(s.uia.as_ref()),
            s.report.as_deref().unwrap_or("null"),
        );
        (Some(json), s.hwnd, s.out_dir.clone())
    });
    let Some(json) = json else { return };
    println!("{json}");
    if let Some(dir) = out_dir {
        let path = dir.join("report.json");
        match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, &json)) {
            Ok(()) => log(&format!("wrote {}", path.display())),
            Err(err) => log(&format!("cannot write {}: {err}", path.display())),
        }
    }
    log(&format!("smoke result: exit {}", state(|s| s.exit)));
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}
