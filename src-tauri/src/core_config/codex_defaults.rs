use super::*;

// The GUI owns the installation default; the kernel still decides model
// capabilities from the registered executors. Read the original file before
// migration can insert a disabled value from the kernel's example template.
pub(crate) fn apply_codex_apply_patch_default(
    content: &str,
    current: Option<&str>,
) -> Result<String, String> {
    let explicit = current
        .map(|current| -> Result<Option<bool>, String> {
            let document: serde_norway::Value = serde_norway::from_str(current)
                .map_err(|error| format!("Failed to parse kernel configuration: {error}"))?;
            let root = document
                .as_mapping()
                .ok_or("Kernel configuration root must be a YAML mapping")?;
            nested_yaml_value(root, &["client", "codex", "enable-apply-patch"])
                .or_else(|| {
                    nested_yaml_value(root, &["oauth", "providers", "codex", "enable-apply-patch"])
                })
                .or_else(|| nested_yaml_value(root, &["codex", "enable-apply-patch"]))
                .map(|value| {
                    value.as_bool().ok_or_else(|| {
                        "client.codex.enable-apply-patch must be a boolean".to_string()
                    })
                })
                .transpose()
        })
        .transpose()?
        .flatten();
    Ok(patch_core_yaml_document(content, |document| {
        set_core_yaml_path_value(
            document,
            &["client", "codex", "enable-apply-patch"],
            serde_norway::Value::Bool(explicit.unwrap_or(true)),
        )
    })?
    .unwrap_or_else(|| content.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_apply_patch_defaults_enable_new_installs_and_missing_fields() {
        let template = "config-version: 8\nclient:\n  codex:\n    enable-apply-patch: false\n    optimize-multi-agent-v2: false\n";
        for current in [
            None,
            Some("config-version: 8\n"),
            Some("port: 8317\n"),
            Some("config-version: 8\nclient: {codex: {optimize-multi-agent-v2: true}}\n"),
        ] {
            let merged =
                merge_core_config_yaml(template, current, &GuiConfigFile::default()).unwrap();
            let value: serde_norway::Value = serde_norway::from_str(&merged).unwrap();
            assert_eq!(value["client"]["codex"]["enable-apply-patch"], true);
            if current.is_some_and(|text| text.contains("optimize-multi-agent-v2: true")) {
                assert_eq!(value["client"]["codex"]["optimize-multi-agent-v2"], true);
            }
            assert_eq!(
                merge_core_config_yaml(template, Some(&merged), &GuiConfigFile::default()).unwrap(),
                merged
            );
        }
    }

    #[test]
    fn codex_apply_patch_defaults_preserve_explicit_choices_and_comments() {
        for value in [false, true] {
            for field in [
                format!("client: {{codex: {{enable-apply-patch: {value}}}}}"),
                format!("oauth: {{providers: {{codex: {{enable-apply-patch: {value}}}}}}}"),
                format!("codex: {{enable-apply-patch: {value}}}"),
            ] {
                let current = format!("# user choice\n{field}\n");
                let rendered = apply_codex_apply_patch_default(&current, Some(&current)).unwrap();
                let document: serde_norway::Value = serde_norway::from_str(&rendered).unwrap();
                assert_eq!(document["client"]["codex"]["enable-apply-patch"], value);
                assert!(rendered.contains("# user choice"));
                assert_eq!(
                    apply_codex_apply_patch_default(&rendered, Some(&rendered)).unwrap(),
                    rendered
                );
            }
        }
    }

    #[test]
    fn codex_apply_patch_defaults_are_saved_before_start_and_keep_opt_out() {
        let install = std::env::temp_dir().join(format!(
            "cpa-codex-apply-patch-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&install).unwrap();
        fs::write(
            install.join(CORE_EXAMPLE_CONFIG_FILE),
            "config-version: 8\nclient: {codex: {enable-apply-patch: false}}\n",
        )
        .unwrap();
        let config = GuiConfigFile::default();
        let path = merge_core_config_for_start(&install, &config).unwrap();
        let first = fs::read_to_string(&path).unwrap();
        let document: serde_norway::Value = serde_norway::from_str(&first).unwrap();
        assert_eq!(document["client"]["codex"]["enable-apply-patch"], true);
        let disabled = patch_core_yaml_document(&first, |document| {
            set_core_yaml_path_value(
                document,
                &["client", "codex", "enable-apply-patch"],
                serde_norway::Value::Bool(false),
            )
        })
        .unwrap()
        .unwrap();
        fs::write(&path, &disabled).unwrap();
        merge_core_config_for_start(&install, &config).unwrap();
        let saved: serde_norway::Value =
            serde_norway::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(saved["client"]["codex"]["enable-apply-patch"], false);
        fs::remove_dir_all(install).unwrap();
    }

    #[test]
    fn codex_apply_patch_defaults_reject_invalid_existing_choices() {
        let invalid = "client: {codex: {enable-apply-patch: invalid}}\n";
        assert!(apply_codex_apply_patch_default(invalid, Some(invalid)).is_err());
    }
}
