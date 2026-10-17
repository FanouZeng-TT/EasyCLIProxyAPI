use super::*;

// ByteProxy customization: preconfigure the plugin store and use an absolute plugin path.
// Existing store sources and user-defined absolute plugin directories remain unchanged.

pub(crate) const BYTEPROXY_PLUGIN_STORE_URL: &str =
    "https://lf0-fast-deliver-inner.bytedance.net/obj/eden-internal/2121eh7bvuhmnulg/byteproxy/registry.json";

pub(crate) const BYTEPROXY_CLAUDE_ROUTES: [&str; 3] = [
    CLAUDE_DESKTOP_OPUS_MODEL_ID,
    CLAUDE_DESKTOP_SONNET_MODEL_ID,
    CLAUDE_DESKTOP_HAIKU_MODEL_ID,
];

const LEGACY_BYTEPROXY_CLAUDE_ROUTES: [&str; 6] = [
    "claude-opus-4-5",
    "claude-sonnet-4-5",
    "claude-haiku-4-5",
    "claude-opus-4-5-cpa",
    "claude-sonnet-4-5-cpa",
    "claude-haiku-4-5-cpa",
];

/// 旧受管入口仍留在 YAML 里：内核配置更新不支持删除任意键，插件解析时会忽略它们。
fn has_legacy_byteproxy_claude_override(aliases: &serde_norway::Mapping) -> bool {
    LEGACY_BYTEPROXY_CLAUDE_ROUTES
        .iter()
        .any(|route| aliases.contains_key(yaml_key(route)))
}

pub(crate) fn is_byteproxy_model(model: &str, models: &[AgentModelOption]) -> bool {
    let model = model.trim();
    let is_byteproxy_label = |label: &str| {
        label.contains(" · LLMBox")
            || label.contains(" · TRAE")
            || label.contains("Claude 别名 →")
    };
    models.iter().any(|entry| {
        entry.name.eq_ignore_ascii_case(model)
            && entry.alias.as_deref().is_some_and(is_byteproxy_label)
    }) || (model.eq_ignore_ascii_case("auto")
        && models.iter().filter_map(|entry| entry.alias.as_deref()).any(is_byteproxy_label))
}

pub(crate) fn apply_byteproxy_claude_aliases(
    content: &str,
    mappings: &ClaudeDesktopModelMappings,
    models: &[AgentModelOption],
) -> Result<String, String> {
    let Some(entries) = mappings.desktop_models.as_ref() else {
        return Ok(content.to_string());
    };
    let managed = entries.iter().filter(|entry| is_byteproxy_model(&entry.model, models))
        .map(|entry| (entry.alias.trim(), entry.model.trim()))
        .collect::<Vec<_>>();
    let existing_aliases = serde_norway::from_str::<serde_norway::Value>(content)
        .ok()
        .and_then(|document| document.get("plugins")?.get("configs")?.get("byteproxy")?
            .get("claude_aliases")?.as_mapping().cloned())
        .unwrap_or_default();
    let has_managed_override = BYTEPROXY_CLAUDE_ROUTES
        .iter()
        .any(|route| existing_aliases.contains_key(yaml_key(route)))
        || has_legacy_byteproxy_claude_override(&existing_aliases);
    if managed.is_empty() && !has_managed_override {
        return Ok(content.to_string());
    }
    patch_core_yaml_document(content, |document| {
        let mut aliases = existing_aliases.clone();
        for route in BYTEPROXY_CLAUDE_ROUTES {
            aliases.insert(yaml_key(route), serde_norway::Value::String("auto".to_string()));
        }
        for (route, target) in &managed {
            if !BYTEPROXY_CLAUDE_ROUTES.iter().any(|allowed| allowed.eq_ignore_ascii_case(route)) {
                return Err(format!("Unsupported ByteProxy Claude route: {route}"));
            }
            aliases.insert(yaml_key(route), serde_norway::Value::String((*target).to_string()));
        }
        let mut changed = set_core_yaml_path_value(
            document,
            &["plugins", "configs", "byteproxy", "claude_alias_enabled"],
            serde_norway::Value::Bool(true),
        )?;
        changed |= set_core_yaml_path_value(
            document,
            &["plugins", "configs", "byteproxy", "claude_aliases"],
            serde_norway::Value::Mapping(aliases),
        )?;
        changed |= set_core_yaml_path_value(
            document,
            &["plugins", "configs", "byteproxy", "claude_context_window"],
            serde_norway::Value::Number(mappings.max_context_tokens.into()),
        )?;
        Ok(changed)
    }).map(|updated| updated.unwrap_or_else(|| content.to_string()))
}

