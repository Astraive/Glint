//! Typegen drift check (SRF-001): `packages/types/src/surfaces.ts` must equal
//! the output of `glint_surfaces::typescript_module()`.
//!
//! Regenerate with `GLINT_UPDATE_TYPES=1 cargo test -p glint-surfaces --test typegen`.

use std::path::PathBuf;

use glint_surfaces::{TYPESCRIPT_PATH, typescript_module};

#[test]
fn committed_typescript_matches_the_generator() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(TYPESCRIPT_PATH);
    let generated = typescript_module();
    if std::env::var_os("GLINT_UPDATE_TYPES").is_some() {
        std::fs::write(&path, &generated).expect("write generated types");
        return;
    }
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    if committed != generated {
        let line = committed
            .lines()
            .zip(generated.lines())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| committed.lines().count().min(generated.lines().count()));
        panic!(
            "DRIFT: {TYPESCRIPT_PATH} is out of date with crates/glint-surfaces (first difference \
             at line {}). Run: GLINT_UPDATE_TYPES=1 cargo test -p glint-surfaces --test typegen",
            line + 1
        );
    }
}

#[test]
fn generated_module_names_every_kind_and_code() {
    let text = typescript_module();
    for kind in glint_surfaces::StandardSurface::ALL {
        assert!(text.contains(&format!("\"{kind}\"")), "{kind}");
    }
    for code in glint_surfaces::SurfaceErrorCode::ALL {
        assert!(text.contains(code.as_str()), "{code}");
    }
    assert!(text.contains("export type CustomSurfaceId = `custom:${string}.${string}`;"));
    assert!(text.ends_with(";\n"));
}
