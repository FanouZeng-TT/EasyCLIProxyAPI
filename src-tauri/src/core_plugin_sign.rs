// ByteProxy 定制：macOS 上官方 CPA 内核启用了 Library Validation，会拒绝加载
// 未同队签名的第三方插件 dylib。这里复刻 local-core-signfix.sh 的做法——启动前
// 检查内核是否带 disable-library-validation entitlement，没有则本地重签名。
// 每次安装只会执行一次（entitlement 已存在即跳过），失败不阻断内核启动。

const ENTITLEMENT_PLIST: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
    "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
    "<plist version=\"1.0\">\n",
    "<dict>\n",
    "  <key>com.apple.security.cs.disable-library-validation</key>\n",
    "  <true/>\n",
    "</dict>\n",
    "</plist>\n",
);

const LIBRARY_VALIDATION_ENTITLEMENT: &str = "com.apple.security.cs.disable-library-validation";

#[cfg(target_os = "macos")]
fn core_entitlements(binary: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new("codesign")
        .args(["-d", "--entitlements", ":-"])
        .arg(binary)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(target_os = "macos")]
fn resign_core_with_plugin_entitlement(binary: &std::path::Path) -> Result<(), String> {
    let entitlements = std::env::temp_dir().join("byteproxy-core-entitlements.plist");
    std::fs::write(&entitlements, ENTITLEMENT_PLIST)
        .map_err(|error| format!("Failed to write entitlements file: {error}"))?;
    let result = std::process::Command::new("codesign")
        .args(["--force", "--sign", "-", "--options", "runtime"])
        .arg("--entitlements")
        .arg(&entitlements)
        .arg(binary)
        .output()
        .map_err(|error| format!("Failed to run codesign: {error}"))?;
    let _ = std::fs::remove_file(&entitlements);
    if !result.status.success() {
        return Err(format!(
            "codesign failed: {}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    Ok(())
}

/// 启动内核前确保内核允许加载第三方插件；已具备 entitlement 时是纯检查，开销极小。
#[cfg(target_os = "macos")]
pub(crate) fn ensure_core_plugin_loading_supported(binary: &std::path::Path) -> bool {
    match core_entitlements(binary) {
        Some(entitlements) if entitlements.contains(LIBRARY_VALIDATION_ENTITLEMENT) => true,
        _ => match resign_core_with_plugin_entitlement(binary) {
            Ok(()) => {
                eprintln!(
                    "ByteProxy: re-signed CPA kernel with disable-library-validation"
                );
                true
            }
            Err(error) => {
                eprintln!(
                    "ByteProxy: failed to re-sign CPA kernel for plugin loading: {error}"
                );
                false
            }
        },
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn ensure_core_plugin_loading_supported(_binary: &std::path::Path) -> bool {
    true
}
