//! 系统类命令：后端状态、版本列表、Java 检测、窗口控制、设置持久化

use std::path::{Path, PathBuf};
use std::process::Command;

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::models::{AppSettings, JavaInfo, VersionInfo, VersionListResponse, WindowAction};
use crate::state::AppState;

use super::instance::emit_log;
use super::isolation;

/// 设置文件路径（game_root/settings.json）
fn settings_path(game_root: &Path) -> std::path::PathBuf {
    game_root.join("settings.json")
}

/// 后端状态（健康检查）
#[tauri::command]
pub async fn backend_status(app: AppHandle) -> Result<serde_json::Value, String> {
    let state = app.state::<AppState>();
    let running = state.list_running();
    Ok(serde_json::json!({
        "core": mscl_core::core_ready(),
        "os": format!("{:?}", mscl_platform::current_os()),
        "game_root": state.game_root.to_string_lossy(),
        "running_pid": running.first().map(|item| item.pid),
        "running_instances": running,
    }))
}

/// 获取版本清单（从 Mojang 官方拉取）
#[tauri::command]
pub async fn get_version_list(app: AppHandle) -> Result<VersionListResponse, String> {
    let http_client = app.state::<AppState>().http_client.clone();
    let manifest = mscl_core::fetch_manifest(&http_client)
        .await
        .map_err(|e| format!("获取版本清单失败: {}", e))?;

    let latest_release = manifest.latest.release.clone();
    let versions = manifest
        .versions
        .iter()
        .map(|v| VersionInfo {
            id: v.id.clone(),
            // 统一小写，前端按 "release" / "snapshot" 判断
            version_type: format!("{:?}", v.version_type).to_lowercase(),
            release_time: v.release_time.clone(),
            is_latest_release: v.id == latest_release,
        })
        .collect();

    Ok(VersionListResponse {
        latest_release,
        latest_snapshot: manifest.latest.snapshot.clone(),
        versions,
    })
}

/// 检测系统 Java
#[tauri::command]
pub async fn detect_java() -> Result<JavaInfo, String> {
    let java = mscl_platform::detect_java().map_err(|e| format!("{}", e))?;
    Ok(JavaInfo {
        path: java.path.to_string_lossy().into_owned(),
        major_version: java.major_version,
        version_output: java.version_output,
    })
}

/// 读取应用设置；文件不存在时返回默认值（首次启动）
#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Result<AppSettings, String> {
    let game_root = app.state::<AppState>().game_root.clone();
    let path = settings_path(&game_root);

    match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<AppSettings>(&text) {
            Ok(settings) => Ok(settings),
            Err(e) => {
                let _ = std::fs::write(
                    game_root.join("settings.json.invalid"),
                    format!("{}\n{}", text, e),
                );
                Ok(AppSettings::load_or_default(&path))
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AppSettings::default()),
        Err(e) => Err(format!("读取设置失败 ({}): {}", path.display(), e)),
    }
}

/// 保存应用设置：先写临时文件再替换，避免写入中断导致设置损坏
#[tauri::command]
pub async fn save_settings(app: AppHandle, mut settings: AppSettings) -> Result<(), String> {
    if settings.username.trim().is_empty() {
        return Err("游戏昵称不能为空".to_string());
    }
    if settings.max_memory_mb > 65536 {
        return Err("内存分配不能超过 65536 MB".to_string());
    }
    if !(1..=32).contains(&settings.download_concurrency) {
        return Err("下载并发数需在 1-32 之间".to_string());
    }
    if settings.download_source.trim().is_empty() {
        return Err("下载源不能为空".to_string());
    }
    settings.isolation_resources = isolation::normalize_resources(&settings.isolation_resources);

    let game_root = app.state::<AppState>().game_root.clone();
    std::fs::create_dir_all(&game_root)
        .map_err(|e| format!("创建数据目录失败 ({}): {}", game_root.display(), e))?;

    let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    let tmp_path = game_root.join("settings.json.tmp");
    std::fs::write(&tmp_path, text)
        .map_err(|e| format!("写入临时设置文件失败: {}", e))?;
    std::fs::rename(&tmp_path, settings_path(&game_root))
        .map_err(|e| format!("保存设置失败: {}", e))?;

    for warning in isolation::sync_all_instances(&game_root, &settings) {
        emit_log(&app, "warn", format!("版本隔离：{}", warning));
    }

    Ok(())
}

