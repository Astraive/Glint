//! Windows host for the SPK-001 spike: appbar window, WebView2 controllers,
//! navigation/content policy and the `--smoke` probe sequence.
//!
//! Everything runs on one STA UI thread. WebView2 callbacks are delivered on
//! that thread through the message loop, so shared state lives in
//! thread-locals and borrows are never held across calls that pump messages.

use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    ffi::c_void,
    fmt,
    process::ExitCode,
    rc::Rc,
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering},
        mpsc,
    },
    time::Instant,
};

use webview2_com::{
    CreateCoreWebView2ControllerCompletedHandler, CreateCoreWebView2EnvironmentCompletedHandler,
    ExecuteScriptCompletedHandler, Microsoft::Web::WebView2::Win32::*,
    NavigationCompletedEventHandler, NavigationStartingEventHandler,
    NewWindowRequestedEventHandler, PermissionRequestedEventHandler,
    WebMessageReceivedEventHandler, WebResourceRequestedEventHandler, take_pwstr,
};
use windows::{
    Win32::{
        Foundation::{
            CloseHandle, E_POINTER, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
        },
        Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint},
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx},
            Console::SetConsoleCtrlHandler,
            LibraryLoader::GetModuleHandleW,
            ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
            Threading::{GetCurrentProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
        },
        UI::{
            HiDpi::{
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, GetDpiForWindow,
                MDT_EFFECTIVE_DPI, SetProcessDpiAwarenessContext,
            },
            Shell::{
                ABE_TOP, ABM_ACTIVATE, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS,
                ABM_WINDOWPOSCHANGED, ABN_FULLSCREENAPP, ABN_POSCHANGED, APPBARDATA,
                SHAppBarMessage, SHCreateMemStream,
            },
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
                GetMessageW, GetWindowRect, HWND_BOTTOM, HWND_TOPMOST, IDC_ARROW, IsWindowVisible,
                KillTimer, LoadCursorW, MSG, PostMessageW, PostQuitMessage, RegisterClassExW,
                RegisterWindowMessageW, SPI_GETWORKAREA, SW_HIDE, SW_SHOWNOACTIVATE,
                SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetForegroundWindow, SetTimer, SetWindowPos,
                ShowWindow, SystemParametersInfoW, TranslateMessage, WM_ACTIVATE, WM_APP, WM_CLOSE,
                WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ENDSESSION, WM_SIZE, WM_TIMER,
                WM_WINDOWPOSCHANGED, WNDCLASSEXW, WS_CLIPCHILDREN, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
                WS_POPUP,
            },
        },
    },
    core::{BOOL, HSTRING, Interface, PCWSTR, PWSTR, w},
};

use crate::logic::{self, HostMessage, ORIGIN, Options, Rect};

/// Appbar callback message (`APPBARDATA::uCallbackMessage`).
const WM_APPBAR: u32 = WM_APP + 1;
const TIMER_SMOKE_OPEN: usize = 1;
const TIMER_SMOKE_CLOSE: usize = 2;
const TIMER_SMOKE_END: usize = 3;
const TIMER_SMOKE_AFTER_REMOVE: usize = 4;

/// Cross-thread state for the console-control handler and panic hook, which
/// must be able to unregister the appbar without touching the UI thread.
static APPBAR_HWND: AtomicIsize = AtomicIsize::new(0);
static APPBAR_REGISTERED: AtomicBool = AtomicBool::new(false);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
static START: OnceLock<Instant> = OnceLock::new();

/// Typed failure. The ticket's taxonomy also names permission-denied and
/// timeout; neither occurs in this spike's code paths, so they are not modelled.
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

impl SpikeError {
    fn exit_code(&self) -> ExitCode {
        match self {
            Self::Unavailable { .. } => ExitCode::from(3),
            Self::Failure { .. } => ExitCode::from(1),
        }
    }
}

