//! 启动器备份与还原：按内容勾选创建快照，还原前自动保护当前数据。

use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::state::{self, AppState};

use super::plugin::copy_dir_recursive;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupOptions {
    #[serde(default = "default_true")]
    pub settings: bool,
    #[serde(default)]
    pub plugins: bool,
    #[serde(default)]
    pub instances: bool,
    #[serde(default = "default_true")]
    pub accounts: bool,
    #[serde(default)]
    pub logs: bool,
    #[serde(default)]
    pub extra_paths: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl Default for BackupOptions {
    fn default() -> Self {
        Self {
            settings: true,
            plugins: false,
            instances: false,
            accounts: true,
            logs: false,
            extra_paths: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupItem {
    id: String,
    source: String,
    kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupManifest {
    schema: u32,
    created_at: u64,
    app_version: String,
    note: String,
    options: BackupOptions,
    items: Vec<BackupItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub id: String,
    pub path: String,
    pub created_at: u64,
    pub app_version: String,
    pub note: String,
    pub size: u64,
    pub items: Vec<String>,
}

fn backups_root(app: &AppHandle) -> PathBuf {
    app.state::<AppState>().game_root.join("backups")
}

fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn dir_size(path: &Path) -> u64 {
    if path.is_file() {
        return fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    }
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            total += dir_size(&entry.path());
        }
    }
    total
}

fn copy_entry(source: &Path, target: &Path) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    if source.is_dir() {
        copy_dir_recursive(source, target)
    } else if source.is_file() {
        fs::copy(source, target).map_err(|e| format!("复制失败: {}", e))?;
        Ok(())
    } else {
        Ok(())
    }
}

fn resolve_extra(game_root: &Path, value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("自定义路径不能包含上级目录".to_string());
    }
    if path.is_absolute() {
        let launcher_dir = state::launcher_config_path()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()));
        let allowed = path.starts_with(game_root)
            || launcher_dir.as_ref().map(|root| path.starts_with(root)).unwrap_or(false);
        if !allowed {
            return Err("自定义路径必须位于游戏数据目录或启动器配置目录内".to_string());
        }
        Ok(path)
    } else {
        Ok(game_root.join(path))
    }
}

fn collect_items(game_root: &Path, options: &BackupOptions) -> Result<Vec<BackupItem>, String> {
    let mut items = Vec::new();
    if options.settings {
        let source = game_root.join("settings.json");
        if source.exists() {
            items.push(BackupItem {
                id: "settings".to_string(),
                source: source.to_string_lossy().into_owned(),
                kind: "file".to_string(),
            });
        }
    }
    if options.plugins {
        let source = game_root.join("plugins");
        if source.exists() {
            items.push(BackupItem {
                id: "plugins".to_string(),
                source: source.to_string_lossy().into_owned(),
                kind: "dir".to_string(),
            });
        }
    }
    if options.instances {
        let root = game_root.join("instances");
        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.flatten() {
                if !entry.path().is_dir() {
                    continue;
                }
                for name in ["instance.json", "launch.json", ".mscl-resources.json"] {
                    let file = entry.path().join(name);
                    if file.is_file() {
                        items.push(BackupItem {
                            id: format!("instances/{}", entry.file_name().to_string_lossy()),
                            source: file.to_string_lossy().into_owned(),
                            kind: "file".to_string(),
                        });
                    }
                }
            }
        }
    }
    if options.accounts {
        if let Some(config) = state::launcher_config_path() {
            if config.exists() {
                items.push(BackupItem {
                    id: "launcher-config".to_string(),
                    source: config.to_string_lossy().into_owned(),
                    kind: "file".to_string(),
                });
            }
        }
    }
    if options.logs {
        let source = state::current_log_dir();
        if source.exists() {
            items.push(BackupItem {
                id: "logs".to_string(),
                source: source.to_string_lossy().into_owned(),
                kind: "dir".to_string(),
            });
        }
    }
    for (index, value) in options.extra_paths.iter().enumerate() {
        let source = resolve_extra(game_root, value)?;
        if source.exists() {
            items.push(BackupItem {
                id: format!("extra/{}", index),
                source: source.to_string_lossy().into_owned(),
                kind: if source.is_dir() { "dir" } else { "file" }.to_string(),
            });
        }
    }
    Ok(items)
}

