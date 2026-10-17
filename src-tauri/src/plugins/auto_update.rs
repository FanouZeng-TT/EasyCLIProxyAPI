use super::super::*;

const BYTEPROXY_PLUGIN_ID: &str = "byteproxy";
const BYTEPROXY_UPDATE_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const BYTEPROXY_INITIAL_DELAY: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq, Eq)]
struct ByteProxyUpdate {
    source_id: String,
    installed_version: String,
    version: String,
    installed_path: PathBuf,
}

fn pending_byteproxy_update(payload: &serde_json::Value) -> Option<ByteProxyUpdate> {
    let entries = payload.get("plugins")?.as_array()?;
    let entry = entries.iter().find(|entry| {
        entry.get("id").and_then(serde_json::Value::as_str) == Some(BYTEPROXY_PLUGIN_ID)
            && entry.get("update_available").and_then(serde_json::Value::as_bool) == Some(true)
    })?;
    Some(ByteProxyUpdate {
        source_id: entry.get("source_id")?.as_str()?.trim().to_string(),
        installed_version: entry
            .get("installed_version")?
            .as_str()?
            .trim()
            .to_string(),
        version: entry.get("version")?.as_str()?.trim().to_string(),
        installed_path: PathBuf::from(entry.get("path")?.as_str()?.trim()),
    })
    .filter(|update| {
        !update.source_id.is_empty()
            && !update.installed_version.is_empty()
            && !update.version.is_empty()
            && !update.installed_path.as_os_str().is_empty()
    })
}

async fn fetch_byteproxy_update(config: &GuiConfigFile) -> Result<Option<ByteProxyUpdate>, String> {
    let response = management_http_client()?
        .get(management_endpoint(config, "plugins/store")?)
        .header("Authorization", management_authorization(config)?)
        .timeout(Duration::from_secs(120))
        .send()
        .await
        .map_err(|error| format_management_request_error("Failed to check ByteProxy updates", &error))?;
    Ok(pending_byteproxy_update(&read_management_value(response).await?))
}

async fn install_byteproxy_update(
    config: &GuiConfigFile,
    update: &ByteProxyUpdate,
) -> Result<PathBuf, String> {
    let response = management_http_client()?
        .post(management_endpoint(config, &format!("plugins/store/{BYTEPROXY_PLUGIN_ID}/install"))?)
        .header("Authorization", management_authorization(config)?)
        .query(&[("source", update.source_id.as_str()), ("version", update.version.as_str())])
        .json(&serde_json::json!({"version": update.version}))
        .timeout(Duration::from_secs(120))
        .send()
        .await
        .map_err(|error| format_management_request_error("Failed to install ByteProxy update", &error))?;
    let value = read_management_value(response).await?;
    value
        .get("path")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "ByteProxy install response did not include the installed file path".to_string())
}

fn restore_plugin_after_failed_restart(
    installed_path: &Path,
    previous_path: &Path,
    previous_bytes: &[u8],
) -> Result<(), String> {
    if installed_path != previous_path {
        let _ = fs::remove_file(installed_path);
    }
    write_bytes_atomically(previous_path, previous_bytes)
        .map_err(|error| format!("failed to restore previous ByteProxy plugin: {error}"))
}

