use super::*;

// ByteProxy 定制：在内核配置里预置字节插件商店，并保证插件目录使用绝对路径。
// 只做加法：已有 store-sources 条目保留，用户自定的绝对插件目录不覆盖。

pub(crate) const BYTEPROXY_PLUGIN_STORE_URL: &str =
    "https://lf0-fast-deliver-inner.bytedance.net/obj/eden-internal/2121eh7bvuhmnulg/byteproxy/registry.json";

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
