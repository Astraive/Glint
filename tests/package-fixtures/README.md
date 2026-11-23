# Package fixtures

Synthetic test inputs for the Glint package and manifest parsers.

| Fixture | Ticket | Purpose |
|---|---|---|
| `valid-minimal/glint.json` | SCH-001 | Smallest theme manifest the proposed v1 parser accepts: exactly the fields `schemas/glint-theme-v1.schema.json` marks as required |
| `manifests/valid/`, `manifests/invalid/` | SCH-002 | Golden manifest corpus shared by the Rust parser (`cargo test -p glint-schema --test fixtures`) and the TypeScript validator (`corepack pnpm@10.18.0 --filter @glint/theme-schema test`) |
| `manifests/expectations.json` | SCH-002 | Expected warnings for every valid fixture and the expected error code and JSON Pointer for every invalid fixture, plus the full error and warning code lists |
| `surfaces/cases.json` | SRF-001 | Golden surface definition cases (`surfaces.<id>` entries) with expected resolved definitions or error code and JSON Pointer, run by `cargo test -p glint-surfaces --test surface_manifest` and checked against the generated types by `corepack pnpm@10.18.0 --filter @glint/types test` |
| `manifests/version-strings.json` | SCH-002 | SemVer versions and requirements both validators must accept or reject identically |

Invalid fixtures that are not well-formed JSON use the `.txt` extension so
the repository JSON check does not treat them as broken files. Every file in
`manifests/valid/` and `manifests/invalid/` must have exactly one entry in
`expectations.json`; both test suites enforce this.

Provenance: every file here is written by hand for this repository and
contains no personal data, third-party trademarks, fonts or images. It is
covered by the repository [MIT license](../../LICENSE). The manifest format is
*proposed* and these fixtures change with it.
