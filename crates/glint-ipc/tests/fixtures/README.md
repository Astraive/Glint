# glint-ipc golden fixtures

Golden JSON for the *proposed* local IPC protocol, API version 1 (COR-003,
[ADR 0010](../../../../docs/adr/0010-interprocess-for-local-ipc.md)). Each
file is one frame body (the JSON after the 4-byte length prefix).
`tests/golden.rs` serializes the typed value and compares it with the file,
then decodes the file and compares the result with the typed value, so a
change to the wire format fails until the fixture is updated on purpose.

Provenance: written for this repository; synthetic values only (the pid,
sequence numbers and messages are made up, no user data). License: MIT, as
the repository.
