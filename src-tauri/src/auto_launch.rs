use crate::error::AppError;
use auto_launch::{AutoLaunch, AutoLaunchBuilder};

// AppImage's current_exe points inside an ephemeral mount. Persist the launcher instead.
#[cfg(any(target_os = "linux", test))]
fn linux_launch_path(
    exe: std::path::PathBuf,
    appimage: Option<std::ffi::OsString>,
) -> std::path::PathBuf {
    appimage
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or(exe)
}

/// 获取 macOS 上的 .app bundle 路径
/// 将 `/path/to/CC Switch.app/Contents/MacOS/CC Switch` 转换为 `/path/to/CC Switch.app`
#[cfg(target_os = "macos")]
fn get_macos_app_bundle_path(exe_path: &std::path::Path) -> Option<std::path::PathBuf> {
    let path_str = exe_path.to_string_lossy();
    // 查找 .app/Contents/MacOS/ 模式
    if let Some(app_pos) = path_str.find(".app/Contents/MacOS/") {
        let app_bundle_end = app_pos + 4; // ".app" 的结束位置
        Some(std::path::PathBuf::from(&path_str[..app_bundle_end]))
    } else {
        None
    }
}

/// 初始化 AutoLaunch 实例
fn get_auto_launch() -> Result<AutoLaunch, AppError> {
    let app_name = "CC Switch";
    let exe_path =
        std::env::current_exe().map_err(|e| AppError::Message(format!("无法获取应用路径: {e}")))?;

    // macOS 需要使用 .app bundle 路径，否则 AppleScript login item 会打开终端
    #[cfg(target_os = "macos")]
    let app_path = get_macos_app_bundle_path(&exe_path).unwrap_or(exe_path);

    #[cfg(target_os = "linux")]
    let app_path = linux_launch_path(exe_path, std::env::var_os("APPIMAGE"));

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let app_path = exe_path;

    // 使用 AutoLaunchBuilder 消除平台差异
    // macOS: 使用 AppleScript 方式（默认），需要 .app bundle 路径
    // Windows/Linux: 使用注册表/XDG autostart
    let auto_launch = AutoLaunchBuilder::new()
        .set_app_name(app_name)
        .set_app_path(&app_path.to_string_lossy())
        .build()
        .map_err(|e| AppError::Message(format!("创建 AutoLaunch 失败: {e}")))?;

    Ok(auto_launch)
}

/// 启用开机自启
pub fn enable_auto_launch() -> Result<(), AppError> {
    let auto_launch = get_auto_launch()?;
    auto_launch
        .enable()
        .map_err(|e| AppError::Message(format!("启用开机自启失败: {e}")))?;
    log::info!("已启用开机自启");
    Ok(())
}

/// 禁用开机自启
pub fn disable_auto_launch() -> Result<(), AppError> {
    let auto_launch = get_auto_launch()?;
    auto_launch
        .disable()
        .map_err(|e| AppError::Message(format!("禁用开机自启失败: {e}")))?;
    log::info!("已禁用开机自启");
    Ok(())
}

/// 检查是否已启用开机自启
pub fn is_auto_launch_enabled() -> Result<bool, AppError> {
    let auto_launch = get_auto_launch()?;
    auto_launch
        .is_enabled()
        .map_err(|e| AppError::Message(format!("检查开机自启状态失败: {e}")))
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn appimage_autostart_uses_the_persistent_launcher_path() {
        let exe = std::env::temp_dir().join(".mount_AppImage/usr/bin/cc-switch");
        let launcher = std::env::temp_dir().join("My Apps/CC Switch.AppImage");
        assert_eq!(
            linux_launch_path(exe.clone(), Some(launcher.clone().into_os_string())),
            launcher
        );
        assert_eq!(linux_launch_path(exe.clone(), None), exe);
        assert_eq!(
            linux_launch_path(exe.clone(), Some("relative.AppImage".into())),
            exe
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_macos_app_bundle_path_valid() {
        let exe_path = std::path::Path::new("/Applications/CC Switch.app/Contents/MacOS/CC Switch");
        let result = get_macos_app_bundle_path(exe_path);
        assert_eq!(
            result,
            Some(std::path::PathBuf::from("/Applications/CC Switch.app"))
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_macos_app_bundle_path_with_spaces() {
        let exe_path =
            std::path::Path::new("/Users/test/My Apps/CC Switch.app/Contents/MacOS/CC Switch");
        let result = get_macos_app_bundle_path(exe_path);
        assert_eq!(
            result,
            Some(std::path::PathBuf::from(
                "/Users/test/My Apps/CC Switch.app"
            ))
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_macos_app_bundle_path_not_in_bundle() {
        let exe_path = std::path::Path::new("/usr/local/bin/cc-switch");
        let result = get_macos_app_bundle_path(exe_path);
        assert_eq!(result, None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_macos_app_bundle_path_dev_build() {
        // 开发环境下的路径通常不在 .app bundle 内
        let exe_path = std::path::Path::new("/Users/dev/project/target/debug/cc-switch");
        let result = get_macos_app_bundle_path(exe_path);
        assert_eq!(result, None);
    }
}
