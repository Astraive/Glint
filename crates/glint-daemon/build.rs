//! Pack the bundled default theme (`themes/default`) into `OUT_DIR` with the
//! deterministic `glint-package` writer, so the daemon can install or repair
//! it for `theme-use default` and `safe-mode` requests (CLI-006). The bytes
//! depend only on the theme's files, so the daemon embeds exactly what the
//! CLI embeds (`apps/cli/build.rs`) and both agree on its digest.

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let theme = manifest_dir.join("../../themes/default");
    let out =
        PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR")).join("default-theme.glint");
    println!("cargo:rerun-if-changed={}", theme.display());
    println!("cargo:rerun-if-changed=build.rs");
    if let Err(error) = glint_package::pack_directory(&theme, &out) {
        panic!(
            "cannot pack the bundled default theme {}: {error}",
            theme.display()
        );
    }
}