fn failure<E: fmt::Debug>(what: &'static str) -> impl FnOnce(E) -> SpikeError {
    move |err| SpikeError::Failure {
        what,
        detail: format!("{err:?}"),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Role {
    Panel,
    ControlCenter,
}

impl Role {
    fn page(self) -> &'static str {
        match self {
            Self::Panel => "panel.html",
            Self::ControlCenter => "control-center.html",
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Panel => "panel",
            Self::ControlCenter => "control-center",
        }
    }
}

struct View {
    controller: ICoreWebView2Controller,
    webview: ICoreWebView2,
}

struct App {
    appbar: HWND,
    popup: HWND,
    env: RefCell<Option<ICoreWebView2Environment>>,
    panel: RefCell<Option<View>>,
    control_center: RefCell<Option<View>>,
    bar_rect: Cell<Rect>,
    monitor_rect: Cell<Rect>,
    dpi: Cell<u32>,
    positioning: Cell<bool>,
}

impl App {
    fn view(&self, role: Role) -> &RefCell<Option<View>> {
        match role {
            Role::Panel => &self.panel,
            Role::ControlCenter => &self.control_center,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct NavResult {
    ms: u128,
    success: bool,
    web_error_status: i32,
}

#[derive(Default)]
struct Metrics {
    runtime_version: String,
    environment_version: String,
    dpi_awareness: String,
    registered_ms: Option<u128>,
    environment_ms: Option<u128>,
    navigation: BTreeMap<Role, NavResult>,
    work_area_before: Option<Rect>,
    work_area_during: Option<Rect>,
    work_area_after: Option<Rect>,
    registered_rect: Option<Rect>,
    window_rect: Option<Rect>,
    window_dpi: u32,
    messages: Vec<String>,
    rejected_messages: u32,
    csp_violations: Vec<String>,
    blocked_navigations: Vec<String>,
    blocked_new_windows: Vec<String>,
    blocked_requests: Vec<String>,
    served_requests: u32,
    denied_permissions: u32,
    popup_opened: u32,
    popup_closed: u32,
    popup_visible_after_open: bool,
    poschanged_events: u32,
    fullscreen_events: u32,
    taskbar_created_events: u32,
    memory: Vec<String>,
    smoke_passed: Option<bool>,
}

thread_local! {
    static APP: RefCell<Option<Rc<App>>> = const { RefCell::new(None) };
    static METRICS: RefCell<Metrics> = RefCell::new(Metrics::default());
    static SMOKE: Cell<Option<u32>> = const { Cell::new(None) };
}

fn app() -> Option<Rc<App>> {
    APP.with(|slot| slot.borrow().clone())
}

fn metrics<R>(f: impl FnOnce(&mut Metrics) -> R) -> R {
    METRICS.with(|m| f(&mut m.borrow_mut()))
}

fn ms() -> u128 {
    START.get().map_or(0, |start| start.elapsed().as_millis())
}

fn log(line: &str) {
    eprintln!("[t+{:>5}ms] {line}", ms());
}

fn to_rect(r: RECT) -> Rect {
    Rect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

fn to_win(r: Rect) -> RECT {
    RECT {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

fn fmt_rect(r: Option<Rect>) -> String {
    r.map_or_else(
        || "unknown".to_owned(),
        |r| {
            format!(
                "({},{})-({},{}) {}x{}",
                r.left,
                r.top,
                r.right,
                r.bottom,
                r.width(),
                r.height()
            )
        },
    )
}

// ---------------------------------------------------------------- entry ----

pub fn run(options: Options) -> ExitCode {
    let _ = START.set(Instant::now());
    SMOKE.with(|s| s.set(options.smoke_seconds));
    let result = run_inner();
    // Whatever happened, never leave a reserved work area behind.
    remove_appbar();
    APP.with(|slot| slot.borrow_mut().take());
    match result {
        Ok(()) => match metrics(|m| m.smoke_passed) {
            Some(false) => ExitCode::from(1),
            _ => ExitCode::SUCCESS,
        },
        Err(err) => {
            eprintln!("error: {err}");
            err.exit_code()
        }
    }
}

fn run_inner() -> Result<(), SpikeError> {
    // Per-monitor v2 must be set before any window exists. It fails if a
    // manifest already chose an awareness; record that instead of aborting.
    let awareness =
        unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    metrics(|m| {
        m.dpi_awareness = match awareness {
            Ok(()) => "per-monitor-v2".to_owned(),
            Err(err) => format!("unchanged ({err})"),
        }
    });

    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .map_err(failure("CoInitializeEx"))?;

    let runtime = webview2_runtime_version()?;
    log(&format!("WebView2 Runtime {runtime}"));
    metrics(|m| m.runtime_version = runtime);

    install_exit_hooks();
    let _guard = RegistrationGuard;

    let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    TASKBAR_CREATED.store(taskbar_created, Ordering::SeqCst);

    let (appbar, popup) = create_windows()?;
    let app = Rc::new(App {
        appbar,
        popup,
        env: RefCell::new(None),
        panel: RefCell::new(None),
        control_center: RefCell::new(None),
        bar_rect: Cell::new(Rect::default()),
        monitor_rect: Cell::new(Rect::default()),
        dpi: Cell::new(96),
        positioning: Cell::new(false),
    });
    APP.with(|slot| *slot.borrow_mut() = Some(app.clone()));

    metrics(|m| m.work_area_before = work_area());
    register_appbar(appbar)?;
    position_appbar(&app);
    unsafe {
        let _ = ShowWindow(appbar, SW_SHOWNOACTIVATE);
    }
    metrics(|m| {
        m.registered_ms = Some(ms());
        m.registered_rect = Some(app.bar_rect.get());
    });
    log(&format!(
        "appbar registered at {} (dpi {})",
        fmt_rect(Some(app.bar_rect.get())),
        app.dpi.get()
    ));

    let env = create_environment()?;
    metrics(|m| {
        m.environment_ms = Some(ms());
        m.environment_version = browser_version(&env);
    });
    *app.env.borrow_mut() = Some(env.clone());

    let panel = create_view(&env, appbar, Role::Panel)?;
    *app.panel.borrow_mut() = Some(panel);
    let control_center = create_view(&env, popup, Role::ControlCenter)?;
    unsafe { control_center.controller.SetIsVisible(false) }.map_err(failure("SetIsVisible"))?;
    *app.control_center.borrow_mut() = Some(control_center);

    if let Some(seconds) = SMOKE.with(Cell::get) {
        let total = seconds * 1000;
        unsafe {
            SetTimer(Some(appbar), TIMER_SMOKE_OPEN, total / 3, None);
            SetTimer(Some(appbar), TIMER_SMOKE_CLOSE, total * 2 / 3, None);
            SetTimer(Some(appbar), TIMER_SMOKE_END, total, None);
        }
        log(&format!("smoke mode: exiting after {seconds}s"));
    } else {
        log("running: use the panel's Quit button, Ctrl+C or close the console to exit");
    }
    drop(app);

    let mut msg = MSG::default();
    loop {
        let status = unsafe { GetMessageW(&mut msg, None, 0, 0) }.0;
        match status {
            0 => break,
            -1 => {
                return Err(failure("GetMessageW")(windows::core::Error::from_thread()));
            }
            _ => unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            },
        }
    }
    Ok(())
}

// ------------------------------------------------------- runtime check ----

/// Evergreen WebView2 Runtime detection. A missing runtime is a typed
/// `Unavailable` error with an actionable message, never a crash.
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
                "no installed runtime found ({}); install the Evergreen runtime from \
                 https://developer.microsoft.com/microsoft-edge/webview2/",
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

// --------------------------------------------------------- exit safety ----

/// Drop guard: unregisters the appbar on every return path of `run_inner`.
struct RegistrationGuard;

impl Drop for RegistrationGuard {
    fn drop(&mut self) {
        remove_appbar();
    }
}

fn install_exit_hooks() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if remove_appbar() {
            eprintln!("panic: appbar unregistered (ABM_REMOVE) before exit");
        }
        default_hook(info);
    }));
    if let Err(err) = unsafe { SetConsoleCtrlHandler(Some(console_ctrl), true) } {
        log(&format!("SetConsoleCtrlHandler failed: {err}"));
    }
}

/// Runs on a system thread for Ctrl+C / Ctrl+Break / console close / logoff /
/// shutdown. Unregister first (the process may be killed when this returns
/// for close events), then ask the UI thread to shut down normally.
unsafe extern "system" fn console_ctrl(_ctrl_type: u32) -> BOOL {
    remove_appbar();
    let hwnd = APPBAR_HWND.load(Ordering::SeqCst);
    if hwnd != 0 {
        let _ = unsafe {
            PostMessageW(
                Some(HWND(hwnd as *mut c_void)),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            )
        };
    }
    BOOL(1)
}

// ------------------------------------------------------------- appbar ----

fn appbar_data(hwnd: HWND) -> APPBARDATA {
    APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        uCallbackMessage: WM_APPBAR,
        ..Default::default()
    }
}

fn register_appbar(hwnd: HWND) -> Result<(), SpikeError> {
    APPBAR_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    let mut data = appbar_data(hwnd);
    if unsafe { SHAppBarMessage(ABM_NEW, &mut data) } == 0 {
        return Err(SpikeError::Failure {
            what: "SHAppBarMessage(ABM_NEW)",
            detail: "the shell refused the registration".to_owned(),
        });
    }
    APPBAR_REGISTERED.store(true, Ordering::SeqCst);
    Ok(())
}

/// Idempotent `ABM_REMOVE`; safe from the console-control thread, the panic
/// hook, `WM_DESTROY` and the drop guard. Returns whether it unregistered.
fn remove_appbar() -> bool {
    if !APPBAR_REGISTERED.swap(false, Ordering::SeqCst) {
        return false;
    }
    let hwnd = HWND(APPBAR_HWND.load(Ordering::SeqCst) as *mut c_void);
    let mut data = appbar_data(hwnd);
    unsafe { SHAppBarMessage(ABM_REMOVE, &mut data) };
    true
}

fn primary_monitor() -> (Rect, u32) {
    unsafe {
        let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let (mut dpi_x, mut dpi_y) = (96, 96);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_err() {
            dpi_x = 96;
        }
        (to_rect(info.rcMonitor), dpi_x)
    }
}

/// ABM_QUERYPOS -> adjust -> ABM_SETPOS -> move the window, as documented for
/// application desktop toolbars. Re-run on ABN_POSCHANGED, DPI and display
/// changes, and after Explorer restarts.
fn position_appbar(app: &App) {
    if app.positioning.replace(true) {
        return;
    }
    let (monitor, dpi) = primary_monitor();
    let height = logic::scale(logic::PANEL_HEIGHT_DIP, dpi);
    let mut data = appbar_data(app.appbar);
    data.uEdge = ABE_TOP;
    data.rc = to_win(logic::proposed_top_rect(monitor, height));
    unsafe { SHAppBarMessage(ABM_QUERYPOS, &mut data) };
    data.rc = to_win(logic::adjust_queried_top_rect(to_rect(data.rc), height));
    unsafe { SHAppBarMessage(ABM_SETPOS, &mut data) };
    let bar = to_rect(data.rc);
    app.bar_rect.set(bar);
    app.monitor_rect.set(monitor);
    app.dpi.set(dpi);
    unsafe {
        let _ = SetWindowPos(
            app.appbar,
            Some(HWND_TOPMOST),
            bar.left,
            bar.top,
            bar.width(),
            bar.height(),
            SWP_NOACTIVATE,
        );
    }
    app.positioning.set(false);
    fit_view(app, Role::Panel);
}

fn on_appbar_notification(app: &App, code: u32, lparam: LPARAM) {
    match code {
        ABN_POSCHANGED => {
            metrics(|m| m.poschanged_events += 1);
            position_appbar(app);
        }
        ABN_FULLSCREENAPP => {
            let entering = lparam.0 != 0;
            metrics(|m| m.fullscreen_events += 1);
            log(&format!(
                "ABN_FULLSCREENAPP: fullscreen app {}",
                if entering { "opened" } else { "closed" }
            ));
            // Documented behaviour: drop out of the topmost band while a
            // fullscreen app owns the monitor, restore afterwards.
            let insert_after = if entering { HWND_BOTTOM } else { HWND_TOPMOST };
            if entering {
                set_control_center(app, Some(false));
            }
            unsafe {
                let _ = SetWindowPos(
                    app.appbar,
                    Some(insert_after),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
        _ => {}
    }
}

fn on_taskbar_created(app: &App) {
    metrics(|m| m.taskbar_created_events += 1);
    // Explorer restarted and forgot every appbar. Re-register only if we have
    // not started shutting down (the flag is cleared by ABM_REMOVE paths).
    if APPBAR_REGISTERED.load(Ordering::SeqCst) {
        let mut data = appbar_data(app.appbar);
        let ok = unsafe { SHAppBarMessage(ABM_NEW, &mut data) } != 0;
        log(&format!(
            "TaskbarCreated: re-registered appbar (ABM_NEW={ok})"
        ));
        position_appbar(app);
    }
}

fn work_area() -> Option<Rect> {
    let mut rect = RECT::default();
    unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut rect as *mut RECT as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .ok()?;
    Some(to_rect(rect))
}

// ------------------------------------------------------------ windows ----

fn create_windows() -> Result<(HWND, HWND), SpikeError> {
    unsafe {
        let module = GetModuleHandleW(None).map_err(failure("GetModuleHandleW"))?;
        let instance = HINSTANCE(module.0);
        let cursor = LoadCursorW(None, IDC_ARROW).map_err(failure("LoadCursorW"))?;
        for (name, proc) in [
            (
                w!("GlintSpikeAppbar"),
                appbar_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
            ),
            (w!("GlintSpikeControlCenter"), popup_proc),
        ] {
            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(proc),
                hInstance: instance,
                hCursor: cursor,
                lpszClassName: name,
                ..Default::default()
            };
            if RegisterClassExW(&class) == 0 {
                return Err(failure("RegisterClassExW")(
                    windows::core::Error::from_thread(),
                ));
            }
        }
        // WS_EX_TOOLWINDOW keeps the spike out of the taskbar and Alt+Tab.
        let appbar = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            w!("GlintSpikeAppbar"),
            w!("Glint spike panel"),
            WS_POPUP | WS_CLIPCHILDREN,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
        .map_err(failure("CreateWindowExW(appbar)"))?;
        // Owned by the appbar so it is destroyed with it and stays above it.
        let popup = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            w!("GlintSpikeControlCenter"),
            w!("Glint spike control center"),
            WS_POPUP | WS_CLIPCHILDREN,
            0,
            0,
            0,
            0,
            Some(appbar),
            None,
            Some(instance),
            None,
        )
        .map_err(failure("CreateWindowExW(control center)"))?;
        Ok((appbar, popup))
    }
}

unsafe extern "system" fn appbar_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let app = app();
    match msg {
        WM_APPBAR => {
            if let Some(app) = app {
                on_appbar_notification(&app, wparam.0 as u32, lparam);
            }
            LRESULT(0)
        }
        WM_ACTIVATE | WM_WINDOWPOSCHANGED => {
            if APPBAR_REGISTERED.load(Ordering::SeqCst) {
                let message = if msg == WM_ACTIVATE {
                    ABM_ACTIVATE
                } else {
                    ABM_WINDOWPOSCHANGED
                };
                let mut data = appbar_data(hwnd);
                unsafe { SHAppBarMessage(message, &mut data) };
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_SIZE => {
            if let Some(app) = app {
                fit_view(&app, Role::Panel);
            }
            LRESULT(0)
        }
        WM_DPICHANGED | WM_DISPLAYCHANGE => {
            if let Some(app) = app {
                position_appbar(&app);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if let Some(app) = app {
                on_smoke_timer(&app, wparam.0);
            }
            LRESULT(0)
        }
        WM_ENDSESSION => {
            if wparam.0 != 0 {
                remove_appbar();
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            if let Some(app) = app {
                for role in [Role::ControlCenter, Role::Panel] {
                    if let Ok(mut slot) = app.view(role).try_borrow_mut()
                        && let Some(view) = slot.take()
                    {
                        let _ = unsafe { view.controller.Close() };
                    }
                }
            }
            if remove_appbar() {
                log("WM_DESTROY: appbar unregistered (ABM_REMOVE)");
            }
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        other if other != 0 && other == TASKBAR_CREATED.load(Ordering::SeqCst) => {
            if let Some(app) = app {
                on_taskbar_created(&app);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe extern "system" fn popup_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_SIZE => {
            if let Some(app) = app() {
                fit_view(&app, Role::ControlCenter);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if let Some(app) = app() {
                set_control_center(&app, Some(false));
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn fit_view(app: &App, role: Role) {
    let hwnd = match role {
        Role::Panel => app.appbar,
        Role::ControlCenter => app.popup,
    };
    let Ok(slot) = app.view(role).try_borrow() else {
        return;
    };
    if let Some(view) = slot.as_ref() {
        let mut rect = RECT::default();
        unsafe {
            let _ = GetClientRect(hwnd, &mut rect);
            let _ = view.controller.SetBounds(rect);
        }
    }
}

fn post_to(app: &App, role: Role, message: &str) {
    if let Ok(slot) = app.view(role).try_borrow()
        && let Some(view) = slot.as_ref()
    {
        let _ = unsafe { view.webview.PostWebMessageAsString(&HSTRING::from(message)) };
    }
}

fn set_control_center(app: &App, open: Option<bool>) {
    let visible = unsafe { IsWindowVisible(app.popup) }.as_bool();
    let target = open.unwrap_or(!visible);
    if target == visible {
        return;
    }
    if target {
        let rect = logic::popup_rect(app.bar_rect.get(), app.monitor_rect.get(), app.dpi.get());
        unsafe {
            let _ = SetWindowPos(
                app.popup,
                Some(HWND_TOPMOST),
                rect.left,
                rect.top,
                rect.width(),
                rect.height(),
                SWP_SHOWWINDOW,
            );
            let _ = SetForegroundWindow(app.popup);
        }
    } else {
        unsafe {
            let _ = ShowWindow(app.popup, SW_HIDE);
        }
    }
    if let Ok(slot) = app.control_center.try_borrow()
        && let Some(view) = slot.as_ref()
    {
        unsafe {
            let _ = view.controller.SetIsVisible(target);
            if target {
                let _ = view
                    .controller
                    .MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
            }
        }
    }
    let now_visible = unsafe { IsWindowVisible(app.popup) }.as_bool();
    metrics(|m| {
        if target {
            m.popup_opened += 1;
            m.popup_visible_after_open = now_visible;
        } else {
            m.popup_closed += 1;
        }
    });
    log(&format!(
        "control center {} (window visible: {now_visible})",
        if target { "opened" } else { "closed" }
    ));
    post_to(
        app,
        Role::Panel,
        if target {
            "control-center:open"
        } else {
            "control-center:closed"
        },
    );
}

// ------------------------------------------------------------ webview ----

fn create_environment() -> Result<ICoreWebView2Environment, SpikeError> {
    // Spike-only profile directory; product code would use a per-user,
    // per-theme location chosen by the WIN-* host tickets.
    let data_dir = std::env::temp_dir()
        .join("glint-spike-windows-appbar")
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

fn create_view(
    env: &ICoreWebView2Environment,
    parent: HWND,
    role: Role,
) -> Result<View, SpikeError> {
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

    configure_settings(&webview).map_err(failure("ICoreWebView2Settings"))?;
    install_policy(env, &webview, role).map_err(failure("install policy handlers"))?;
    install_events(&webview, role).map_err(failure("install event handlers"))?;

    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(parent, &mut rect);
        controller.SetBounds(rect).map_err(failure("SetBounds"))?;
        controller
            .SetIsVisible(true)
            .map_err(failure("SetIsVisible"))?;
        let url = HSTRING::from(format!("{ORIGIN}{}", role.page()));
        webview.Navigate(&url).map_err(failure("Navigate"))?;
    }
    Ok(View {
        controller,
        webview,
    })
}

fn configure_settings(webview: &ICoreWebView2) -> windows::core::Result<()> {
    let dev = cfg!(debug_assertions);
    unsafe {
        let settings = webview.Settings()?;
        // DevTools stay available in debug builds only.
        settings.SetAreDevToolsEnabled(dev)?;
        settings.SetAreDefaultContextMenusEnabled(false)?;
        settings.SetIsStatusBarEnabled(false)?;
        settings.SetAreHostObjectsAllowed(false)?;
        settings.SetIsZoomControlEnabled(false)?;
        settings.SetAreDefaultScriptDialogsEnabled(false)?;
        settings.SetIsBuiltInErrorPageEnabled(false)?;
        settings.SetIsWebMessageEnabled(true)?;
        if let Ok(settings) = settings.cast::<ICoreWebView2Settings3>() {
            settings.SetAreBrowserAcceleratorKeysEnabled(dev)?;
        }
        if let Ok(settings) = settings.cast::<ICoreWebView2Settings4>() {
            settings.SetIsPasswordAutosaveEnabled(false)?;
            settings.SetIsGeneralAutofillEnabled(false)?;
        }
    }
    Ok(())
}

/// Navigation and content policy: only the bundled origin, served from memory.
fn install_policy(
    env: &ICoreWebView2Environment,
    webview: &ICoreWebView2,
    role: Role,
) -> windows::core::Result<()> {
    let mut token = 0i64;
    {
        for frame in [false, true] {
            let handler = NavigationStartingEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut uri = PWSTR::null();
                unsafe { args.Uri(&mut uri)? };
                let uri = take_pwstr(uri);
                if !logic::is_bundled_url(&uri) {
                    unsafe { args.SetCancel(true)? };
                    let kind = if frame { "frame" } else { "top-level" };
                    log(&format!(
                        "{}: blocked {kind} navigation to {uri}",
                        role.name()
                    ));
                    metrics(|m| {
                        m.blocked_navigations
                            .push(format!("{} {kind} {uri}", role.name()))
                    });
                }
                Ok(())
            }));
            if frame {
                unsafe { webview.add_FrameNavigationStarting(&handler, &mut token)? };
            } else {
                unsafe { webview.add_NavigationStarting(&handler, &mut token)? };
            }
        }

        let handler = NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut uri = PWSTR::null();
            unsafe {
                args.Uri(&mut uri)?;
                // Handled without supplying a NewWindow: nothing opens.
                args.SetHandled(true)?;
            }
            let uri = take_pwstr(uri);
            log(&format!("{}: blocked new window {uri}", role.name()));
            metrics(|m| m.blocked_new_windows.push(format!("{} {uri}", role.name())));
            Ok(())
        }));
        unsafe { webview.add_NewWindowRequested(&handler, &mut token)? };

        let handler = PermissionRequestedEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)? };
                metrics(|m| m.denied_permissions += 1);
            }
            Ok(())
        }));
        unsafe { webview.add_PermissionRequested(&handler, &mut token)? };

        // Every request is intercepted: bundled assets come from memory with
        // a CSP header, everything else gets a 403 without touching the network.
        unsafe {
            webview.AddWebResourceRequestedFilter(w!("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?
        };
        let env = env.clone();
        let handler = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut uri = PWSTR::null();
            unsafe { args.Request()?.Uri(&mut uri)? };
            let uri = take_pwstr(uri);
            let (status, reason, body, headers) = match logic::asset_for_url(&uri) {
                Some(asset) => {
                    metrics(|m| m.served_requests += 1);
                    (
                        200,
                        "OK",
                        asset.body,
                        format!(
                            "Content-Type: {}\r\nContent-Security-Policy: {}\r\n\
                                     X-Content-Type-Options: nosniff\r\nCache-Control: no-store",
                            asset.mime,
                            logic::CSP
                        ),
                    )
                }
                None => {
                    log(&format!("{}: refused request {uri}", role.name()));
                    metrics(|m| m.blocked_requests.push(format!("{} {uri}", role.name())));
                    let (status, reason) = if logic::is_bundled_url(&uri) {
                        (404, "Not Found")
                    } else {
                        (403, "Forbidden")
                    };
                    (
                        status,
                        reason,
                        "",
                        "Content-Type: text/plain; charset=utf-8".to_owned(),
                    )
                }
            };
            let stream = unsafe { SHCreateMemStream(Some(body.as_bytes())) };
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
    }
    Ok(())
}

fn install_events(webview: &ICoreWebView2, role: Role) -> windows::core::Result<()> {
    let mut token = 0i64;
    {
        let handler = NavigationCompletedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut success = BOOL(0);
            let mut status = COREWEBVIEW2_WEB_ERROR_STATUS(0);
            unsafe {
                args.IsSuccess(&mut success)?;
                args.WebErrorStatus(&mut status)?;
            }
            let result = NavResult {
                ms: ms(),
                success: success.as_bool(),
                web_error_status: status.0,
            };
            log(&format!(
                "{}: NavigationCompleted success={} status={}",
                role.name(),
                result.success,
                result.web_error_status
            ));
            metrics(|m| {
                m.navigation.entry(role).or_insert(result);
            });
            Ok(())
        }));
        unsafe { webview.add_NavigationCompleted(&handler, &mut token)? };

        let handler = WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut source = PWSTR::null();
            unsafe { args.Source(&mut source)? };
            let source = take_pwstr(source);
            if !logic::is_bundled_url(&source) {
                metrics(|m| m.rejected_messages += 1);
                return Ok(());
            }
            let mut raw = PWSTR::null();
            if unsafe { args.TryGetWebMessageAsString(&mut raw) }.is_err() {
                metrics(|m| m.rejected_messages += 1);
                return Ok(());
            }
            let raw = take_pwstr(raw);
            let Some(message) = HostMessage::parse(&raw) else {
                metrics(|m| m.rejected_messages += 1);
                return Ok(());
            };
            metrics(|m| m.messages.push(format!("{}:{raw}", role.name())));
            let Some(app) = app() else { return Ok(()) };
            match message {
                HostMessage::ToggleControlCenter => set_control_center(&app, None),
                HostMessage::CloseControlCenter => set_control_center(&app, Some(false)),
                HostMessage::Quit => unsafe {
                    let _ = PostMessageW(Some(app.appbar), WM_CLOSE, WPARAM(0), LPARAM(0));
                },
                HostMessage::CspViolation(detail) => {
                    log(&format!("{}: CSP violation {detail}", role.name()));
                    metrics(|m| m.csp_violations.push(format!("{} {detail}", role.name())));
                }
                HostMessage::PanelReady | HostMessage::ControlCenterReady => {}
            }
            Ok(())
        }));
        unsafe { webview.add_WebMessageReceived(&handler, &mut token)? };
    }
    Ok(())
}

