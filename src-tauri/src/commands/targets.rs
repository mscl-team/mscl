//! 资源安装目标：实例、本地目录、服务端与多人游戏目录的统一枚举。

use std::fs;
use std::path::{Component, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::models::{AppSettings, ResourceTargetDir};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTarget {
    pub id: String,
    pub label: String,
    pub path: String,
    /// instance | local | server | multiplayer | launcher
    pub kind: String,
}

fn validate_dir_path(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value.trim());
    if path.as_os_str().is_empty() {
        return Err("目录不能为空".to_string());
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("目录不能包含上级路径".to_string());
    }
    Ok(path)
}

#[tauri::command]
pub async fn list_resource_targets(app: AppHandle) -> Result<Vec<ResourceTarget>, String> {
    let game_root = app.state::<AppState>().game_root.clone();
    let settings = AppSettings::load_or_default(&game_root.join("settings.json"));
    let mut targets = Vec::new();

    let instances = game_root.join("instances");
    if let Ok(entries) = fs::read_dir(&instances) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let name = fs::read_to_string(entry.path().join("instance.json"))
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .and_then(|json| json.get("name").and_then(|v| v.as_str()).map(String::from))
                .unwrap_or_else(|| entry.file_name().to_string_lossy().into_owned());
            let id = entry.file_name().to_string_lossy().into_owned();
            targets.push(ResourceTarget {
                id: format!("instance:{}", id),
                label: name,
                path: entry.path().to_string_lossy().into_owned(),
                kind: "instance".to_string(),
            });
        }
    }

    for (folder, kind, label) in [
        ("local-resources", "local", "本地资源目录"),
        ("servers", "server", "服务器目录"),
        ("multiplayer", "multiplayer", "多人游戏目录"),
    ] {
        let root = game_root.join(folder);
        targets.push(ResourceTarget {
            id: format!("{}:default", kind),
            label: label.to_string(),
            path: root.to_string_lossy().into_owned(),
            kind: kind.to_string(),
        });
        if kind == "local" || !root.exists() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.flatten() {
                if !entry.path().is_dir() {
                    continue;
                }
                let id = entry.file_name().to_string_lossy().into_owned();
                targets.push(ResourceTarget {
                    id: format!("{}:{}", kind, id),
                    label: format!("{} / {}", label, id),
                    path: entry.path().to_string_lossy().into_owned(),
                    kind: kind.to_string(),
                });
            }
        }
    }

    for dir in settings.resource_targets {
        targets.push(ResourceTarget {
            id: dir.id,
            label: dir.name,
            path: dir.path,
            kind: dir.kind,
        });
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            targets.push(ResourceTarget {
                id: "launcher:self".to_string(),
                label: "启动器安装目录".to_string(),
                path: parent.to_string_lossy().into_owned(),
                kind: "launcher".to_string(),
            });
        }
    }
    targets.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.label.cmp(&b.label)));
    Ok(targets)
}

#[tauri::command]
pub async fn add_resource_target_dir(
    app: AppHandle,
    input: ResourceTargetDir,
) -> Result<Vec<ResourceTargetDir>, String> {
    if input.name.trim().is_empty() {
        return Err("目录名称不能为空".to_string());
    }
    if !["local", "server", "multiplayer"].contains(&input.kind.as_str()) {
        return Err("目录类型不受支持".to_string());
    }
    let path = validate_dir_path(&input.path)?;
    let game_root = app.state::<AppState>().game_root.clone();
    let settings_path = game_root.join("settings.json");
    let mut settings = AppSettings::load_or_default(&settings_path);
    let mut item = input;
    item.path = path.to_string_lossy().into_owned();
    if item.id.trim().is_empty() {
        item.id = format!("{}-{}", item.kind, path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "custom".to_string()));
    }
    if settings.resource_targets.iter().any(|target| target.id == item.id) {
        return Err("该目录已添加".to_string());
    }
    settings.resource_targets.push(item);
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("保存设置失败: {}", e))?;
    Ok(settings.resource_targets)
}

#[tauri::command]
pub async fn remove_resource_target_dir(
    app: AppHandle,
    id: String,
) -> Result<Vec<ResourceTargetDir>, String> {
    let game_root = app.state::<AppState>().game_root.clone();
    let settings_path = game_root.join("settings.json");
    let mut settings = AppSettings::load_or_default(&settings_path);
    settings.resource_targets.retain(|target| target.id != id);
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("保存设置失败: {}", e))?;
    Ok(settings.resource_targets)
}
