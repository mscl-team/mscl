//! 维护命令（阶段 15）
//! 重置设置 / 更改、导入、删除游戏目录

use tauri::{AppHandle, Manager};

use crate::models::{AppSettings, CleanupResult, GameDirInfo, GameRootInfo, InstanceInfo, LogDirInfo};
use crate::state::{self, AppState};

use super::instance::scan_versions;

fn dir_info(path: &std::path::Path, is_custom: bool) -> GameDirInfo {
    let installed_versions = std::fs::read_dir(path.join("versions"))
        .map(|entries| entries.flatten().count())
        .unwrap_or(0);
    GameDirInfo {
        path: path.to_string_lossy().into_owned(),
        exists: path.exists(),
        installed_versions,
        is_custom,
    }
}

/// 当前游戏目录信息
#[tauri::command]
pub async fn get_game_dir_info(app: AppHandle) -> Result<GameDirInfo, String> {
    let root = app.state::<AppState>().game_root.clone();
    Ok(dir_info(&root, state::load_custom_game_root().is_some()))
}

/// 更改 / 导入游戏目录（重启后生效）
#[tauri::command]
pub async fn set_game_dir(path: String) -> Result<GameDirInfo, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("目录路径不能为空".to_string());
    }

    let target = std::path::PathBuf::from(trimmed);
    if target.parent().is_none() {
        return Err("不能使用盘符根目录作为游戏目录".to_string());
    }

    std::fs::create_dir_all(&target).map_err(|e| format!("创建目录失败: {}", e))?;
    state::save_custom_game_root(&target)?;

    Ok(dir_info(&target, true))
}

/// 用系统浏览器打开外部链接（MineBBS 等需要网页下载的来源）
#[tauri::command]
pub async fn open_external_url(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    let trimmed = url.trim();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("只允许打开 http(s) 链接".to_string());
    }
    app.opener()
        .open_url(trimmed, None::<&str>)
        .map_err(|e| format!("打开链接失败: {}", e))
}

/// 重置所有设置（不删除游戏文件）
#[tauri::command]
pub async fn reset_settings(app: AppHandle) -> Result<AppSettings, String> {
    let root = app.state::<AppState>().game_root.clone();
    let defaults = AppSettings::default();
    let text = serde_json::to_string_pretty(&defaults).map_err(|e| e.to_string())?;
    std::fs::write(root.join("settings.json"), text)
        .map_err(|e| format!("写入设置失败: {}", e))?;
    // 日志目录也属于启动器设置，重置时一并恢复默认
    state::reset_custom_log_dir()?;
    Ok(defaults)
}

/// 删除游戏数据目录（危险操作，需前端二次确认）
#[tauri::command]
pub async fn delete_game_data(app: AppHandle, confirm: String) -> Result<(), String> {
    if confirm.trim() != "DELETE" {
        return Err("确认文本不正确，已取消".to_string());
    }

    // 先停掉可能运行中的游戏（可能有多开实例）
    for instance in app.state::<AppState>().clear_running() {
        let _ = mscl_platform::kill_process_by_pid(instance.pid);
    }

    let root = app.state::<AppState>().game_root.clone();

    // 安全护栏：拒绝盘根和明显的系统目录
    if root.parent().is_none() {
        return Err("拒绝删除盘符根目录".to_string());
    }
    let lower = root.to_string_lossy().to_lowercase();
    let depth = lower.matches(['\\', '/']).count();
    if (lower.contains("windows") || lower.contains("program files")) && depth <= 2 {
        return Err("拒绝删除系统目录".to_string());
    }
    // 至少要有两层目录（例如 D:\MC\data），避免误删整个分区
    if depth < 2 {
        return Err("目录层级过浅，已拒绝删除".to_string());
    }

    if root.exists() {
        std::fs::remove_dir_all(&root).map_err(|e| format!("删除失败: {}", e))?;
    }
    Ok(())
}

fn is_log_file(name: &std::ffi::OsStr) -> bool {
    let lower = name.to_string_lossy().to_lowercase();
    lower.ends_with(".log") || lower.contains(".log.")
}

fn is_partial_download(name: &std::ffi::OsStr) -> bool {
    let raw = name.to_string_lossy();
    if let Some(index) = raw.find(".part-") {
        let tail = &raw[index + 6..];
        return !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit());
    }
    false
}

fn log_dir_info() -> LogDirInfo {
    let dir = state::current_log_dir();
    let mut file_count = 0usize;
    let mut total_bytes = 0u64;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_log_file(&entry.file_name()) {
                file_count += 1;
                total_bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    LogDirInfo {
        path: dir.to_string_lossy().into_owned(),
        exists: dir.exists(),
        is_custom: state::load_custom_log_dir().is_some(),
        file_count,
        total_bytes,
    }
}

/// 当前日志目录信息
#[tauri::command]
pub async fn get_log_dir_info() -> Result<LogDirInfo, String> {
    Ok(log_dir_info())
}

/// 更改日志文件存储目录（立即生效，无需重启）
#[tauri::command]
pub async fn set_log_dir(path: String) -> Result<LogDirInfo, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("日志目录不能为空".to_string());
    }
    let target = std::path::PathBuf::from(trimmed);
    if target.parent().is_none() {
        return Err("不能使用盘符根目录作为日志目录".to_string());
    }
    std::fs::create_dir_all(&target).map_err(|e| format!("创建日志目录失败: {}", e))?;
    state::save_custom_log_dir(&target)?;
    Ok(log_dir_info())
}

