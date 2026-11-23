# Sources, install URL rules, Git strategy and offline packages

## Source types

```rust
pub enum ThemeSource {
    Registry { name: String, package: String, version: Option<String> },
    Git { url: String, reference: Option<String>, subpath: Option<String> },
    LocalArchive { path: PathBuf },
    LocalDirectory { path: PathBuf },
    ArtifactUrl { url: String, sha256: Option<String> },
}
```

These values are illustrative; in production `url` parsing must restrict schemes and refuse dangerous local protocol tricks. `git+https` and `https` artifact links should remain unambiguous.

## Theme source URL examples

- `official/mono` — resolve via bundled official registry.
- `community/carbon@1.4.2` — resolve via registered custom index.
- `https://github.com/user/glint-midnight.git` — root theme Git repository.
- `https://github.com/user/themes.git#v1.0.0:themes/midnight` — proposed ref/subpath syntax, **not final public syntax**.
- `~/Downloads/midnight.glint` — offline archive.
- `./my-theme/` — local developer directory; should not auto-run build in normal user mode.
- `https://example.org/midnight.glint` — optional direct artifact install, verify content before unpacking.

## Git download approach

**Discovery:** fetch only small index or manifest metadata. Use Git host raw/content/archive endpoints where appropriate, with caching and ETag/Last-Modified as available. **Installation:** sparse/partial clone of a chosen theme directory where repo is large; pin requested commit, resolve Git LFS assets deliberately and verify file hashes. **Caching:** reuse a per-source bare/object cache or sparse checkout with locks and cleanup. Avoid one clone per installed theme if Git history is large.

Conceptual command for an official multi-theme Git repo:

```sh
git clone --depth 1 --filter=blob:none --sparse \
  https://github.com/YOUR_ORG/glint-library.git glint-library-cache
cd glint-library-cache
git sparse-checkout set themes/mono
```

This needs a local Git tool or linked Git implementation if used directly. The recommended **consumer experience must not require Git CLI**; Glint can vendor/ship an implementation or use downloadable prebuilt archives. Partial clone optimization depends on server support; fall back safely.

## Offline import/export

- ZIP-based `.glint` is self-contained and independently usable.
- Exported package includes every referenced asset/compiled surface but excludes personal local config, private credentials, auth tokens and unlicensed external assets.
- Import from filesystem, drag/drop GUI, double-click association, and CLI.
- No network lookup needed to preview or apply the file. Remote `updateUrl` may be used for optional update checks later.

## Source provenance

Store source kind, source URL, Git commit SHA or artifact digest, registry identifier, package ID/version, download time, verified archive digest, signing status and local install path. Never assume identical package IDs from different origins are trustworthy duplicates.

## Update policy

- Default recommended: **manual or opt-in updates**, no silent remote code replacement.
- Registry installations: compare index versions, lock digest and fetch new version to staging.
- Git installations: pin commit; checking branch head != changing installed version.
- Local files: no upstream by default, even if manifest contains an author URL.
- On upgrade, show new permissions and compatibility differences, allow preview and rollback.

## Network/privacy

Avoid sending an exhaustive installed-theme list to external registries. Requests only necessary for source metadata or downloads. Support offline mode, private Git credentials through system credential manager (never embed in `.glint`) and origin-specific network access.
