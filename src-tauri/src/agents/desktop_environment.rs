use super::*;

fn claude_desktop_context_environment(window: u64) -> [(&'static str, String); 2] {
    let value = window.to_string();
    [
        (CLAUDE_CODE_MAX_CONTEXT_TOKENS_ENV, value.clone()),
        (CLAUDE_CODE_AUTO_COMPACT_WINDOW_ENV, value),
    ]
}

#[cfg(target_os = "macos")]
pub(crate) fn apply_claude_desktop_context_environment(window: u64) -> Result<(), String> {
    for (name, value) in claude_desktop_context_environment(window) {
        let output = Command::new("/bin/launchctl")
            .args(["setenv", name, &value])
            .output()
            .map_err(|error| format!("Failed to update the Claude Desktop context environment: {error}"))?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if detail.is_empty() {
                format!("Failed to update the Claude Desktop context environment variable {name}")
            } else {
                format!("Failed to update the Claude Desktop context environment variable {name}: {detail}")
            });
        }
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn apply_claude_desktop_context_environment(_window: u64) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_environment_uses_the_configured_window() {
        assert_eq!(
            claude_desktop_context_environment(350_000),
            [
                (CLAUDE_CODE_MAX_CONTEXT_TOKENS_ENV, "350000".to_string()),
                (CLAUDE_CODE_AUTO_COMPACT_WINDOW_ENV, "350000".to_string()),
            ]
        );
    }
}