fn execute_script(app: &App, role: Role, script: &str) {
    if let Ok(slot) = app.view(role).try_borrow()
        && let Some(view) = slot.as_ref()
    {
        let handler = ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(())));
        let _ = unsafe { view.webview.ExecuteScript(&HSTRING::from(script), &handler) };
    }
}

// -------------------------------------------------------------- smoke ----

/// Probes run inside the panel: a remote script (must be blocked by CSP), a
/// remote popup (must be blocked by NewWindowRequested), the real toggle
/// button (exercises postMessage -> host) and a remote navigation (must be
/// cancelled by NavigationStarting).
const SMOKE_OPEN_SCRIPT: &str = r#"(() => {
  const s = document.createElement('script');
  s.src = 'https://example.com/glint-spike-remote.js';
  document.head.appendChild(s);
  window.open('https://example.com/glint-spike-popup');
  document.getElementById('cc-toggle').click();
  setTimeout(() => { location.href = 'https://example.com/glint-spike-navigation'; }, 250);
})();"#;

fn on_smoke_timer(app: &App, id: usize) {
    unsafe {
        let _ = KillTimer(Some(app.appbar), id);
    }
    match id {
        TIMER_SMOKE_OPEN => {
            log("smoke: probes + click panel toggle");
            execute_script(app, Role::Panel, SMOKE_OPEN_SCRIPT);
        }
        TIMER_SMOKE_CLOSE => {
            log("smoke: click control-center Close button");
            execute_script(
                app,
                Role::ControlCenter,
                "document.getElementById('close').click();",
            );
        }
        TIMER_SMOKE_END => {
            let mut rect = RECT::default();
            unsafe {
                let _ = GetWindowRect(app.appbar, &mut rect);
            }
            let memory = sample_memory(app);
            let window_dpi = unsafe { GetDpiForWindow(app.appbar) };
            metrics(|m| {
                m.work_area_during = work_area();
                m.window_rect = Some(to_rect(rect));
                m.window_dpi = window_dpi;
                m.memory = memory;
            });
            if remove_appbar() {
                log("smoke: ABM_REMOVE sent");
            }
            unsafe {
                SetTimer(Some(app.appbar), TIMER_SMOKE_AFTER_REMOVE, 750, None);
            }
        }
        TIMER_SMOKE_AFTER_REMOVE => {
            metrics(|m| m.work_area_after = work_area());
            let passed = print_report(app);
            metrics(|m| m.smoke_passed = Some(passed));
            unsafe {
                let _ = DestroyWindow(app.appbar);
            }
        }
        _ => {}
    }
}

