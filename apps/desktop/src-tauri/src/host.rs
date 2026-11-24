//! The Tauri window and its command set (Windows only).
//!
//! Exactly two commands exist, both read-only and both allow-listed for the
//! `main` window in `capabilities/main.json` through the app manifest in
//! `build.rs`; no Tauri core plugin permission is granted. IPC calls block,
//! so each command runs on a blocking worker, never on the UI thread.

// Without the embedded UI (see build.rs) the window is never built, so the
// commands are compiled and linted but unused.
#![cfg_attr(glint_desktop_no_ui, allow(dead_code, unused_imports))]

use std::process::ExitCode;

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::bridge::{Bridge, DaemonStatus, PingResult, Problem};
use crate::navigation::is_app_origin;

/// `get-status` and `get-snapshot` from the core, or why not.
#[tauri::command]
async fn daemon_status(app: AppHandle) -> DaemonStatus {
    let bridge = app.state::<Bridge>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || bridge.status())
        .await
        .unwrap_or_else(|error| DaemonStatus::Unreachable {
            problem: Problem::bridge_failed(error.to_string()),
        })
}

/// `ping` the core, or why not.
#[tauri::command]
async fn daemon_ping(app: AppHandle) -> PingResult {
    let bridge = app.state::<Bridge>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || bridge.ping())
        .await
        .unwrap_or_else(|error| PingResult::Unreachable {
            problem: Problem::bridge_failed(error.to_string()),
        })
}

#[cfg(not(glint_desktop_no_ui))]
fn context() -> tauri::Context<tauri::Wry> {
    tauri::generate_context!()
}

/// Open the manager window and run until it closes.
pub fn run() -> ExitCode {
    #[cfg(glint_desktop_no_ui)]
    {
        eprintln!(
            "glint-desktop: built without the web UI (apps/desktop/dist was missing at compile \
             time); run `corepack pnpm@10.18.0 --filter @glint/desktop build` and rebuild"
        );
        ExitCode::FAILURE
    }
    #[cfg(not(glint_desktop_no_ui))]
    {
        let result = tauri::Builder::default()
            .manage(Bridge::from_env())
            .invoke_handler(tauri::generate_handler![daemon_status, daemon_ping])
            .setup(|app| {
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title("Glint")
                    .inner_size(1100.0, 720.0)
                    .min_inner_size(720.0, 480.0)
                    .on_navigation(|url| is_app_origin(url.scheme(), url.host_str()))
                    .build()?;
                Ok(())
            })
            .run(context());
        match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("glint-desktop: {error}");
                ExitCode::FAILURE
            }
        }
    }
}
