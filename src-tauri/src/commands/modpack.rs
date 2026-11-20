//! 整合包导入（阶段 11）
//!
//! 当前支持 Modrinth 的 `.mrpack` 格式：
//!   - `modrinth.index.json`：声明 Minecraft / loader 依赖与文件清单
//!   - `overrides/`、`client-overrides/`：需要覆盖到实例目录的静态文件
//!
//! 导入目标为「已安装版本对应的实例目录」（`instances/{version_id}`），
//! 这样导入的 Mod 能直接被版本隔离机制使用。

use std::io::Read;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use crate::models::ImportMrpackResult;
use crate::state::AppState;

#[derive(Debug, serde::Deserialize)]
struct ModrinthIndex {
    #[serde(default)]
    name: String,
    #[serde(default)]
    version_id: Option<String>,
    #[serde(default)]
    files: Vec<ModrinthFile>,
    #[serde(default)]
    dependencies: std::collections::HashMap<String, String>,
}

#[derive(Debug, serde::Deserialize)]
struct ModrinthFile {
    path: String,
    #[serde(default)]
    downloads: Vec<String>,
    #[serde(default)]
    hashes: ModrinthHashes,
    #[serde(default)]
    env: Option<ModrinthEnv>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ModrinthHashes {
    #[serde(default)]
    sha1: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ModrinthEnv {
    #[serde(default)]
    client: Option<String>,
}

/// 仅允许安全的相对路径（禁止绝对路径与 `..` 穿越）
fn is_safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && !path.contains("..")
        && !path.contains(':')
}

/// 找到可用于导入的已安装版本：优先 Fabric 版本，其次原版
fn find_target_version(game_root: &Path, mc_version: &str) -> Option<String> {
    let versions_dir = game_root.join("versions");
    let suffix = format!("-{}", mc_version);

    let mut fabric: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&versions_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("fabric-loader-") && name.ends_with(&suffix) {
                fabric.push(name);
            }
        }
    }
    fabric.sort();
    if let Some(v) = fabric.pop() {
        return Some(v);
    }

    if versions_dir
        .join(mc_version)
        .join(format!("{}.json", mc_version))
        .exists()
    {
        return Some(mc_version.to_string());
    }

    None
}

/// 从 zip 中解压指定前缀的目录到目标根
fn extract_prefixed(
    archive: &mut zip::ZipArchive<std::fs::File>,
    prefix: &str,
    dest_root: &Path,
) -> Result<usize, String> {
    let mut count = 0usize;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("读取压缩包条目失败: {}", e))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let Some(rel) = name.strip_prefix(prefix) else {
            continue;
        };
        if rel.is_empty() || !is_safe_relative(rel) {
            continue;
        }

        let dest = dest_root.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建目录失败 {}: {}", parent.display(), e))?;
        }
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| format!("解压 {} 失败: {}", name, e))?;
        std::fs::write(&dest, &buf)
            .map_err(|e| format!("写入 {} 失败: {}", dest.display(), e))?;
        count += 1;
    }
    Ok(count)
}

async fn download_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
) -> Result<(), String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载失败 ({}): {}", url, e))?;
    if !response.status().is_success() {
        return Err(format!("下载 {} 返回状态 {}", url, response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("读取下载内容失败: {}", e))?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建目录失败 {}: {}", parent.display(), e))?;
    }
    std::fs::write(dest, &bytes).map_err(|e| format!("写入 {} 失败: {}", dest.display(), e))?;
    Ok(())
}

