//! 启动器与插件库的 GitHub Release 检查、安全更新入口。

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use crate::models::AppSettings;
use crate::state::AppState;

use super::plugin::{
    check_plugin_repo_updates, repo_slug, safe_asset_name, spawn_detached, PluginRelease,
};

const DEFAULT_LAUNCHER_REPO: &str = "https://github.com/mscl-team/mscl";

#[derive(Debug, Clone, serde::Serialize)]
pub struct StartupUpdateCheck {
    pub launcher: Option<PluginRelease>,
    pub plugins: Option<PluginRelease>,
    pub launcher_auto_update: bool,
    pub plugins_auto_update: bool,
}

fn launcher_repo() -> String {
    std::env::var("MSCL_LAUNCHER_REPO")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_LAUNCHER_REPO.to_string())
}

/// 判断 release 资产 URL 是否指向启动器本体（而非安装器/卸载器）。
fn is_launcher_asset(url: &str) -> bool {
    let name = url.rsplit('/').next().unwrap_or("").to_ascii_lowercase();
    name == "mscl-app.exe"
}

fn version_numbers(value: &str) -> Vec<u64> {
    value
        .trim()
        .trim_start_matches(['v', 'V'])
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

fn version_newer(current: &str, candidate: &str) -> bool {
    let current = version_numbers(current);
    let candidate = version_numbers(candidate);
    if candidate.is_empty() {
        return false;
    }
    for index in 0..current.len().max(candidate.len()) {
        let a = *current.get(index).unwrap_or(&0);
        let b = *candidate.get(index).unwrap_or(&0);
        if b > a {
            return true;
        }
        if b < a {
            return false;
        }
    }
    false
}

/// 启动时按设置检查程序与插件库更新；检查失败不影响启动。
#[tauri::command]
pub async fn check_startup_updates(app: AppHandle) -> Result<StartupUpdateCheck, String> {
    let game_root = app.state::<AppState>().game_root.clone();
    let settings = AppSettings::load_or_default(&game_root.join("settings.json"));
    let launcher = if settings.launcher_auto_update {
        check_launcher_updates(app.clone()).await.ok().flatten()
    } else {
        None
    };
    let plugins = if settings.plugin_auto_update || settings.plugin_check_updates {
        if super::plugin::is_valid_repo_setting(&settings.plugin_repo_url) {
            check_plugin_repo_updates(app.clone(), settings.plugin_repo_url.clone())
                .await
                .ok()
                .flatten()
                .filter(|release| version_newer("1.0.0", &release.tag))
        } else {
            None
        }
    } else {
        None
    };
    Ok(StartupUpdateCheck {
        launcher,
        plugins,
        launcher_auto_update: settings.launcher_auto_update,
        plugins_auto_update: settings.plugin_auto_update,
    })
}

/// 检查启动器本体是否有新 release。
#[tauri::command]
pub async fn check_launcher_updates(app: AppHandle) -> Result<Option<PluginRelease>, String> {
    let release = check_plugin_repo_updates(app, launcher_repo()).await?;
    Ok(release.filter(|r| version_newer(env!("CARGO_PKG_VERSION"), &r.tag)))
}

/// 下载 release 中的启动器可执行文件，并启动独立助手完成替换。
#[tauri::command]
pub async fn prepare_launcher_update(app: AppHandle, version: String) -> Result<String, String> {
    let release = check_launcher_updates(app.clone())
        .await?
        .ok_or_else(|| "当前已是最新版本".to_string())?;
    let repo = launcher_repo();
    let slug = repo_slug(&repo)?;
    // 精确挑选启动器本体资产：只认文件名末尾是 mscl-app.exe 的项。
    // 不能泛匹配 "含 mscl 的 .exe"，否则会误选同 release 里的
    // MSCL_Setup_*.exe / MSCL_Uninstall_*.exe，把安装程序当作新版启动器替换。
    let asset = release
        .assets
        .iter()
        .find(|url| is_launcher_asset(url))
        .cloned()
        .unwrap_or_else(|| {
            format!(
                "https://github.com/{}/releases/download/{}/mscl-app.exe",
                slug, release.tag
            )
        });

    let game_root = app.state::<AppState>().game_root.clone();
    let updates = game_root.join("updates");
    std::fs::create_dir_all(&updates).map_err(|e| format!("创建下载目录失败: {}", e))?;
    let target = updates.join(format!("mscl-app-{}.exe", safe_asset_name(&release.tag)));

    let http = app.state::<AppState>().http_client.clone();
    let response = http
        .get(&asset)
        .header("User-Agent", "MSCL-Updater")
        .send()
        .await
        .map_err(|e| format!("下载新版启动器失败: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("下载新版启动器失败: HTTP {}", response.status()));
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() < 1024 * 1024 || &bytes[..2] != b"MZ" {
        return Err("下载内容不是有效的 Windows 可执行文件".to_string());
    }
    std::fs::write(&target, &bytes).map_err(|e| format!("保存新版启动器失败: {}", e))?;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--apply-launcher-update").arg(&target);
    spawn_detached(&mut cmd)?;
    Ok(format!("新版 {} 已下载，正在安全更新", version))
}

/// 独立助手入口：主窗口关闭后替换启动器，失败时恢复原文件。
pub fn apply_launcher_update(download: &Path) -> Result<(), String> {
    std::thread::sleep(std::time::Duration::from_millis(2000));
    let target = std::env::current_exe().map_err(|e| e.to_string())?;
    if target.file_name().and_then(|n| n.to_str()) != Some("mscl-app.exe") {
        return Err("当前程序不是可更新的启动器主程序".to_string());
    }
    let backup: PathBuf = target.with_extension("exe.bak");
    let mut last_error = String::new();
    let mut renamed = false;
    for _ in 0..60 {
        match std::fs::rename(&target, &backup) {
            Ok(()) => {
                renamed = true;
                break;
            }
            Err(e) => {
                last_error = e.to_string();
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    }
    if !renamed {
        return Err(format!("无法接管启动器文件: {}", last_error));
    }
    if let Err(e) = std::fs::copy(download, &target) {
        let _ = std::fs::rename(&backup, &target);
        return Err(format!("写入新版失败，已恢复旧版: {}", e));
    }
    match std::fs::metadata(&target) {
        Ok(meta) if meta.len() > 1024 * 1024 => {
            let _ = std::fs::remove_file(&backup);
            let _ = std::fs::remove_file(download);
            Ok(())
        }
        Ok(_) => {
            let _ = std::fs::remove_file(&target);
            let _ = std::fs::rename(&backup, &target);
            Err("新版文件大小异常，已恢复旧版".to_string())
        }
        Err(e) => {
            let _ = std::fs::rename(&backup, &target);
            Err(format!("校验新版失败，已恢复旧版: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(version_newer("0.1.0", "0.2.0"));
        assert!(version_newer("0.1.0", "v0.1.1"));
        assert!(!version_newer("0.1.0", "0.1.0"));
        assert!(!version_newer("0.2.0", "0.1.9"));
        assert!(!version_newer("0.1.0", "release"));
    }

    #[test]
    fn picks_only_launcher_asset() {
        let base = "https://github.com/mscl-team/mscl/releases/download/v0.2.0/";
        assert!(is_launcher_asset(&format!("{}mscl-app.exe", base)));
        assert!(is_launcher_asset(&format!("{}MSCL-App.exe", base)));
        // 安装器/卸载器绝不能被当作启动器下载
        assert!(!is_launcher_asset(&format!("{}MSCL_Setup_0.2.0.exe", base)));
        assert!(!is_launcher_asset(&format!("{}MSCL_Uninstall_0.2.0.exe", base)));
        assert!(!is_launcher_asset(&format!("{}mscl-app.exe.zip", base)));
    }
}