#[tauri::command]
pub async fn create_launcher_backup(
    app: AppHandle,
    options: BackupOptions,
    note: String,
) -> Result<BackupSummary, String> {
    let game_root = app.state::<AppState>().game_root.clone();
    let items = collect_items(&game_root, &options)?;
    if items.is_empty() {
        return Err("没有可备份的内容，请先勾选备份项".to_string());
    }
    let id = format!("backup-{}", timestamp());
    let root = backups_root(&app).join(&id);
    fs::create_dir_all(&root).map_err(|e| format!("创建备份目录失败: {}", e))?;

    for item in &items {
        let target = root.join(&item.id);
        copy_entry(Path::new(&item.source), &target)?;
    }
    let manifest = BackupManifest {
        schema: 1,
        created_at: timestamp(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        note,
        options,
        items,
    };
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    fs::write(root.join("manifest.json"), text).map_err(|e| format!("写入备份清单失败: {}", e))?;
    summarize(&id, &root, &manifest, dir_size(&root))
}

fn load_manifest(root: &Path) -> Result<BackupManifest, String> {
    let text = fs::read_to_string(root.join("manifest.json"))
        .map_err(|e| format!("读取备份清单失败: {}", e))?;
    let manifest: BackupManifest =
        serde_json::from_str(&text).map_err(|e| format!("解析备份清单失败: {}", e))?;
    if manifest.schema != 1 {
        return Err("备份版本不受支持".to_string());
    }
    Ok(manifest)
}

fn summarize(
    id: &str,
    root: &Path,
    manifest: &BackupManifest,
    size: u64,
) -> Result<BackupSummary, String> {
    Ok(BackupSummary {
        id: id.to_string(),
        path: root.to_string_lossy().into_owned(),
        created_at: manifest.created_at,
        app_version: manifest.app_version.clone(),
        note: manifest.note.clone(),
        size,
        items: manifest.items.iter().map(|item| item.id.clone()).collect(),
    })
}

fn backup_dir(app: &AppHandle, path: &str) -> Result<PathBuf, String> {
    let root = backups_root(app);
    let target = PathBuf::from(path)
        .canonicalize()
        .map_err(|_| "备份目录不存在".to_string())?;
    let base = root
        .canonicalize()
        .unwrap_or_else(|_| root.clone());
    if !target.starts_with(&base) || target == base {
        return Err("只能操作游戏数据目录内的备份".to_string());
    }
    Ok(target)
}

#[tauri::command]
pub async fn list_launcher_backups(app: AppHandle) -> Result<Vec<BackupSummary>, String> {
    let root = backups_root(&app);
    let mut result = Vec::new();
    if !root.exists() {
        return Ok(result);
    }
    for entry in fs::read_dir(root).map_err(|e| e.to_string())?.flatten() {
        if !entry.path().is_dir() || !entry.path().join("manifest.json").exists() {
            continue;
        }
        if let Ok(manifest) = load_manifest(&entry.path()) {
            let id = entry.file_name().to_string_lossy().into_owned();
            if let Ok(summary) = summarize(&id, &entry.path(), &manifest, dir_size(&entry.path())) {
                result.push(summary);
            }
        }
    }
    result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(result)
}

/// 还原目标由备份项 ID 重新推导，不信任 manifest 中的 source 字段。
fn restore_destination(
    game_root: &Path,
    item_id: &str,
    extra_paths: &[String],
) -> Result<PathBuf, String> {
    let parts: Vec<&str> = item_id.split('/').collect();
    match parts.as_slice() {
        ["settings"] => Ok(game_root.join("settings.json")),
        ["plugins"] => Ok(game_root.join("plugins")),
        ["launcher-config"] => state::launcher_config_path()
            .ok_or_else(|| "无法定位启动器配置目录".to_string()),
        ["logs"] => Ok(state::current_log_dir()),
        ["instances", version, file] => {
            if version.is_empty() || version.contains("..") || version.contains('/') || version.contains('\\') {
                return Err("备份项中的实例标识无效".to_string());
            }
            if !["instance.json", "launch.json", ".mscl-resources.json"].contains(file) {
                return Err("备份项中的实例文件无效".to_string());
            }
            Ok(game_root.join("instances").join(version).join(file))
        }
        ["extra", index] => {
            let index = index.parse::<usize>().map_err(|_| "备份项序号无效".to_string())?;
            let value = extra_paths
                .get(index)
                .ok_or_else(|| "该备份项缺少对应的自定义路径".to_string())?;
            resolve_extra(game_root, value)
        }
        _ => Err("备份项标识无效".to_string()),
    }
}

fn rollback_restore(safety_root: &Path, applied: &[(String, PathBuf)]) {
    for (id, destination) in applied.iter().rev() {
        let saved = safety_root.join(id);
        if destination.is_dir() {
            let _ = fs::remove_dir_all(destination);
        } else if destination.exists() {
            let _ = fs::remove_file(destination);
        }
        if saved.exists() {
            let _ = copy_entry(&saved, destination);
        }
    }
}

#[tauri::command]
pub async fn restore_launcher_backup(
    app: AppHandle,
    path: String,
    options: BackupOptions,
) -> Result<String, String> {
    let source_root = backup_dir(&app, &path)?;
    let source_base = source_root
        .canonicalize()
        .map_err(|e| format!("读取备份目录失败: {}", e))?;
    let manifest = load_manifest(&source_root)?;
    let game_root = app.state::<AppState>().game_root.clone();
    let safety_root = backups_root(&app).join(format!("restore-safety-{}", timestamp()));
    fs::create_dir_all(&safety_root).map_err(|e| format!("创建保护备份失败: {}", e))?;

    let selected: Vec<&BackupItem> = manifest
        .items
        .iter()
        .filter(|item| {
            let extra_index = item
                .id
                .strip_prefix("extra/")
                .and_then(|value| value.parse::<usize>().ok());
            item.id == "settings" && options.settings
                || item.id == "plugins" && options.plugins
                || item.id.starts_with("instances/") && options.instances
                || item.id == "launcher-config" && options.accounts
                || item.id == "logs" && options.logs
                || extra_index.map(|i| i < options.extra_paths.len()).unwrap_or(false)
        })
        .collect();
    if selected.is_empty() {
        let _ = fs::remove_dir_all(&safety_root);
        return Err("没有勾选要还原的内容".to_string());
    }

    let mut applied: Vec<(String, PathBuf)> = Vec::new();
    for item in &selected {
        let source = source_root.join(&item.id);
        let source = match source.canonicalize() {
            Ok(path) if path.starts_with(&source_base) => path,
            _ => {
                rollback_restore(&safety_root, &applied);
                let _ = fs::remove_dir_all(&safety_root);
                return Err("备份项路径无效，已取消还原".to_string());
            }
        };
        if !source.exists() {
            continue;
        }
        let destination = match restore_destination(&game_root, &item.id, &options.extra_paths) {
            Ok(path) => path,
            Err(e) => {
                rollback_restore(&safety_root, &applied);
                let _ = fs::remove_dir_all(&safety_root);
                return Err(format!("{},已取消还原", e));
            }
        };
        if let Err(e) = copy_entry(&destination, &safety_root.join(&item.id)) {
            rollback_restore(&safety_root, &applied);
            let _ = fs::remove_dir_all(&safety_root);
            return Err(format!("保护当前数据失败，已取消还原: {}", e));
        }
        if let Err(e) = copy_entry(&source, &destination) {
            applied.push((item.id.clone(), destination));
            rollback_restore(&safety_root, &applied);
            let _ = fs::remove_dir_all(&safety_root);
            return Err(format!("还原失败，已撤销本次改动: {}", e));
        }
        applied.push((item.id.clone(), destination));
    }
    let _ = fs::remove_dir_all(&safety_root);
    Ok(format!("已还原 {} 项内容", applied.len()))
}

#[tauri::command]
pub async fn delete_launcher_backup(app: AppHandle, path: String) -> Result<(), String> {
    let target = backup_dir(&app, &path)?;
    fs::remove_dir_all(target).map_err(|e| format!("删除备份失败: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_path() {
        let root = Path::new("D:/MSCL");
        assert!(resolve_extra(root, "../secret").is_err());
        assert_eq!(resolve_extra(root, "instances/demo").unwrap(), root.join("instances/demo"));
    }
}