/// 可导出的设置字段（与前端勾选列表一致）
const EXPORTABLE_SETTING_KEYS: &[&str] = &[
    "username",
    "max_memory_mb",
    "java_path",
    "download_source",
    "download_concurrency",
    "version_isolation",
    "isolation_resources",
    "resource_sources",
    "check_dependencies",
    "verify_resources_on_launch",
    "verify_game_on_launch",
    "launcher_language",
    "extra_jvm_args",
    "extra_game_args",
    "hide_install_logs",
    "hide_update_logs",
    "hide_launch_logs",
    "plugin_repo_url",
    "plugin_repo_branch",
    "plugin_auto_update",
    "plugin_check_updates",
    "launcher_auto_update",
    "resource_targets",
];

/// 导出设置：按前端勾选的字段生成带版本标记的 JSON 字符串
#[tauri::command]
pub async fn export_settings(
    app: AppHandle,
    keys: Vec<String>,
) -> Result<serde_json::Value, String> {
    let game_root = app.state::<AppState>().game_root.clone();
    let settings = crate::models::AppSettings::load_or_default(&settings_path(&game_root));
    let value =
        serde_json::to_value(&settings).map_err(|e| format!("序列化设置失败: {}", e))?;

    let mut selected = serde_json::Map::new();
    for key in &keys {
        if !EXPORTABLE_SETTING_KEYS.contains(&key.as_str()) {
            return Err(format!("不支持的设置字段: {}", key));
        }
        if let Some(field) = value.get(key) {
            selected.insert(key.clone(), field.clone());
        }
    }
    if selected.is_empty() {
        return Err("请至少选择一个导出字段".to_string());
    }

    Ok(serde_json::json!({
        "type": "mscl-settings",
        "version": 1,
        "exported_at": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        "settings": selected,
    }))
}

/// 导入设置：只合并文件里出现的已知字段，校验后保存
#[tauri::command]
pub async fn import_settings(
    app: AppHandle,
    payload: serde_json::Value,
) -> Result<AppSettings, String> {
    let kind = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if kind != "mscl-settings" {
        return Err("不是有效的 MSCL 设置文件".to_string());
    }
    let imported = payload
        .get("settings")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "设置文件缺少 settings 字段".to_string())?;

    let game_root = app.state::<AppState>().game_root.clone();
    let path = settings_path(&game_root);
    let settings = crate::models::AppSettings::load_or_default(&path);
    let current = serde_json::to_value(&settings).map_err(|e| e.to_string())?;
    let mut merged = current
        .as_object()
        .cloned()
        .ok_or_else(|| "当前设置不是对象".to_string())?;

    let mut applied = Vec::new();
    for (key, field) in imported {
        if EXPORTABLE_SETTING_KEYS.contains(&key.as_str()) {
            merged.insert(key.clone(), field.clone());
            applied.push(key.clone());
        }
    }
    if applied.is_empty() {
        return Err("设置文件中没有可识别的字段".to_string());
    }

    let merged_value = serde_json::Value::Object(merged);
    let mut next: AppSettings = serde_json::from_value(merged_value)
        .map_err(|e| format!("设置字段格式错误: {}", e))?;

    // 复用保存校验
    if next.username.trim().is_empty() {
        return Err("游戏昵称不能为空".to_string());
    }
    if next.max_memory_mb > 65536 {
        return Err("内存分配不能超过 65536 MB".to_string());
    }
    if !(1..=32).contains(&next.download_concurrency) {
        return Err("下载并发数需在 1-32 之间".to_string());
    }
    if next.download_source.trim().is_empty() {
        return Err("下载源不能为空".to_string());
    }
    next.isolation_resources = isolation::normalize_resources(&next.isolation_resources);

    let text = serde_json::to_string_pretty(&next).map_err(|e| e.to_string())?;
    let tmp_path = game_root.join("settings.json.tmp");
    std::fs::write(&tmp_path, text).map_err(|e| format!("写入设置失败: {}", e))?;
    std::fs::rename(&tmp_path, &path).map_err(|e| format!("保存设置失败: {}", e))?;

    for warning in isolation::sync_all_instances(&game_root, &next) {
        emit_log(&app, "warn", format!("版本隔离：{}", warning));
    }
    Ok(next)
}

/// 导出设置到指定 JSON 文件
#[tauri::command]
pub async fn save_settings_file(
    app: AppHandle,
    path: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    let target = super::plugin::resolve_user_file(&app, &path)?;
    let text = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    std::fs::write(target, text).map_err(|e| format!("写入导出文件失败: {}", e))
}

/// 读取设置导出文件的 JSON 文本
#[tauri::command]
pub async fn read_settings_file(app: AppHandle, path: String) -> Result<String, String> {
    let target = super::plugin::resolve_user_file(&app, &path)?;
    let meta = std::fs::metadata(&target).map_err(|e| format!("读取文件失败: {}", e))?;
    if meta.len() > 1024 * 1024 {
        return Err("设置文件过大".to_string());
    }
    std::fs::read_to_string(target).map_err(|e| format!("读取文件失败: {}", e))
}

