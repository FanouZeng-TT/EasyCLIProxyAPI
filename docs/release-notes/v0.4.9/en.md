## Codex model context capacity rule update

- When the upstream reports a context limit, the effective window is `min(upstream limit, max(350K, user configuration))`: 350K by default, larger values are allowed but never exceed the upstream limit, and a smaller upstream limit wins.
- When the upstream reports no limit, the 350K cap no longer applies: the default is 250K and the user configuration is used as-is, so 500K, 1M and other sizes can be configured; actual upstream support still depends on the backend.
- Raising the context window in the editor now also raises the maximum context window, so the two fields can no longer block saving on the mutual validation.
- Existing saved customizations are preserved, and the auto-compact threshold never exceeds the effective context window.

## Companion plugin note

- This release publishes CPA GUI 0.4.9 only and does not include a new ByteProxy plugin release.
- Older ByteProxy plugins report unknown capacity as 350K, which the GUI cannot distinguish from a real limit. Preserving large custom windows for unknown-capacity models therefore still requires the companion plugin update that retains unknown capacity; plugins remain distributed separately through the internal store.

## Validation

- Context and catalog synchronization regression tests passed, along with frontend type checking.
- Existing full-suite failures caused by the local symlink test environment were reproduced on the unmodified baseline and are unrelated to this change.
- The local proxy was not restarted; live-client end-to-end validation was not performed.
