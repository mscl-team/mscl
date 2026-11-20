//! 自定义版本隔离：按资源类型在实例目录与共享目录之间建立目录链接。
//!
//! 设计：
//! - `version_isolation = false`：不处理，游戏直接使用共享根目录。
//! - `version_isolation = true`：游戏使用 `instances/{version}`。
//!   - 勾选的类型保留为实例内真实目录（真正隔离）。
//!   - 未勾选的类型创建指向共享根目录的 junction / symlink（共享）。
//! - 旧版本 settings.json 没有 `isolationResources` 时按“全部隔离”处理。

use std::path::Path;

use crate::models::{default_isolation_resources, AppSettings};

pub const RESOURCE_TYPES: [&str; 6] = [
    "saves",
    "mods",
    "resourcepacks",
    "shaderpacks",
    "config",
    "datapacks",
];

/// 过滤未知类型并去重；空列表按旧行为视为全部隔离
pub fn normalize_resources(values: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        if RESOURCE_TYPES.contains(&value.as_str()) && !out.contains(value) {
            out.push(value.clone());
        }
    }
    if out.is_empty() {
        default_isolation_resources()
    } else {
        out
    }
}

fn path_exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn is_link(path: &Path) -> bool {
    std::fs::read_link(path).is_ok()
}

#[cfg(windows)]
fn remove_dir_link(path: &Path) -> Result<(), String> {
    std::fs::remove_dir(path).map_err(|e| format!("移除目录链接失败 {}: {}", path.display(), e))
}

#[cfg(not(windows))]
fn remove_dir_link(path: &Path) -> Result<(), String> {
    std::fs::remove_file(path).map_err(|e| format!("移除目录链接失败 {}: {}", path.display(), e))
}

#[cfg(windows)]
fn create_dir_link(target: &Path, link: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let output = std::process::Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("创建目录链接失败: {}", e))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "创建目录链接失败 {} -> {}: {}",
            link.display(),
            target.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(not(windows))]
fn create_dir_link(target: &Path, link: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, link)
        .map_err(|e| format!("创建目录链接失败 {} -> {}: {}", link.display(), target.display(), e))
}

/// 确保该类型是实例内真实目录（隔离）
fn ensure_isolated_dir(path: &Path) -> Result<(), String> {
    if path_exists(path) {
        if is_link(path) {
            remove_dir_link(path)?;
        } else if path.is_dir() {
            return Ok(());
        } else {
            return Err(format!("{} 已存在且不是目录", path.display()));
        }
    }
    std::fs::create_dir_all(path).map_err(|e| format!("创建隔离目录失败 {}: {}", path.display(), e))
}

/// 确保该类型是指向共享目录的链接（共享）
fn ensure_shared_link(link: &Path, target: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target)
        .map_err(|e| format!("创建共享目录失败 {}: {}", target.display(), e))?;

    if path_exists(link) {
        if is_link(link) {
            if let Ok(current) = std::fs::read_link(link) {
                if current == target {
                    return Ok(());
                }
            }
            remove_dir_link(link)?;
        } else if link.is_dir() {
            let empty = std::fs::read_dir(link)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false);
            if !empty {
                return Err(format!(
                    "{} 已有独立文件，未自动切换为共享；请手动迁移后再试",
                    link.display()
                ));
            }
            std::fs::remove_dir(link)
                .map_err(|e| format!("移除空目录失败 {}: {}", link.display(), e))?;
        } else {
            return Err(format!("{} 已存在且不是目录", link.display()));
        }
    }

    create_dir_link(target, link)
}

/// 为单个实例同步目录链接，返回警告（不阻断启动）
pub fn ensure_instance_links(
    game_root: &Path,
    version_id: &str,
    settings: &AppSettings,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if !settings.version_isolation {
        return warnings;
    }

    let instance = game_root.join("instances").join(version_id);
    if let Err(e) = std::fs::create_dir_all(&instance) {
        warnings.push(format!("创建实例目录失败: {}", e));
        return warnings;
    }

    let isolated = normalize_resources(&settings.isolation_resources);
    for kind in RESOURCE_TYPES {
        let link = instance.join(kind);
        let target = game_root.join(kind);
        let result = if isolated.iter().any(|value| value == kind) {
            ensure_isolated_dir(&link)
        } else {
            ensure_shared_link(&link, &target)
        };
        if let Err(e) = result {
            warnings.push(format!("{}: {}", kind, e));
        }
    }
    warnings
}