fn yaml_string(value: &serde_norway::Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn plugin_store_sources(root: &serde_norway::Mapping) -> Vec<String> {
    root.get(yaml_key("plugins"))
        .and_then(serde_norway::Value::as_mapping)
        .and_then(|plugins| plugins.get(yaml_key("store-sources")))
        .and_then(serde_norway::Value::as_sequence)
        .map(|sources| {
            sources
                .iter()
                .filter_map(yaml_string)
                .collect::<Vec<String>>()
        })
        .unwrap_or_default()
}

fn plugin_dir_value(root: &serde_norway::Mapping) -> Option<String> {
    root.get(yaml_key("plugins"))
        .and_then(serde_norway::Value::as_mapping)
        .and_then(|plugins| plugins.get(yaml_key("dir")))
        .and_then(yaml_string)
}

/// 追加 ByteProxy 商店来源；插件目录缺失或为相对路径时改为绝对路径。
pub(crate) fn apply_byteproxy_kernel_defaults(
    content: &str,
    install_dir: &Path,
) -> Result<String, String> {
    patch_core_yaml_document(content, |document| {
        let (mut sources, current_dir) = {
            let root = document
                .as_mapping()
                .ok_or_else(|| "Kernel configuration root must be a YAML mapping".to_string())?;
            (plugin_store_sources(root), plugin_dir_value(root))
        };
        let mut changed = false;

        if !sources
            .iter()
            .any(|source| source.eq_ignore_ascii_case(BYTEPROXY_PLUGIN_STORE_URL))
        {
            sources.push(BYTEPROXY_PLUGIN_STORE_URL.to_string());
            changed |= set_core_yaml_path_value(
                document,
                &["plugins", "store-sources"],
                serde_norway::Value::Sequence(
                    sources
                        .into_iter()
                        .map(serde_norway::Value::String)
                        .collect(),
                ),
            )?;
        }

        let absolute_plugins_dir = install_dir.join("plugins");
        let needs_dir_fix = match &current_dir {
            Some(dir) => !Path::new(dir).is_absolute(),
            None => true,
        };
        if needs_dir_fix {
            changed |= set_core_yaml_path_value(
                document,
                &["plugins", "dir"],
                serde_norway::Value::String(path_to_string(&absolute_plugins_dir)),
            )?;
        }

        Ok(changed)
    })
    .map(|updated| updated.unwrap_or_else(|| content.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_store_source_and_keeps_existing_entries() {
        let content = "config-version: 8\nplugins:\n  store-sources:\n    - https://example.com/registry.json\n";
        let updated =
            apply_byteproxy_kernel_defaults(content, Path::new("/tmp/cpa-core")).unwrap();
        assert!(updated.contains("https://example.com/registry.json"));
        assert!(updated.contains(BYTEPROXY_PLUGIN_STORE_URL));
    }

    #[test]
    fn rewrites_relative_plugin_dir_to_absolute() {
        let content = "config-version: 8\nplugins:\n  dir: \"./plugins\"\n";
        let updated =
            apply_byteproxy_kernel_defaults(content, Path::new("/tmp/cpa-core")).unwrap();
        assert!(updated.contains("/tmp/cpa-core/plugins"));
        assert!(!updated.contains("\"./plugins\""));
    }

    #[test]
    fn keeps_custom_absolute_plugin_dir() {
        let content = "config-version: 8\nplugins:\n  dir: \"/custom/plugins\"\n";
        let updated =
            apply_byteproxy_kernel_defaults(content, Path::new("/tmp/cpa-core")).unwrap();
        assert!(updated.contains("/custom/plugins"));
    }

    #[test]
    fn is_idempotent() {
        let content = "config-version: 8\nplugins:\n  dir: \"/tmp/cpa-core/plugins\"\n  store-sources:\n    - https://lf0-fast-deliver-inner.bytedance.net/obj/eden-internal/2121eh7bvuhmnulg/byteproxy/registry.json\n";
        let updated =
            apply_byteproxy_kernel_defaults(content, Path::new("/tmp/cpa-core")).unwrap();
        assert_eq!(updated, content);
    }
}
