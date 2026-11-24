//! Pack the bundled default theme (`themes/default`) into `OUT_DIR` with the
//! deterministic `glint-package` writer, so `src/default_theme.rs` can embed
//! it with `include_bytes!` (CLI-002). The archive bytes depend only on the
//! theme's files, so every build of the same sources embeds the same digest,
//! and `glint safe-mode` never depends on the repository or the store.

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