/// 恢复默认日志目录
#[tauri::command]
pub async fn reset_log_dir() -> Result<LogDirInfo, String> {
    state::reset_custom_log_dir()?;
    Ok(log_dir_info())
}

/// 在系统文件管理器中打开日志目录
#[tauri::command]
pub async fn open_log_dir(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = state::current_log_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建日志目录失败: {}", e))?;
    app.opener()
        .open_path(dir.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|e| format!("打开日志目录失败: {}", e))
}

/// 递归删除目录下匹配的文件，统计释放空间
fn remove_matching_files(
    dir: &std::path::Path,
    predicate: fn(&std::ffi::OsStr) -> bool,
    result: &mut CleanupResult,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            remove_matching_files(&path, predicate, result);
            continue;
        }
        if !predicate(&entry.file_name()) {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if std::fs::remove_file(&path).is_ok() {
            result.deleted_files += 1;
            result.freed_bytes += size;
            if result.paths.len() < 50 {
                result.paths.push(path.to_string_lossy().into_owned());
            }
        }
    }
}

/// 清理日志文件（只删 *.log / *.log.*，保留目录本身）
#[tauri::command]
pub async fn clear_log_files() -> Result<CleanupResult, String> {
    let dir = state::current_log_dir();
    let mut result = CleanupResult {
        deleted_files: 0,
        deleted_dirs: 0,
        freed_bytes: 0,
        paths: Vec::new(),
    };
    remove_matching_files(&dir, is_log_file, &mut result);
    Ok(result)
}

/// 清理启动器缓存：配置目录 cache、游戏目录 .mscl-cache、残留的 .part-<pid> 下载临时文件
#[tauri::command]
pub async fn clear_cache(app: AppHandle) -> Result<CleanupResult, String> {
    let game_root = app.state::<AppState>().game_root.clone();
    let mut result = CleanupResult {
        deleted_files: 0,
        deleted_dirs: 0,
        freed_bytes: 0,
        paths: Vec::new(),
    };

    if let Some(dir) = state::launcher_cache_dir() {
        remove_matching_files(&dir, |_| true, &mut result);
    }
    remove_matching_files(&game_root.join(".mscl-cache"), |_| true, &mut result);
    for sub in ["versions", "instances"] {
        remove_matching_files(&game_root.join(sub), is_partial_download, &mut result);
    }

    Ok(result)
}

fn paths_equal(left: &std::path::Path, right: &std::path::Path) -> bool {
    if let (Ok(a), Ok(b)) = (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        return a == b;
    }
    left.to_string_lossy().to_lowercase() == right.to_string_lossy().to_lowercase()
}

fn game_root_info(path: &std::path::Path, primary: &std::path::Path) -> GameRootInfo {
    GameRootInfo {
        path: path.to_string_lossy().into_owned(),
        is_primary: paths_equal(path, primary),
        exists: path.is_dir(),
        instance_count: scan_versions(path).len(),
    }
}

fn collect_game_roots(app: &AppHandle) -> Result<Vec<GameRootInfo>, String> {
    let primary = app.state::<AppState>().game_root.clone();
    let roots = state::all_game_roots()?;
    Ok(roots
        .iter()
        .map(|path| game_root_info(path, &primary))
        .collect())
}

/// 已注册的游戏目录（主目录 + 额外目录）
#[tauri::command]
pub async fn list_game_roots(app: AppHandle) -> Result<Vec<GameRootInfo>, String> {
    collect_game_roots(&app)
}

/// 添加额外游戏目录，用于跨目录显示/启动实例
#[tauri::command]
pub async fn add_game_root(app: AppHandle, path: String) -> Result<Vec<GameRootInfo>, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("目录路径不能为空".to_string());
    }
    let target = std::path::PathBuf::from(trimmed);
    if target.parent().is_none() {
        return Err("不能使用盘符根目录作为游戏目录".to_string());
    }
    std::fs::create_dir_all(&target).map_err(|e| format!("创建游戏目录失败: {}", e))?;
    let primary = app.state::<AppState>().game_root.clone();
    let mut extras = state::load_extra_game_roots();
    if !paths_equal(&target, &primary) && !extras.iter().any(|item| paths_equal(item, &target)) {
        extras.push(target);
        state::save_extra_game_roots(&extras)?;
    }
    collect_game_roots(&app)
}

/// 移除额外游戏目录（主目录不可移除）
#[tauri::command]
pub async fn remove_game_root(app: AppHandle, path: String) -> Result<Vec<GameRootInfo>, String> {
    let target = std::path::PathBuf::from(path);
    let primary = app.state::<AppState>().game_root.clone();
    if paths_equal(&target, &primary) {
        return Err("主游戏目录不能移除".to_string());
    }
    let extras = state::load_extra_game_roots();
    let kept: Vec<_> = extras
        .into_iter()
        .filter(|item| !paths_equal(item, &target))
        .collect();
    state::save_extra_game_roots(&kept)?;
    collect_game_roots(&app)
}

/// 扫描所有已注册游戏目录中的实例
#[tauri::command]
pub async fn list_all_instances(app: AppHandle) -> Result<Vec<InstanceInfo>, String> {
    let primary = app.state::<AppState>().game_root.clone();
    let roots = state::all_game_roots()?;
    let mut out = Vec::new();
    for root in roots {
        let mut instances = scan_versions(&root);
        // 主目录实例保持原有顺序和字段，额外目录通过 game_root 区分
        if paths_equal(&root, &primary) {
            for item in &mut instances {
                item.game_root = root.to_string_lossy().into_owned();
            }
        }
        out.extend(instances);
    }
    Ok(out)
}
