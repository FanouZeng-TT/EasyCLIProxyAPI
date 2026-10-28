use super::*;

pub(crate) fn sync_client_context_limits(content: &str, catalog: &str) -> Result<String, String> {
    let mut document = content
        .parse::<toml_edit::Document>()
        .map_err(|error| format!("Failed to parse Codex context configuration: {error}"))?;
    let catalog: Value = serde_json::from_str(catalog).map_err(|error| error.to_string())?;
    let selected = document.get("model").and_then(toml_edit::Item::as_str);
    let context = catalog
        .get("models")
        .and_then(Value::as_array)
        .and_then(|models| {
            models
                .iter()
                .find(|model| model.get("slug").and_then(Value::as_str) == selected)
        })
        .and_then(|model| positive_u64_value(model.get("context_window")));
    if let Some(context) = context {
        if document.get("model_context_window").is_some() {
            document["model_context_window"] =
                toml_edit::value(context.min(i64::MAX as u64) as i64);
        }
        if let Some(compact) = document
            .get("model_auto_compact_token_limit")
            .and_then(toml_edit::Item::as_integer)
            .filter(|compact| *compact > 0)
        {
            document["model_auto_compact_token_limit"] =
                toml_edit::value(compact.min(context.min(i64::MAX as u64) as i64));
        }
    }
    Ok(document.to_string())
}

pub(crate) fn apply_client_context_limits(
    runtime_models: &mut [CodexRuntimeModel],
    content: &str,
) -> Result<(), String> {
    let document: toml::Value = toml::from_str(content)
        .map_err(|error| format!("Failed to parse Codex context configuration: {error}"))?;
    let Some(selected) = document.get("model").and_then(toml::Value::as_str) else {
        return Ok(());
    };
    let Some(limit) = document
        .get("model_context_window")
        .and_then(toml::Value::as_integer)
        .and_then(|limit| u64::try_from(limit).ok())
        .filter(|limit| *limit > 0)
    else {
        return Ok(());
    };
    for runtime in runtime_models
        .iter_mut()
        .filter(|runtime| runtime.slug == selected)
    {
        if runtime.context_source != "configuration" {
            runtime.max_context_window = runtime.max_context_window.or(runtime.context_window);
        }
        runtime.context_window = Some(limit);
        runtime.context_source = "configuration";
    }
    Ok(())
}

pub(super) fn apply_context_policy(
    model: &mut Map<String, Value>,
    runtime: Option<&CodexRuntimeModel>,
    initialize: bool,
) {
    let maximum = runtime
        .and_then(|runtime| {
            if runtime.context_source == "configuration" {
                runtime.max_context_window
            } else {
                runtime.max_context_window.or(runtime.context_window)
            }
        })
        .map(|maximum| maximum.min(350_000));
    let configured = runtime
        .filter(|runtime| runtime.context_source == "configuration")
        .and_then(|runtime| runtime.context_window);
    for field in ["context_window", "max_context_window"] {
        let requested = if initialize {
            configured.unwrap_or(250_000)
        } else {
            positive_u64_value(model.get(field)).unwrap_or(250_000)
        };
        model.insert(
            field.into(),
            Value::from(maximum.map_or(requested, |maximum| requested.min(maximum))),
        );
    }
    let context = positive_u64_value(model.get("context_window")).unwrap_or(250_000);
    if let Some(compact) = positive_u64_value(model.get("auto_compact_token_limit")) {
        model.insert(
            "auto_compact_token_limit".into(),
            Value::from(compact.min(context)),
        );
    }
}