async fn update_byteproxy_once(app: &tauri::AppHandle) -> Result<bool, String> {
    let config = app.state::<GuiConfigState>().snapshot()?;
    let status = current_core_status(Some(app.state::<CoreProcessState>().inner()), Some(config.port))?;
    if !status.ready {
        return Ok(false);
    }
    let Some(update) = fetch_byteproxy_update(&config).await? else {
        return Ok(false);
    };
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = CORE_OPERATION_LOCK
            .try_lock()
            .map_err(|_| "The kernel is performing another operation; deferring ByteProxy update".to_string())?;
        let process_state = app.state::<CoreProcessState>();
        let was_running = current_core_status(Some(process_state.inner()), None)?.running;
        let previous_bytes = fs::read(&update.installed_path).map_err(|error| {
            format!("Failed to back up the installed ByteProxy plugin: {error}")
        })?;
        // macOS/Linux can install the new versioned file while the old dylib is loaded.
        // Keep CPA alive for its management API, then restart once to activate the new file.
        let installed_path = tauri::async_runtime::block_on(install_byteproxy_update(&config, &update))?;
        if was_running {
            if let Err(stop_error) = stop_core_process_inner(process_state.inner()) {
                let rollback = restore_plugin_after_failed_restart(
                    &installed_path,
                    &update.installed_path,
                    &previous_bytes,
                );
                return Err(format!(
                    "ByteProxy {} installed but CPA could not stop for activation: {stop_error}; rollback={}",
                    update.version,
                    rollback.map(|_| "ok".to_string()).unwrap_or_else(|error| error),
                ));
            }
        }
        if was_running && !process_state.is_shutting_down() {
            if let Err(restart_error) = start_core_process_inner(process_state.inner(), &config) {
                let rollback = restore_plugin_after_failed_restart(
                    &installed_path,
                    &update.installed_path,
                    &previous_bytes,
                );
                let restore_restart = start_core_process_inner(process_state.inner(), &config);
                return Err(format!(
                    "ByteProxy {} installed but CPA failed to restart: {restart_error}; rollback={}; restore_restart={}",
                    update.version,
                    rollback.map(|_| "ok".to_string()).unwrap_or_else(|error| error),
                    restore_restart.map(|_| "ok".to_string()).unwrap_or_else(|error| error),
                ));
            }
        }
        eprintln!("Automatically updated ByteProxy from {} to {}", update.installed_version, update.version);
        Ok(true)
    })
    .await
    .map_err(|error| format!("ByteProxy update task failed: {error}"))?
}

pub(crate) fn start_byteproxy_auto_update(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(BYTEPROXY_INITIAL_DELAY).await;
        loop {
            if let Err(error) = update_byteproxy_once(&app).await {
                eprintln!("ByteProxy automatic update check failed; keeping the installed plugin: {error}");
            }
            tokio::time::sleep(BYTEPROXY_UPDATE_INTERVAL).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_restart_rollback_restores_previous_plugin_and_removes_new_version() {
        let root = std::env::temp_dir().join(format!(
            "byteproxy-rollback-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),
        ));
        fs::create_dir_all(&root).unwrap();
        let previous = root.join("byteproxy-v0.5.49.dylib");
        let installed = root.join("byteproxy-v0.5.50.dylib");
        fs::write(&previous, b"damaged").unwrap();
        fs::write(&installed, b"new").unwrap();

        restore_plugin_after_failed_restart(&installed, &previous, b"old").unwrap();

        assert_eq!(fs::read(&previous).unwrap(), b"old");
        assert!(!installed.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finds_only_available_byteproxy_updates() {
        let payload = serde_json::json!({"plugins": [
            {"id": "other", "update_available": true, "source_id": "official", "installed_version": "8.0.0", "version": "9.0.0", "path": "/tmp/other.dylib"},
            {"id": "byteproxy", "update_available": true, "source_id": "custom-1", "installed_version": "0.5.49", "version": "0.5.50", "path": "/tmp/byteproxy-v0.5.49.dylib"}
        ]});
        assert_eq!(pending_byteproxy_update(&payload), Some(ByteProxyUpdate {
            source_id: "custom-1".into(),
            installed_version: "0.5.49".into(),
            version: "0.5.50".into(),
            installed_path: PathBuf::from("/tmp/byteproxy-v0.5.49.dylib"),
        }));
        assert_eq!(pending_byteproxy_update(&serde_json::json!({"plugins": [
            {"id": "byteproxy", "update_available": false, "source_id": "custom-1", "installed_version": "0.5.49", "version": "0.5.50", "path": "/tmp/byteproxy-v0.5.49.dylib"}
        ]})), None);
    }
}
