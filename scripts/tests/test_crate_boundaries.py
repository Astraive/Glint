"""Synthetic cargo-metadata fixtures for scripts/check-crate-boundaries.py."""
import importlib.util
from pathlib import Path
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "check-crate-boundaries.py"
spec = importlib.util.spec_from_file_location("ccb", SCRIPT)
ccb = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ccb)

ROOT = Path("/ws")


def pkg(rel, name, deps=()):
    dependencies = []
    for d in deps:
        if isinstance(d, tuple):  # (renamed dependency name, path relative to ROOT)
            dependencies.append({"name": d[0], "kind": None, "target": None, "path": str(ROOT / d[1])})
        else:
            dependencies.append({"name": d, "kind": None, "target": None})
    return {"id": name, "name": name, "manifest_path": str(ROOT / rel / "Cargo.toml"),
            "dependencies": dependencies}


def run(*pkgs):
    metadata = {"packages": list(pkgs), "workspace_members": [p["id"] for p in pkgs]}
    return ccb.check(metadata, ROOT)


class Boundaries(unittest.TestCase):
    def test_clean_tree_passes(self):
        self.assertEqual(run(
            pkg("crates/glint-schema", "glint-schema", ["serde"]),
            pkg("crates/glint-storage", "glint-storage", ["dirs", "rusqlite", "glint-schema"]),
            pkg("crates/glint-windows", "glint-windows", ["windows", "glint-platform"]),
            pkg("crates/glint-platform", "glint-platform", ["glint-schema"]),
            pkg("apps/cli", "glint-cli", ["glint-schema", "glint-windows"]),
            pkg("spikes/a", "spike-a", ["windows"])), [])

    def test_core_direct_os_binding(self):
        for dep in ["windows", "windows-sys", "webview2-com", "objc2-app-kit", "cocoa",
                    "gtk4", "webkit2gtk", "wayland-client", "x11rb", "winapi"]:
            out = run(pkg("crates/glint-core", "glint-core", [dep]))
            self.assertEqual(len(out), 1, dep)
            self.assertIn(f"glint-core -> {dep}", out[0])

    def test_cross_platform_deps_not_flagged(self):
        self.assertEqual(run(pkg("crates/glint-storage", "glint-storage",
                                 ["dirs", "rusqlite", "tempfile", "windowsill"])), [])

    def test_core_to_adapter_direct_and_transitive(self):
        out = run(pkg("crates/glint-schema", "glint-schema", ["glint-linux"]),
                  pkg("crates/glint-linux", "glint-linux"))
        self.assertEqual(len(out), 1)
        self.assertIn("glint-schema -> glint-linux", out[0])
        out = run(pkg("crates/glint-installer", "glint-installer", ["glint-storage"]),
                  pkg("crates/glint-storage", "glint-storage", ["glint-macos"]),
                  pkg("crates/glint-macos", "glint-macos"))
        self.assertTrue(any("glint-installer -> glint-storage -> glint-macos" in x for x in out))

    def test_adapters_may_use_os_crates(self):
        self.assertEqual(run(pkg("crates/glint-macos", "glint-macos", ["objc2", "cocoa"])), [])

    def test_app_may_use_adapter_but_not_os_crate(self):
        self.assertEqual(run(pkg("apps/d", "d", ["glint-windows"]),
                             pkg("crates/glint-windows", "glint-windows")), [])
        out = run(pkg("apps/d", "d", ["windows"]))
        self.assertIn("R4", out[0])

    def test_spike_imports(self):
        out = run(pkg("crates/glint-x", "glint-x", ["spike-a"]), pkg("spikes/a", "spike-a"))
        self.assertTrue(any(x.startswith("R1") and "glint-x -> spike-a" in x for x in out))
        out = run(pkg("apps/d", "d", ["glint-x"]), pkg("crates/glint-x", "glint-x", ["spike-a"]),
                  pkg("spikes/a", "spike-a"))
        self.assertTrue(any("d -> glint-x -> spike-a" in x for x in out))

    def test_spike_by_renamed_path_dep(self):
        out = run(pkg("crates/glint-x", "glint-x", [("alias", "spikes/a")]), pkg("spikes/a", "spike-a"))
        self.assertTrue(any("glint-x -> alias" in x for x in out))

    def test_spikes_may_import_anything(self):
        self.assertEqual(run(pkg("spikes/a", "spike-a", ["glint-schema", "windows"]),
                             pkg("crates/glint-schema", "glint-schema")), [])

    def test_allow_list(self):
        ccb.ALLOW[("glint-core", "windows-sys")] = "test"
        try:
            self.assertEqual(run(pkg("crates/glint-core", "glint-core", ["windows-sys"])), [])
        finally:
            ccb.ALLOW.clear()


if __name__ == "__main__":
    unittest.main()
