## Codex model context configuration

- Model List Settings and Update Configuration now use the same context policy.
- When upstream capacity is known, the effective context limit is the minimum of **350K, upstream capacity, and the user-configured value**.
- When upstream capacity is unknown, custom values are preserved without a 350K cap. Values such as 500K or 1M can be configured, but upstream support is not guaranteed.
- Unconfigured models default to **250K tokens**, reduced further when a known upstream limit is smaller.
- Saved model customizations are preserved. Existing Codex global context settings are synchronized with the selected model's effective capacity; automatic compaction thresholds cannot exceed that capacity.

## Companion plugin requirement

- This GitHub release publishes **CPA GUI 0.4.6** only; it does not include a new ByteProxy plugin release.
- Older ByteProxy plugins report unknown capacity as 350K. The GUI cannot distinguish this fallback from a real limit. Preserving larger custom limits for unknown-capacity models requires a companion plugin update that retains unknown capacity; plugins remain distributed separately through the internal store.
- Per-response output limits are unchanged. Image and video generation are not added.

## Validation

- All 42 context and synchronization regression tests passed, along with frontend type checking.
- Five existing full-suite failures were reproduced on the unmodified baseline, involving an external local CLI, symlink test environment, and customized download URL assertions.
- The local proxy was not restarted; live-client end-to-end validation was not performed.