fn memory_of(process: HANDLE) -> Option<(usize, usize)> {
    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    unsafe { K32GetProcessMemoryInfo(process, &mut counters, counters.cb) }
        .as_bool()
        .then_some((counters.WorkingSetSize, counters.PagefileUsage))
}

fn sample_memory(app: &App) -> Vec<String> {
    const MIB: f64 = 1024.0 * 1024.0;
    let mut lines = Vec::new();
    let mut total_ws = 0usize;
    let mut total_private = 0usize;
    if let Some((ws, private)) = memory_of(unsafe { GetCurrentProcess() }) {
        total_ws += ws;
        total_private += private;
        lines.push(format!(
            "host pid={} working_set={:.1}MiB private={:.1}MiB",
            std::process::id(),
            ws as f64 / MIB,
            private as f64 / MIB
        ));
    }
    let env = app.env.borrow().clone();
    let infos = env
        .and_then(|env| env.cast::<ICoreWebView2Environment8>().ok())
        .and_then(|env| unsafe { env.GetProcessInfos() }.ok());
    if let Some(infos) = infos {
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
            let sample =
                unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid as u32) }
                    .ok()
                    .and_then(|handle| {
                        let sample = memory_of(handle);
                        let _ = unsafe { CloseHandle(handle) };
                        sample
                    });
            match sample {
                Some((ws, private)) => {
                    total_ws += ws;
                    total_private += private;
                    lines.push(format!(
                        "webview2 {kind} pid={pid} working_set={:.1}MiB private={:.1}MiB",
                        ws as f64 / MIB,
                        private as f64 / MIB
                    ));
                }
                None => lines.push(format!("webview2 {kind} pid={pid} memory=unavailable")),
            }
        }
    } else {
        lines.push("webview2 process list unavailable (ICoreWebView2Environment8)".to_owned());
    }
    lines.push(format!(
        "total working_set={:.1}MiB private={:.1}MiB (working sets double-count shared pages)",
        total_ws as f64 / MIB,
        total_private as f64 / MIB
    ));
    lines
}

