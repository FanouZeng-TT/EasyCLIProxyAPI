## Standalone Codex apply_patch tool

- Ships **CPA GUI 0.4.7 with CPA core 8.0.40** in matching installation packages.
- The core adds plugin declarations `executor_apply_patch` and `executor_apply_patch_models` model prefixes. A model advertises `apply_patch_tool_type: freeform` only when every candidate executor supports it.
- The GUI synchronizes core-reported capabilities to the managed Codex model catalog, allowing compatible Codex clients to register the standalone `apply_patch` tool and display file edits. No Codex client source changes are required.
- Capability advertisement defaults to enabled for new installations and existing configurations without the setting. Explicit opt-outs remain disabled.

## Scope and plugin delivery

- This release delivers core capability forwarding and GUI configuration support. The ByteDance model plugin is released separately through the internal store; its new version is not included.
- Existing plugins that do not declare support retain their behavior. LLMBox and TRAE conversion support depends on the companion plugin's actual declarations and implementation. This package does not automatically enable unsupported models.
- Automatic catalog synchronization targets managed Codex configuration. Public `/v1/models` adds an optional capability field for supported models; other clients retain their request and response protocols and receive no automatically injected tools.

## Validation

- Regression checks cover configuration defaults, preserved opt-outs and Codex catalog synchronization; frontend type checking passed.
- Release builds use production web assets and package the matching core for each platform. The local proxy is not restarted for this release. Build and regression validation does not constitute end-to-end acceptance for every client.
