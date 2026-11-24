//! Build script for the Glint manager host (MGR-001).
//!
//! On Windows it runs `tauri-build` with an app manifest that names the
//! only two commands, so each needs an explicit permission
//! (`allow-daemon-status`, `allow-daemon-ping`) in `capabilities/`.
//!
//! `tauri::generate_context!` embeds `../dist`, the built web UI. When it has
//! not been built (a plain `cargo clippy --workspace` on CI), the crate is
//! compiled with `cfg(glint_desktop_no_ui)` instead: everything is checked
//! except the asset embedding, and the binary says how to build the UI.

use std::path::Path;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(glint_desktop_no_ui)");
    println!("cargo::rerun-if-changed=../dist/index.html");
    if std::env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }
    if !Path::new("../dist/index.html").is_file() {
        println!("cargo::rustc-cfg=glint_desktop_no_ui");
        println!(
            "cargo::warning=apps/desktop/dist is missing; glint-desktop is built without its UI \
             (run `corepack pnpm@10.18.0 --filter @glint/desktop build` first)"
        );
    }
    #[cfg(windows)]
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new().commands(&["daemon_status", "daemon_ping"]),
        ),
    )
    .expect("tauri-build failed");
}
