#!/usr/bin/env python3
"""Enforce the crate dependency boundaries of docs/architecture/module-boundaries.md.

Self-contained, Python 3.12 standard library only. Reads `cargo metadata
--no-deps` (offline, `--locked`) and checks every workspace member's declared
dependencies (normal, dev, build and target-gated alike):

  R1  spike-never-imported   Product crates (crates/*, apps/*) must not depend
                             on a spikes/* crate, directly or through other
                             workspace crates.
  R2  core-no-adapter        Platform-independent crates (every crates/* member
                             except the adapters glint-windows, glint-linux and
                             glint-macos) must not depend on an adapter,
                             directly or through other workspace crates.
  R3  core-no-os-binding     Platform-independent crates must not depend
                             directly on an OS-binding crate (windows*, winapi*,
                             objc2*, cocoa, gtk*, webkit2gtk*, wayland-*, ...).
  R4  app-no-os-binding      apps/* stay thin: no direct OS-binding crate.
                             They reach the OS through adapter crates.

Rules are pattern-based on workspace paths, so new crates are covered without
editing this file. OS-binding rules are direct-only: a cross-platform
dependency such as `dirs` or `rusqlite` legitimately pulls `windows-sys` under
`cfg(windows)`, and flagging that would force a permanent allow-list. Workspace
edges are followed transitively, because an adapter or spike reached through an
intermediate crate is still a leak.

Usage: python scripts/check-crate-boundaries.py [--self-test]
"""
from __future__ import annotations

import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]

ADAPTERS = {"glint-windows", "glint-linux", "glint-macos"}

# Crates whose purpose is to bind to one operating system's API.
OS_BINDING = re.compile(
    r"^(windows([-_].+)?|winapi([-_].+)?|winreg|webview2-com([-_].+)?|"
    r"objc2?([-_].+)?|cocoa([-_].+)?|core-foundation([-_].+)?|core-graphics([-_].+)?|"
    r"gtk[0-9]*([-_].+)?|gdk[0-9]*([-_].+)?|webkit2gtk([-_].+)?|wayland-.+|smithay-.+|"
    r"x11([-_].+)?|xcb|xcb-.+|x11rb([-_].+)?|zbus|dbus([-_].+)?|"
    r"wry|tao)$"
)

# Explicit, reviewed exceptions: (workspace crate, dependency) -> reason.
# Covers both R3/R4 (OS-binding crate) and R2 (adapter). Keep this empty unless
# a ticket justifies an entry, and name that ticket in the reason. Example
# (not active):
#   ("glint-storage", "windows-sys"): "STO-123: needs LockFileEx; no portable crate",
ALLOW: dict[tuple[str, str], str] = {}


def classify(manifest_path: str, root: Path) -> str:
    """Return 'app', 'crate', 'spike' or 'other' from the member's directory."""
    try:
        rel = Path(manifest_path).resolve().relative_to(root.resolve())
    except ValueError:
        return "other"
    top = rel.parts[0] if rel.parts else ""
    return {"apps": "app", "crates": "crate", "spikes": "spike"}.get(top, "other")


def check(metadata: dict, root: Path = ROOT) -> list[str]:
    members = set(metadata.get("workspace_members", [])) or None
    pkgs = [p for p in metadata["packages"] if members is None or p["id"] in members]
    kind = {p["name"]: classify(p["manifest_path"], root) for p in pkgs}
    edges: dict[str, list[str]] = {p["name"]: [] for p in pkgs}
    problems: list[str] = []

    for p in pkgs:
        name, k = p["name"], kind[p["name"]]
        if k not in ("crate", "app"):
            continue
        is_core = k == "crate" and name not in ADAPTERS
        for dep in p["dependencies"]:
            dname = dep["name"]
            dpath = dep.get("path") or ""
            if dpath and classify(str(Path(dpath) / "Cargo.toml"), root) == "spike":
                problems.append(f"R1 spike-never-imported: {name} -> {dname} (path {dpath}) imports spike code")
            elif dname in kind:
                edges[name].append(dname)
            if (name, dname) in ALLOW:
                continue
            if OS_BINDING.match(dname):
                if is_core:
                    problems.append(
                        f"R3 core-no-os-binding: {name} -> {dname} is an OS-binding crate; "
                        f"move the code into glint-windows/linux/macos or add a justified ALLOW entry")
                elif k == "app":
                    problems.append(
                        f"R4 app-no-os-binding: {name} -> {dname} is an OS-binding crate; "
                        f"apps stay thin and use adapter crates")

    for p in pkgs:
        name, k = p["name"], kind[p["name"]]
        if k not in ("crate", "app"):
            continue
        seen = {name}
        stack = [(name, [name])]
        while stack:
            cur, path = stack.pop()
            for nxt in edges.get(cur, []):
                if nxt in seen:
                    continue
                seen.add(nxt)
                chain = path + [nxt]
                stack.append((nxt, chain))
                text = " -> ".join(chain)
                if kind[nxt] == "spike":
                    problems.append(f"R1 spike-never-imported: {text} ({nxt} is under spikes/)")
                elif k == "crate" and name not in ADAPTERS and nxt in ADAPTERS \
                        and (cur, nxt) not in ALLOW:
                    problems.append(
                        f"R2 core-no-adapter: {text} (core crate {name} reaches adapter {nxt})")
    return sorted(set(problems))


def load_metadata() -> dict:
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--no-deps"],
        cwd=ROOT, capture_output=True, text=True, encoding="utf-8")
    if out.returncode != 0:
        print(f"FAIL: cargo metadata exited {out.returncode}\n{out.stderr}", file=sys.stderr)
        sys.exit(2)
    return json.loads(out.stdout)


def main(argv: list[str]) -> int:
    if "--self-test" in argv:
        import unittest
        sys.path.insert(0, str(ROOT / "scripts" / "tests"))
        suite = unittest.defaultTestLoader.loadTestsFromName("test_crate_boundaries")
        return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1
    metadata = load_metadata()
    problems = check(metadata)
    n = len(metadata["workspace_members"])
    for line in problems:
        print(f"FAIL: {line}")
    if problems:
        print(f"{len(problems)} crate boundary violation(s) across {n} workspace members.")
        return 1
    print(f"PASS: crate boundaries hold across {n} workspace members.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