pub(crate) fn apply_configured_context_limits(
    runtime_models: &mut [CodexRuntimeModel],
    content: &str,
) -> Result<(), String> {
    let document: serde_norway::Value = serde_norway::from_str(content)
        .map_err(|error| format!("Failed to parse kernel model context configuration: {error}"))?;
    let root = document
        .as_mapping()
        .ok_or("Kernel configuration root must be a YAML mapping")?;
    let mut limits: HashMap<String, u64> = HashMap::new();
    for section in crate::MODEL_ALIAS_CONFIG_SECTIONS {
        let Some(providers) =
            crate::yaml_mapping_value(root, section).and_then(serde_norway::Value::as_sequence)
        else {
            continue;
        };
        for provider in providers {
            let Some(models) = provider
                .as_mapping()
                .and_then(|provider| crate::yaml_mapping_value(provider, "models"))
                .and_then(serde_norway::Value::as_sequence)
            else {
                continue;
            };
            for model in models {
                let Some((_, public_name, _)) = crate::configured_model_identity(model) else {
                    continue;
                };
                let Some(limit) = model
                    .as_mapping()
                    .and_then(|model| crate::yaml_mapping_value(model, "max-context-length"))
                    .and_then(serde_norway::Value::as_u64)
                    .filter(|value| *value > 0)
                else {
                    continue;
                };
                limits
                    .entry(normalize_id(&public_name))
                    .and_modify(|existing| *existing = (*existing).min(limit))
                    .or_insert(limit);
            }
        }
    }
    for runtime in runtime_models {
        if let Some(&limit) = limits.get(&normalize_id(&runtime.slug)) {
            runtime.max_context_window = runtime.max_context_window.or(runtime.context_window);
            runtime.context_window = Some(limit);
            runtime.context_source = "configuration";
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_policy_limits_known_capacity_and_preserves_unknown_customizations() {
        for (upstream, requested, expected) in [
            (Some(1_000_000), Some(500_000), 350_000),
            (Some(128_000), Some(500_000), 128_000),
            (Some(350_000), Some(100_000), 100_000),
            (Some(128_000), None, 128_000),
            (None, Some(1_000_000), 1_000_000),
            (None, None, 250_000),
        ] {
            let runtime = parse_runtime_models(&serde_json::json!({"models":[{
                "slug":"policy-model", "context_length":upstream
            }]}))
            .unwrap();
            let mut customizations = customizations::ModelCustomizations::default();
            if let Some(requested) = requested {
                customizations.insert(
                    "policy-model".into(),
                    serde_json::json!({
                        "context_window": requested,
                        "max_context_window": requested,
                        "auto_compact_token_limit": requested
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                );
            }
            let state = CatalogState {
                sources: parse_sources(MODEL_CATALOG_JSON).unwrap(),
                json: MODEL_CATALOG_JSON.into(),
                customizations,
            };
            let prepared = prepare_catalog_with_customizations(
                &runtime,
                &state.sources,
                &state.customizations,
            )
            .unwrap();
            let catalog: Value = serde_json::from_str(&prepared.json).unwrap();
            let snapshot =
                serde_json::to_value(customizations::snapshot_for_state(&runtime, &state).unwrap())
                    .unwrap();
            for model in [
                &catalog["models"][0],
                &snapshot["models"][0]["configuration"],
            ] {
                assert_eq!(model["context_window"], expected);
                assert_eq!(model["max_context_window"], expected);
                if requested.is_some() {
                    assert_eq!(model["auto_compact_token_limit"], expected);
                }
            }
            if let Some(requested) = requested {
                assert_eq!(
                    state.customizations["policy-model"]["context_window"],
                    requested
                );
            }
        }
    }

    #[test]
    fn existing_client_limits_are_preserved_and_clamped_to_effective_model_capacity() {
        let content = "model = \"policy-model\"\nmodel_context_window = 1000000\nmodel_auto_compact_token_limit = 900000\n";
        for (upstream, expected) in [
            (None, 1_000_000),
            (Some(128_000), 128_000),
            (Some(1_000_000), 350_000),
        ] {
            let mut runtime = parse_runtime_models(&serde_json::json!({"models":[{
                "slug":"policy-model", "context_length":upstream
            }]}))
            .unwrap();
            apply_client_context_limits(&mut runtime, content).unwrap();
            let prepared = prepare_catalog_with_customizations(
                &runtime,
                &parse_sources(MODEL_CATALOG_JSON).unwrap(),
                &Default::default(),
            )
            .unwrap();
            let synchronized: toml::Value =
                toml::from_str(&sync_client_context_limits(content, &prepared.json).unwrap())
                    .unwrap();
            assert_eq!(
                synchronized["model_context_window"].as_integer(),
                Some(expected)
            );
            assert_eq!(
                synchronized["model_auto_compact_token_limit"].as_integer(),
                Some(expected.min(900_000))
            );
        }
    }

    #[test]
    fn configured_limit_is_not_mistaken_for_upstream_capacity() {
        let mut runtime = parse_public_models(&serde_json::json!({"data":[{
            "id":"unknown", "max_context_length":1_000_000
        }]}))
        .unwrap();
        apply_configured_context_limits(&mut runtime, "codex-api-key:\n  - models:\n      - name: unknown\n        max-context-length: 1000000\n").unwrap();
        apply_client_context_limits(
            &mut runtime,
            "model = \"unknown\"\nmodel_context_window = 800000\n",
        )
        .unwrap();
        let prepared = prepare_catalog_with_customizations(
            &runtime,
            &parse_sources(MODEL_CATALOG_JSON).unwrap(),
            &Default::default(),
        )
        .unwrap();
        let catalog: Value = serde_json::from_str(&prepared.json).unwrap();
        assert_eq!(catalog["models"][0]["context_window"], 800_000);
    }

    #[test]
    fn codex_client_context_defaults_are_preserved_in_editor_and_generated_catalog() {
        let runtime = parse_runtime_models(&serde_json::json!({"models":[
            {"slug":"gpt-5.6-sol","context_window":272000,"max_context_window":872000},
            {"slug":"fast-alias","context_window":128000,"max_context_window":256000},
            {"slug":"missing-context"}
        ]}))
        .unwrap();
        let sources = parse_sources(MODEL_CATALOG_JSON).unwrap();
        let state = CatalogState {
            sources,
            json: MODEL_CATALOG_JSON.to_string(),
            customizations: Default::default(),
        };
        let snapshot = customizations::snapshot_for_state(&runtime, &state).unwrap();
        let snapshot = serde_json::to_value(snapshot).unwrap();
        let prepared =
            prepare_catalog_with_customizations(&runtime, &state.sources, &state.customizations)
                .unwrap();
        let catalog: Value = serde_json::from_str(&prepared.json).unwrap();
        for (slug, context, maximum, source) in [
            ("gpt-5.6-sol", 250_000, 250_000, "client"),
            ("fast-alias", 250_000, 250_000, "client"),
            ("missing-context", 250_000, 250_000, "template"),
        ] {
            let model = snapshot["models"]
                .as_array()
                .unwrap()
                .iter()
                .find(|model| model["slug"] == slug)
                .unwrap();
            for field in ["configuration", "defaults"] {
                assert_eq!(model[field]["context_window"], context);
                assert_eq!(model[field]["max_context_window"], maximum);
            }
            assert_eq!(model["contextSource"], source);
            let generated = catalog["models"]
                .as_array()
                .unwrap()
                .iter()
                .find(|model| model["slug"] == slug)
                .unwrap();
            assert_eq!(generated["context_window"], context);
            assert_eq!(generated["max_context_window"], maximum);
        }
    }

    #[test]
    fn core_configured_context_limit_takes_precedence_over_client_defaults() {
        let mut runtime = parse_runtime_models(&serde_json::json!({"models":[
            {"slug":"custom-alias","context_window":921000,"max_context_window":921000},
            {"slug":"unmodified","context_window":128000,"max_context_window":128000}
        ]}))
        .unwrap();
        apply_configured_context_limits(&mut runtime, "codex-api-key:\n  - models:\n      - name: upstream-model\n        alias: custom-alias\n        max-context-length: 64000\n").unwrap();
        assert_eq!(runtime[0].context_window, Some(64_000));
        assert_eq!(runtime[0].max_context_window, Some(921_000));
        assert_eq!(runtime[0].context_source, "configuration");
        assert_eq!(runtime[1].context_window, Some(128_000));
    }
}