/// Prints observations and returns whether every check passed. Only facts the
/// process could observe are reported; visual rendering is not verifiable here.
fn print_report(app: &App) -> bool {
    let expected_height = logic::scale(logic::PANEL_HEIGHT_DIP, app.dpi.get());
    METRICS.with(|m| {
        let m = m.borrow();
        let out = |key: &str, value: String| println!("smoke.{key} = {value}");
        out("webview2_runtime", m.runtime_version.clone());
        out("webview2_environment_version", m.environment_version.clone());
        out("dpi_awareness", m.dpi_awareness.clone());
        out("monitor_dpi", app.dpi.get().to_string());
        out("window_dpi", m.window_dpi.to_string());
        out("primary_monitor_rect", fmt_rect(Some(app.monitor_rect.get())));
        out("registered_rect", fmt_rect(m.registered_rect));
        out("window_rect", fmt_rect(m.window_rect));
        out("work_area_before", fmt_rect(m.work_area_before));
        out("work_area_during", fmt_rect(m.work_area_during));
        out("work_area_after_remove", fmt_rect(m.work_area_after));
        out("appbar_registered_ms", format!("{:?}", m.registered_ms));
        out("environment_ready_ms", format!("{:?}", m.environment_ms));
        for (role, nav) in &m.navigation {
            out(
                &format!("navigation_completed.{}", role.name()),
                format!(
                    "{}ms success={} web_error_status={}",
                    nav.ms, nav.success, nav.web_error_status
                ),
            );
        }
        out("served_requests", m.served_requests.to_string());
        out("messages", m.messages.join(" | "));
        out("rejected_messages", m.rejected_messages.to_string());
        out("csp_violations", m.csp_violations.join(" | "));
        out("blocked_navigations", m.blocked_navigations.join(" | "));
        out("blocked_new_windows", m.blocked_new_windows.join(" | "));
        out("blocked_requests", m.blocked_requests.join(" | "));
        out("denied_permissions", m.denied_permissions.to_string());
        out(
            "popup",
            format!(
                "opened={} closed={} visible_after_open={}",
                m.popup_opened, m.popup_closed, m.popup_visible_after_open
            ),
        );
        out(
            "shell_events",
            format!(
                "ABN_POSCHANGED={} ABN_FULLSCREENAPP={} TaskbarCreated={}",
                m.poschanged_events, m.fullscreen_events, m.taskbar_created_events
            ),
        );
        for line in &m.memory {
            out("memory", line.clone());
        }

        let monitor = app.monitor_rect.get();
        let nav_ok = |role| m.navigation.get(&role).is_some_and(|n| n.success);
        let checks = [
            (
                "appbar docked at top of primary monitor",
                m.registered_rect.is_some_and(|r| {
                    r.top >= monitor.top && r.height() == expected_height && r.width() > 0
                }),
            ),
            (
                "work area reserved while registered",
                matches!((m.work_area_during, m.registered_rect), (Some(w), Some(r)) if w.top >= r.bottom),
            ),
            (
                "work area restored after ABM_REMOVE",
                m.work_area_before.is_some() && m.work_area_after == m.work_area_before,
            ),
            ("panel navigation completed", nav_ok(Role::Panel)),
            (
                "control-center navigation completed",
                nav_ok(Role::ControlCenter),
            ),
            (
                "panel and control center scripts ran",
                m.messages.iter().any(|s| s == "panel:panel-ready")
                    && m.messages.iter().any(|s| s == "control-center:control-center-ready"),
            ),
            (
                "popup toggled open and closed via postMessage",
                m.popup_opened >= 1 && m.popup_closed >= 1 && m.popup_visible_after_open,
            ),
            (
                "remote script blocked by CSP",
                m.csp_violations.iter().any(|v| v.contains("example.com")),
            ),
            (
                "remote navigation cancelled",
                m.blocked_navigations.iter().any(|v| v.contains("example.com")),
            ),
            (
                "remote new window blocked",
                m.blocked_new_windows.iter().any(|v| v.contains("example.com")),
            ),
        ];
        let mut passed = true;
        for (name, ok) in checks {
            println!("smoke.check {} {name}", if ok { "PASS" } else { "FAIL" });
            passed &= ok;
        }
        println!("smoke.result = {}", if passed { "PASS" } else { "FAIL" });
        passed
    })
}
