## Upstream core sync with all customizations preserved

- Ships **CPA GUI 0.4.8 with CPA core 8.0.54** in matching installation packages.
- The core merges every upstream v8.0.52 update, including multi-model and multi-auth plugin host registration, request/response pipeline changes, the `MoeForge/moego` dependency migration, the Go 1.26 build chain, and schema v10.
- Preserved customizations: plugin-owned public API routes, standalone Codex `apply_patch` declaration with model prefixes, `/v1/models` capability fields, Claude Desktop ByteDance plugin role routing, preconfigured internal store with automatic updates, our own download sources, and ad-hoc macOS packaging with Ed25519 update signing when no Apple certificate is available.
- Codex context policy is unchanged: 250K by default, known capacity clamped to at most 350K, and user values kept for unknown capacity.

## Validation

- Core `go test ./...` passed serially, along with `go vet` and race checks on the key packages.
- GUI production frontend build and Rust release build passed. Rust full suite had 1035 passing and 4 failing, and the frontend had 15 failing; all of these reproduce unchanged on the upstream baseline and are not introduced by this merge.
- No live request replay was performed; build and regression validation is not end-to-end acceptance.

## Plugin delivery

- The ByteDance model plugin is still released separately through the internal store; its new version is not included in this package.
