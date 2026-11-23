# Practical workflows — consumer, creator, curator and support

All commands illustrative, pending CLI implementation.

## 1. User imports a theme without internet

```sh
glint theme import ~/Downloads/mono.glint
glint theme info dev.example.mono
glint theme use dev.example.mono
glint theme list
```

Expected: archive validated locally; permissions presented; theme preview available; chosen surfaces display; no Git or account required. If `system.network.read` is unavailable, relevant widgets render placeholder/disabled state.

## 2. User installs from arbitrary repository

```sh
glint theme install https://github.com/creator/glint-midnight.git
glint theme use dev.creator.midnight
```

Expected: source resolver handles Git, locates root `glint.json`, pins commit, validates package and tracks origin. No `glint-library` PR necessary.

## 3. User registers their own library

```sh
glint registry add custom https://github.com/creator/glint-collection.git
glint registry sync custom
glint theme search "midnight" --registry custom
glint theme install custom/midnight
```

Expected: custom `registry.json` indexed independently; remains distinct from official.

## 4. User mixes themes

```sh
glint profile create my-setup --base official/mono
glint profile set-surface my-setup dock community/glassdock
glint profile set-surface my-setup notification local/simple
glint profile use my-setup
```

Expected: profile resolver applies explicit precedence and locks versions, without changing unrelated surfaces; command syntax is proposed.

## 5. Developer creates React theme

```sh
glint theme create nova --template react
cd nova
glint theme dev            # sandbox and simulated data
glint theme build
glint theme validate
glint theme pack -o nova.glint
```

Expected: `nova.glint` is standalone and can be shared without GitHub.

## 6. Developer tests OS differences

```sh
glint doctor --json
glint theme preview ./nova.glint --platform windows
glint theme preview ./nova.glint --platform linux:hyprland
glint theme preview ./nova.glint --platform macos
```

Expected: previews emulate capabilities; actual support must still be tested on hardware/VMs. Rendering a mock does not certify native integration.

## 7. Recover after theme failure

```sh
glint safe-mode
glint theme use default
glint doctor --redact
```

Expected: disable third-party code, preserve native OS shell and re-open manager. On profile errors, restore journal state and report anything not reversible.

## 8. Curator contributes to official library

- Fork `glint-library`; add theme under `themes/<name>/`.
- Run `glint theme validate` and tests; include screenshots, author/license/permissions/OS support.
- Open PR; CI regenerates/checks `registry.json`, package checksums and manifests.
- Reviewer verifies claims; merge adds it to the **default public catalog**, not to exclusive distribution rights.