/// 导入 Modrinth 整合包（.mrpack）
#[tauri::command]
pub async fn import_mrpack(
    app: AppHandle,
    path: String,
    target_dir: Option<String>,
) -> Result<ImportMrpackResult, String> {
    let game_root = app.state::<AppState>().game_root.clone();

    // 1. 打开 zip 并读取 index
    let file = std::fs::File::open(&path).map_err(|e| format!("打开整合包失败: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("解析整合包失败（不是 zip）: {}", e))?;

    let mut index_text = String::new();
    archive
        .by_name("modrinth.index.json")
        .map_err(|_| "不是有效的 .mrpack（缺少 modrinth.index.json）".to_string())?
        .read_to_string(&mut index_text)
        .map_err(|e| format!("读取整合包清单失败: {}", e))?;
    let index: ModrinthIndex =
        serde_json::from_str(&index_text).map_err(|e| format!("解析整合包清单失败: {}", e))?;

    // 2. 确定目标版本
    let mc_version = index
        .dependencies
        .get("minecraft")
        .cloned()
        .ok_or_else(|| "整合包未声明 Minecraft 版本".to_string())?;
    let custom_root = super::plugin::resolve_install_root(&app, target_dir.as_deref())?;
    let target_version = match custom_root.as_ref() {
        Some(_) => mc_version.clone(),
        None => find_target_version(&game_root, &mc_version).ok_or_else(|| {
            format!(
                "找不到 {} 的可用版本，请先安装原版 {} 与 Fabric 加载器后再导入",
                mc_version, mc_version
            )
        })?,
    };

    let instance_dir = custom_root.unwrap_or_else(|| {
        game_root.join("instances").join(&target_version)
    });
    std::fs::create_dir_all(&instance_dir)
        .map_err(|e| format!("创建实例目录失败: {}", e))?;

    // 3. 下载文件清单
    let client = reqwest::Client::new();
    let mut downloaded = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for f in &index.files {
        if f.env
            .as_ref()
            .and_then(|e| e.client.as_deref())
            == Some("unsupported")
        {
            continue;
        }
        if !is_safe_relative(&f.path) {
            errors.push(format!("跳过非法路径: {}", f.path));
            continue;
        }
        let Some(url) = f.downloads.first() else {
            errors.push(format!("{} 没有下载地址", f.path));
            continue;
        };

        let dest = instance_dir.join(&f.path);
        if let Some(expected) = f.hashes.sha1.as_deref() {
            if !expected.is_empty()
                && dest.exists()
                && matches!(
                    mscl_core::verify_file(&dest, expected, 0),
                    Ok(mscl_core::VerifyOutcome::Valid)
                )
            {
                skipped += 1;
                continue;
            }
        }

        match download_file(&client, url, &dest).await {
            Ok(()) => {
                // 有哈希时校验一次，避免把错误内容当成功
                if let Some(expected) = f.hashes.sha1.as_deref() {
                    if !expected.is_empty()
                        && !matches!(
                            mscl_core::verify_file(&dest, expected, 0),
                            Ok(mscl_core::VerifyOutcome::Valid)
                        )
                    {
                        errors.push(format!("{} 校验失败（内容可能损坏）", f.path));
                        continue;
                    }
                }
                downloaded += 1;
            }
            Err(e) => errors.push(format!("{}: {}", f.path, e)),
        }
    }

    // 4. 解压 overrides（client-overrides 优先级更高，后写覆盖）
    let overrides = extract_prefixed(&mut archive, "overrides/", &instance_dir)?;
    let client_overrides = extract_prefixed(&mut archive, "client-overrides/", &instance_dir)?;

    // 5. 记录实例元信息，便于以后识别来源
    let meta = serde_json::json!({
        "name": index.name,
        "versionId": index.version_id,
        "minecraft": mc_version,
        "dependencies": index.dependencies,
        "targetVersion": target_version,
    });
    let meta_text = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    std::fs::write(instance_dir.join("mscl-pack.json"), meta_text)
        .map_err(|e| format!("写入实例信息失败: {}", e))?;

    Ok(ImportMrpackResult {
        pack_name: if index.name.is_empty() {
            "未命名整合包".to_string()
        } else {
            index.name
        },
        target_version,
        mc_version,
        downloaded,
        skipped,
        overrides: overrides + client_overrides,
        errors,
    })
}

/// 保留给未来：按路径直接读取 index（单测用）
#[allow(dead_code)]
fn read_index_from_path(path: &Path) -> Result<ModrinthIndex, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

#[allow(dead_code)]
fn instance_path(game_root: &Path, version_id: &str) -> PathBuf {
    game_root.join("instances").join(version_id)
}