/// 设置保存后同步所有实例，避免资源页安装到旧目录
pub fn sync_all_instances(game_root: &Path, settings: &AppSettings) -> Vec<String> {
    let mut warnings = Vec::new();
    if !settings.version_isolation {
        return warnings;
    }
    let Ok(entries) = std::fs::read_dir(game_root.join("instances")) else {
        return warnings;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || is_link(&path) {
            continue;
        }
        let version_id = entry.file_name().to_string_lossy().into_owned();
        warnings.extend(ensure_instance_links(game_root, &version_id, settings));
    }
    warnings
}

/// 单个实例某资源类型的实际状态
#[derive(Debug, Clone, serde::Serialize)]
pub struct IsolationEntry {
    pub kind: String,
    /// isolated | shared | shared_with_files | unknown
    pub state: String,
    /// 共享时指向的共享目录
    pub target: Option<String>,
}

/// 查询实例隔离实际状态（用于设置页与实例编辑页展示）
pub fn inspect_instance(game_root: &Path, version_id: &str, settings: &AppSettings) -> Vec<IsolationEntry> {
    let instance = game_root.join("instances").join(version_id);
    let isolated = normalize_resources(&settings.isolation_resources);
    RESOURCE_TYPES
        .iter()
        .map(|kind| {
            let link = instance.join(kind);
            let expected_isolated = isolated.iter().any(|value| value == kind);
            let (state, target) = if !settings.version_isolation {
                ("shared", None)
            } else if expected_isolated {
                if link.is_dir() && !is_link(&link) {
                    ("isolated", None)
                } else if is_link(&link) {
                    ("shared_with_files", None)
                } else if !link.exists() {
                    ("isolated", None)
                } else {
                    ("unknown", None)
                }
            } else if is_link(&link) {
                let target = std::fs::read_link(&link)
                    .map(|p| p.to_string_lossy().into_owned())
                    .ok();
                ("shared", target)
            } else if link.is_dir() {
                let empty = std::fs::read_dir(&link)
                    .map(|mut entries| entries.next().is_none())
                    .unwrap_or(false);
                if empty {
                    ("shared", None)
                } else {
                    ("shared_with_files", None)
                }
            } else {
                ("shared", None)
            };
            IsolationEntry {
                kind: kind.to_string(),
                state: state.to_string(),
                target,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AppSettings;
    use std::path::PathBuf;

    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mscl-isolation-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ))
    }

    fn cleanup(root: &Path) {
        for kind in RESOURCE_TYPES {
            let path = root.join("instances").join("v1").join(kind);
            if is_link(&path) {
                let _ = remove_dir_link(&path);
            }
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn empty_resource_list_means_all_isolated() {
        let normalized = normalize_resources(&[]);
        assert_eq!(normalized.len(), RESOURCE_TYPES.len());
    }

    #[test]
    fn datapacks_type_is_supported() {
        let normalized = normalize_resources(&["datapacks".to_string()]);
        assert_eq!(normalized, vec!["datapacks".to_string()]);
    }

    #[test]
    fn non_isolated_types_link_to_shared_folder() {
        let root = test_root("links");
        let mut settings = AppSettings::default();
        settings.version_isolation = true;
        settings.isolation_resources = vec!["saves".to_string(), "mods".to_string()];

        let warnings = ensure_instance_links(&root, "v1", &settings);
        assert!(warnings.is_empty(), "warnings: {:?}", warnings);

        let instance = root.join("instances").join("v1");
        assert!(instance.join("saves").is_dir());
        assert!(!is_link(&instance.join("saves")));
        assert!(is_link(&instance.join("config")));

        std::fs::write(root.join("config").join("test.txt"), "shared").unwrap();
        let text = std::fs::read_to_string(instance.join("config").join("test.txt")).unwrap();
        assert_eq!(text, "shared");

        cleanup(&root);
    }
}