/// 查询某个实例的隔离实际状态（用于 UI 展示）
#[tauri::command]
pub async fn get_instance_isolation(
    app: AppHandle,
    version_id: String,
) -> Result<Vec<crate::commands::isolation::IsolationEntry>, String> {
    let game_root = app.state::<AppState>().game_root.clone();
    let settings = crate::models::AppSettings::load_or_default(&settings_path(&game_root));
    Ok(crate::commands::isolation::inspect_instance(
        &game_root,
        &version_id,
        &settings,
    ))
}

/// 查询全局隔离配置概览（总开关 + 各类型勾选状态）
#[tauri::command]
pub async fn get_isolation_overview(
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    let game_root = app.state::<AppState>().game_root.clone();
    let settings = crate::models::AppSettings::load_or_default(&settings_path(&game_root));
    let isolated = crate::commands::isolation::normalize_resources(&settings.isolation_resources);
    Ok(serde_json::json!({
        "version_isolation": settings.version_isolation,
        "types": crate::commands::isolation::RESOURCE_TYPES
            .iter()
            .map(|kind| serde_json::json!({
                "kind": kind,
                "isolated": isolated.iter().any(|value| value == kind),
            }))
            .collect::<Vec<_>>(),
    }))
}

/// 窗口控制（前端自定义标题栏按钮）
#[tauri::command]
pub async fn window_control(action: WindowAction, window: WebviewWindow) -> Result<(), String> {
    match action {
        WindowAction::Minimize => window.minimize().map_err(|e| e.to_string())?,
        WindowAction::Maximize => {
            let maximized = window.is_maximized().map_err(|e| e.to_string())?;
            if maximized {
                window.unmaximize().map_err(|e| e.to_string())?;
            } else {
                window.maximize().map_err(|e| e.to_string())?;
            }
        }
        WindowAction::Close => window.close().map_err(|e| e.to_string())?,
    }
    Ok(())
}

// ---------- 卸载启动器 ----------

/// 设置页展示的卸载器信息
#[derive(serde::Serialize, Clone)]
pub struct UninstallInfo {
    /// 当前平台是否支持独立卸载程序
    pub supported: bool,
    /// uninstall.exe 的完整路径（找不到时为 None）
    pub uninstaller: Option<String>,
    /// 安装目录（找不到时为 None）
    pub install_dir: Option<String>,
}

/// 从注册表读取安装目录（与安装器写入的 HKCU\Software\MSCL 保持一致）
#[cfg(windows)]
fn query_install_dir() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script = "try { (Get-ItemProperty -Path 'Registry::HKEY_CURRENT_USER\\Software\\MSCL' -Name 'InstallPath' -ErrorAction Stop).InstallPath } catch { }";
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd.output().ok()?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(PathBuf::from(value))
    }
}

/// 定位 (uninstall.exe, 安装目录)
fn locate_uninstaller() -> Option<(PathBuf, PathBuf)> {
    #[cfg(windows)]
    let from_registry = query_install_dir();
    #[cfg(not(windows))]
    let from_registry: Option<PathBuf> = None;

    let dir = from_registry.or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|parent| parent.to_path_buf()))
    })?;
    let uninstaller = dir.join("uninstall.exe");
    if uninstaller.is_file() {
        Some((uninstaller, dir))
    } else {
        None
    }
}

/// 查询卸载器是否可用
#[tauri::command]
pub async fn get_uninstall_info() -> Result<UninstallInfo, String> {
    let located = locate_uninstaller();
    Ok(UninstallInfo {
        supported: cfg!(windows),
        uninstaller: located
            .as_ref()
            .map(|(exe, _)| exe.to_string_lossy().into_owned()),
        install_dir: located
            .as_ref()
            .map(|(_, dir)| dir.to_string_lossy().into_owned()),
    })
}

/// 启动独立卸载程序并关闭启动器
///
/// 卸载程序自带警告确认页，并在启动器退出后完成文件 / 快捷方式 / 注册表清理。
#[tauri::command]
pub async fn uninstall_launcher(app: AppHandle) -> Result<(), String> {
    let (uninstaller, install_dir) = locate_uninstaller()
        .ok_or_else(|| "未找到卸载程序 uninstall.exe，请重新安装启动器后再试".to_string())?;

    let mut cmd = Command::new(&uninstaller);
    cmd.arg("--uninstall")
        .arg("--dir")
        .arg(&install_dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS：卸载器不随启动器退出而被回收
        cmd.creation_flags(0x0000_0008);
    }
    cmd.spawn()
        .map_err(|e| format!("启动卸载程序失败: {}", e))?;

    // 卸载器已经独立运行，关闭自身以防文件被占用
    app.exit(0);
    Ok(())
}
