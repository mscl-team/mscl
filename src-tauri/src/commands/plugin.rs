//! 插件系统命令：清单扫描、启停、安装、卸载、安全更新与配置导入导出。

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub entry: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default = "default_layer")]
    pub layer: String,
    #[serde(default)]
    pub hooks: Vec<String>,
    #[serde(default, alias = "extension_points")]
    pub extension_points: Vec<serde_json::Value>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default, alias = "config_schema")]
    pub config_schema: Vec<serde_json::Value>,
    #[serde(default)]
    pub order: u32,
    #[serde(default)]
    pub schema: u32,
}

fn default_version() -> String { "0.1.0".to_string() }
fn default_category() -> String { "launcher".to_string() }
fn default_source() -> String { "local".to_string() }
fn default_layer() -> String { "feature".to_string() }
fn default_true() -> bool { true }

impl Default for PluginManifest {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            version: default_version(),
            category: default_category(),
            description: String::new(),
            author: String::new(),
            entry: String::new(),
            enabled: true,
            builtin: false,
            required: false,
            source: default_source(),
            repo: None,
            capabilities: Vec::new(),
            icon: None,
            layer: default_layer(),
            hooks: Vec::new(),
            extension_points: Vec::new(),
            dependencies: Vec::new(),
            config_schema: Vec::new(),
            order: 100,
            schema: 1,
        }
    }
}

/// 插件条目（manifest + 安装路径 + 是否可运行）
#[derive(Debug, Clone, Serialize)]
pub struct PluginEntry {
    #[serde(flatten)]
    pub manifest: PluginManifest,
    /// 安装目录
    pub path: String,
    /// 是否实际加载（enabled 且 manifest 合法）
    pub loaded: bool,
}

fn plugins_root(app: &AppHandle) -> PathBuf {
    app.state::<AppState>().game_root.join("plugins")
}

fn read_manifest(dir: &Path) -> Option<PluginManifest> {
    let text = fs::read_to_string(dir.join("plugin.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// 扫描已安装插件
#[tauri::command]
pub async fn list_installed_plugins(app: AppHandle) -> Result<Vec<PluginEntry>, String> {
    let root = plugins_root(&app);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(&root).map_err(|e| e.to_string())?.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        if let Some(manifest) = read_manifest(&entry.path()) {
            let id = manifest.id.clone();
            let enabled = manifest.enabled;
            result.push(PluginEntry {
                manifest,
                path: entry.path().to_string_lossy().into_owned(),
                loaded: enabled && !id.is_empty(),
            });
        }
    }
    result.sort_by(|a, b| {
        a.manifest
            .category
            .cmp(&b.manifest.category)
            .then(a.manifest.id.cmp(&b.manifest.id))
    });
    Ok(result)
}

/// 读取单个插件 manifest
#[tauri::command]
pub async fn get_plugin_manifest(app: AppHandle, id: String) -> Result<PluginManifest, String> {
    validate_id(&id)?;
    let path = plugins_root(&app).join(&id).join("plugin.json");
    let text = fs::read_to_string(&path)
        .map_err(|e| format!("读取插件清单失败: {}", e))?;
    serde_json::from_str(&text).map_err(|e| format!("解析插件清单失败: {}", e))
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("插件 ID 只能包含小写字母、数字和短横线（1-64 位）".to_string());
    }
    Ok(())
}

/// 导出/导入文件只允许写在用户文档、下载、桌面、游戏数据目录或临时目录内，
/// 防止通过 IPC 传入任意路径覆盖系统文件。
pub(crate) fn resolve_user_file(app: &AppHandle, path: &str) -> Result<PathBuf, String> {
    if !path.ends_with(".json") {
        return Err("文件必须是 .json 格式".to_string());
    }
    let candidate = PathBuf::from(path);
    if candidate.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("文件路径不能包含上级目录".to_string());
    }
    let parent = candidate
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| "文件路径缺少父目录".to_string())?;
    let base = parent
        .canonicalize()
        .map_err(|_| "目标目录不存在，请先选择已存在的目录".to_string())?;

    let mut allowed = vec![app.state::<AppState>().game_root.clone()];
    for key in ["USERPROFILE", "HOME"] {
        if let Some(home) = std::env::var_os(key) {
            for folder in ["Documents", "Downloads", "Desktop", "文档", "下载", "桌面"] {
                allowed.push(PathBuf::from(&home).join(folder));
            }
        }
    }
    allowed.push(std::env::temp_dir());

    if !allowed.iter().any(|root| base.starts_with(root)) {
        return Err("只能保存到用户文档、下载、桌面或游戏数据目录".to_string());
    }
    Ok(candidate)
}

/// 把内置模块清单落盘为真实插件目录，供安装时未勾选的模块后续下载安装。
#[tauri::command]
pub async fn install_builtin_plugin(
    app: AppHandle,
    manifest: serde_json::Value,
    target_dir: Option<String>,
) -> Result<PluginEntry, String> {
    let id = manifest
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "插件清单缺少 id".to_string())?
        .to_string();
    validate_id(&id)?;
    let root = resolve_install_root(&app, target_dir.as_deref())?
        .unwrap_or_else(|| plugins_root(&app));
    let dir = root.join(&id);
    if dir.exists() {
        return Err("该模块已安装".to_string());
    }
    let mut value = manifest;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "插件清单必须是对象".to_string())?;
    object.insert("builtin".to_string(), serde_json::Value::Bool(true));
    object.insert("enabled".to_string(), serde_json::Value::Bool(true));
    object.insert("source".to_string(), serde_json::Value::String("builtin".to_string()));
    let parsed: PluginManifest = serde_json::from_value(value.clone())
        .map_err(|e| format!("插件清单格式错误: {}", e))?;
    fs::create_dir_all(&dir).map_err(|e| format!("创建插件目录失败: {}", e))?;
    fs::write(
        dir.join("plugin.json"),
        serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("写入插件清单失败: {}", e))?;
    Ok(PluginEntry {
        manifest: parsed,
        path: dir.to_string_lossy().into_owned(),
        loaded: true,
    })
}

/// 启用 / 禁用插件（必需插件不可禁用）
#[tauri::command]
pub async fn set_plugin_enabled(
    app: AppHandle,
    id: String,
    enabled: bool,
) -> Result<PluginManifest, String> {
    validate_id(&id)?;
    let path = plugins_root(&app).join(&id).join("plugin.json");
    let text = fs::read_to_string(&path).map_err(|e| format!("读取插件清单失败: {}", e))?;
    let mut manifest: PluginManifest =
        serde_json::from_str(&text).map_err(|e| format!("解析插件清单失败: {}", e))?;
    if !enabled && manifest.required {
        return Err("该插件为核心必需插件，无法禁用".to_string());
    }
    manifest.enabled = enabled;
    fs::write(&path, serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?)
        .map_err(|e| format!("保存插件清单失败: {}", e))?;
    Ok(manifest)
}

/// 资源/插件安装目标必须在可信根目录内，避免通过 IPC 写入任意绝对路径。
pub(crate) fn resolve_install_root(
    app: &AppHandle,
    target_dir: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    let value = match target_dir.map(str::trim).filter(|s| !s.is_empty()) {
        Some(value) => value,
        None => return Ok(None),
    };
    let path = PathBuf::from(value);
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("安装目录不能包含上级路径".to_string());
    }
    let canonical = if path.exists() {
        path.canonicalize()
            .map_err(|e| format!("无法访问安装目录: {}", e))?
    } else {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| "安装目录无效".to_string())?;
        parent
            .canonicalize()
            .map_err(|_| "安装目录的父目录不存在".to_string())?
            .join(path.file_name().ok_or_else(|| "安装目录无效".to_string())?)
    };

    let mut allowed = vec![app.state::<AppState>().game_root.clone()];
    if let Some(config) = crate::state::launcher_config_path() {
        if let Some(parent) = config.parent() {
            allowed.push(parent.to_path_buf());
        }
    }
    allowed.push(std::env::temp_dir());
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            allowed.push(parent.to_path_buf());
        }
    }
    let settings = crate::models::AppSettings::load_or_default(
        &app.state::<AppState>().game_root.join("settings.json"),
    );
    for target in settings.resource_targets {
        allowed.push(PathBuf::from(target.path));
    }
    let trusted = allowed.iter().any(|root| {
        root.canonicalize()
            .map(|base| canonical.starts_with(base))
            .unwrap_or(false)
    });
    if !trusted {
        return Err("安装目录不在允许范围内，请在设置 → 资源来源中添加该目录".to_string());
    }
    Ok(Some(canonical))
}

/// 从本地目录安装插件（复制整个目录到 plugins/<id>）
#[tauri::command]
pub async fn install_local_plugin(
    app: AppHandle,
    source_dir: String,
    target_dir: Option<String>,
) -> Result<PluginEntry, String> {
    let src = PathBuf::from(&source_dir);
    if !src.is_dir() {
        return Err("插件目录不存在".to_string());
    }
    let manifest = read_manifest(&src).ok_or_else(|| "插件目录缺少合法的 plugin.json".to_string())?;
    validate_id(&manifest.id)?;
    let root = resolve_install_root(&app, target_dir.as_deref())?
        .unwrap_or_else(|| plugins_root(&app));
    let dest = root.join(&manifest.id);
    if dest.exists() {
        return Err("插件已安装，请先卸载".to_string());
    }
    copy_dir_recursive(&src, &dest)?;
    Ok(PluginEntry {
        manifest,
        path: dest.to_string_lossy().into_owned(),
        loaded: true,
    })
}

/// 从 GitHub 仓库安装插件（框架：下载仓库 zip，解析根目录 plugin.json）
#[tauri::command]
pub async fn install_github_plugin(
    _app: AppHandle,
    repo: String,
    _target_dir: Option<String>,
) -> Result<PluginEntry, String> {
    if !is_valid_repo(&repo) {
        return Err("仓库地址格式应为 https://github.com/owner/name".to_string());
    }
    // 仓库尚未部署：明确提示，同时保留框架入口
    Err(format!(
        "GitHub 插件仓库尚未部署，无法安装：{}。可先使用本地目录安装。",
        repo
    ))
}

fn is_valid_repo(repo: &str) -> bool {
    let prefix = "https://github.com/";
    repo.starts_with(prefix)
        && repo[prefix.len()..].split('/').count() == 2
        && !repo[prefix.len()..].contains(char::is_whitespace)
}

pub(crate) fn is_valid_repo_setting(repo: &str) -> bool {
    is_valid_repo(repo.trim_end_matches('/'))
}

/// 卸载插件（内置插件不可卸载）
#[tauri::command]
pub async fn uninstall_plugin(app: AppHandle, id: String) -> Result<(), String> {
    validate_id(&id)?;
    let manifest = get_plugin_manifest(app.clone(), id.clone()).await?;
    if manifest.builtin {
        return Err("内置插件不可卸载，仅可禁用非必需项".to_string());
    }
    let dir = plugins_root(&app).join(&id);
    fs::remove_dir_all(&dir).map_err(|e| format!("卸载插件失败: {}", e))
}

/// 已安装插件版本信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRelease {
    pub tag: String,
    pub body: String,
    pub published_at: String,
    pub html_url: String,
    #[serde(default)]
    pub assets: Vec<String>,
}

/// 检查插件仓库 release（GitHub API，仓库未部署时返回明确错误）
#[tauri::command]
pub async fn check_plugin_repo_updates(
    app: AppHandle,
    repo: String,
) -> Result<Option<PluginRelease>, String> {
    if !is_valid_repo(&repo) {
        return Err("仓库地址格式应为 https://github.com/owner/name".to_string());
    }
    let api = github_api_releases(&repo)?;
    let client = &app.state::<AppState>().http_client;
    let resp = client
        .get(&api)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "MSCL-Plugin-Hub")
        .send()
        .await
        .map_err(|e| format!("请求 GitHub 失败: {}", e))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("仓库不存在或未部署，无法检查更新".to_string());
    }
    if !resp.status().is_success() {
        return Err(format!("GitHub 返回错误: {}", resp.status()));
    }
    let value: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let arr = value.as_array().ok_or_else(|| "GitHub 返回格式异常".to_string())?;
    let first = match arr.first() {
        Some(v) => v,
        None => return Ok(None),
    };
    let assets = first
        .get("assets")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|a| a.get("browser_download_url").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    Ok(Some(PluginRelease {
        tag: first.get("tag_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        body: first.get("body").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        published_at: first.get("published_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        html_url: first.get("html_url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        assets,
    }))
}

pub(crate) fn repo_slug(repo: &str) -> Result<String, String> {
    let prefix = "https://github.com/";
    let slug = repo.strip_prefix(prefix).ok_or_else(|| "非法仓库地址".to_string())?;
    if !is_valid_repo(repo) {
        return Err("仓库地址格式应为 https://github.com/owner/name".to_string());
    }
    Ok(slug.trim_end_matches('/').to_string())
}

fn github_api_releases(repo: &str) -> Result<String, String> {
    let prefix = "https://github.com/";
    let slug = repo.strip_prefix(prefix).ok_or_else(|| "非法仓库地址".to_string())?;
    Ok(format!("https://api.github.com/repos/{}/releases", slug))
}

/// 安全更新插件库：先下载并校验 release 压缩包，再备份插件目录，
/// 最后由独立助手在启动器关闭后解压替换；任何失败都会回滚旧目录。
#[tauri::command]
pub async fn update_plugin_repo_safe(
    app: AppHandle,
    repo: String,
    version: String,
) -> Result<String, String> {
    let root = plugins_root(&app);
    let release = check_plugin_repo_updates(app.clone(), repo.clone())
        .await?
        .ok_or_else(|| "仓库没有可用版本".to_string())?;
    let slug = repo_slug(&repo)?;
    let asset = release
        .assets
        .iter()
        .find(|url| url.ends_with(".zip"))
        .cloned()
        .unwrap_or_else(|| {
            format!("https://github.com/{}/archive/refs/tags/{}.zip", slug, release.tag)
        });

    let game_root = app.state::<AppState>().game_root.clone();
    let updates = game_root.join("updates");
    fs::create_dir_all(&updates).map_err(|e| format!("创建下载目录失败: {}", e))?;
    let archive = updates.join(format!("plugins-{}.zip", safe_asset_name(&release.tag)));
    let http = app.state::<AppState>().http_client.clone();
    let mut request = http.get(&asset).header("User-Agent", "MSCL-Plugin-Hub");
    let settings = crate::models::AppSettings::load_or_default(&game_root.join("settings.json"));
    if !settings.plugin_repo_token.trim().is_empty() {
        request = request.bearer_auth(settings.plugin_repo_token.trim());
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("下载插件库失败: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("下载插件库失败: HTTP {}", response.status()));
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() < 100 || &bytes[..2] != b"PK" {
        return Err("下载内容不是有效的 zip 压缩包".to_string());
    }
    fs::write(&archive, &bytes).map_err(|e| format!("保存插件库失败: {}", e))?;

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup = game_root.join(format!("plugins.backup-{}", stamp));
    if root.exists() {
        copy_dir_recursive(&root, &backup)?;
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--apply-plugin-update")
        .arg(&archive)
        .arg(&backup)
        .arg(&root);
    spawn_detached(&mut cmd)?;
    Ok(format!("已备份并启动更新助手（版本 {}）", version))
}

pub(crate) fn spawn_detached(cmd: &mut std::process::Command) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(DETACHED_PROCESS);
    }
    cmd.spawn().map_err(|e| format!("启动更新助手失败: {}", e))?;
    Ok(())
}

pub(crate) fn safe_asset_name(tag: &str) -> String {
    tag.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

/// 独立更新助手：解压新插件库 → 原子替换 → 失败回滚。
pub fn apply_plugin_update(archive: &Path, backup: &Path, root: &Path) -> Result<(), String> {
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let parent = root
        .parent()
        .ok_or_else(|| "无法确定插件根目录".to_string())?;
    let stage = parent.join(format!("plugins.stage-{}", safe_asset_name(&format!("{:?}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()))));
    let old = parent.join(format!("plugins.old-{}", stage.file_name().unwrap_or_default().to_string_lossy()));
    let result = (|| -> Result<(), String> {
        let file = std::fs::File::open(archive).map_err(|e| format!("打开插件包失败: {}", e))?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("解压插件包失败: {}", e))?;
        zip.extract(&stage).map_err(|e| format!("解压插件包失败: {}", e))?;
        drop(zip);

        let mut sources = Vec::new();
        collect_plugin_dirs(&stage, &stage, &mut sources, 0)?;
        if sources.is_empty() {
            return Err("插件包中没有找到 plugin.json".to_string());
        }
        for source in &sources {
            let manifest = read_manifest(source)
                .ok_or_else(|| format!("插件包中的清单无效: {}", source.display()))?;
            validate_id(&manifest.id)?;
        }

        if root.exists() {
            fs::rename(root, &old).map_err(|e| format!("备份旧插件目录失败: {}", e))?;
        }
        let applied = (|| -> Result<(), String> {
            fs::create_dir_all(root).map_err(|e| e.to_string())?;
            for source in &sources {
                let target = root.join(
                    source
                        .file_name()
                        .ok_or_else(|| "插件目录名无效".to_string())?,
                );
                copy_dir_recursive(source, &target)?;
            }
            Ok(())
        })();
        if let Err(e) = applied {
            let _ = fs::remove_dir_all(root);
            if old.exists() {
                let _ = fs::rename(&old, root);
            }
            return Err(e);
        }
        let _ = fs::remove_dir_all(&old);
        Ok(())
    })();

    let _ = fs::remove_dir_all(&stage);
    match result {
        Ok(()) => {
            if backup.exists() {
                let _ = fs::remove_dir_all(backup);
            }
            let _ = fs::remove_file(archive);
            Ok(())
        }
        Err(e) => {
            if root.exists() {
                let _ = fs::remove_dir_all(root);
            }
            if backup.exists() {
                copy_dir_recursive(backup, root)?;
            }
            Err(format!("更新失败，已回滚到旧版本：{}", e))
        }
    }
}

fn collect_plugin_dirs(
    base: &Path,
    dir: &Path,
    out: &mut Vec<PathBuf>,
    depth: u32,
) -> Result<(), String> {
    if depth > 3 {
        return Ok(());
    }
    if dir.join("plugin.json").is_file() {
        out.push(dir.to_path_buf());
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        if entry.path().is_dir() {
            collect_plugin_dirs(base, &entry.path(), out, depth + 1)?;
        }
    }
    let _ = base;
    Ok(())
}

pub(crate) fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else {
            fs::copy(&path, &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 读取默认插件配置（安装器勾选结果）
#[tauri::command]
pub async fn get_default_plugin_config(app: AppHandle) -> Result<Vec<String>, String> {
    let path = plugins_root(&app).join("config.json");
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return Ok(Vec::new()),
    };
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(value
        .get("enabled")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default())
}

/// 写入默认插件配置（安装器在勾选默认插件后调用）
#[tauri::command]
pub async fn write_default_plugin_config(
    app: AppHandle,
    enabled_ids: Vec<String>,
) -> Result<(), String> {
    let root = plugins_root(&app);
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let config = serde_json::json!({
        "schema": 1,
        "enabled": enabled_ids,
    });
    fs::write(
        root.join("config.json"),
        serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

// ---------- 文件级导入导出辅助（供前端对话框使用）----------

/// 读取本地目录中未安装插件的 manifest（用于安装前校验）
#[tauri::command]
pub async fn read_plugin_manifest_from_dir(dir: String) -> Result<serde_json::Value, String> {
    let path = PathBuf::from(&dir);
    if !path.is_dir() { return Err("插件目录不存在".to_string()); }
    let text = fs::read_to_string(path.join("plugin.json"))
        .map_err(|e| format!("读取 plugin.json 失败: {}", e))?;
    if text.len() > 1024 * 1024 {
        return Err("plugin.json 过大".to_string());
    }
    serde_json::from_str(&text).map_err(|e| format!("解析 plugin.json 失败: {}", e))
}

/// 把导出的插件配置写入指定文件
#[tauri::command]
pub async fn write_plugin_export_file(
    app: AppHandle,
    path: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    let target = resolve_user_file(&app, &path)?;
    fs::write(&target, serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?)
        .map_err(|e| format!("写入导出文件失败: {}", e))
}

/// 从指定文件导入插件配置
#[tauri::command]
pub async fn import_plugin_configs_from_file(
    app: AppHandle,
    path: String,
) -> Result<usize, String> {
    let target = resolve_user_file(&app, &path)?;
    let text = fs::read_to_string(&target).map_err(|e| format!("读取导入文件失败: {}", e))?;
    if text.len() > 4 * 1024 * 1024 { return Err("插件配置文件过大".to_string()); }
    let payload: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("插件配置格式错误: {}", e))?;
    import_plugin_configs(app, payload).await
}

// ---------- 插件 config 编辑 / 导入导出 / 更新 ----------

/// 读取单个插件的 config.json（不存在时返回 {}）
#[tauri::command]
pub async fn get_plugin_config(app: AppHandle, id: String) -> Result<serde_json::Value, String> {
    validate_id(&id)?;
    let path = plugins_root(&app).join(&id).join("config.json");
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("插件配置格式错误: {}", e)),
        Err(_) => Ok(serde_json::json!({})),
    }
}

/// 保存单个插件的 config.json（按 configSchema 做基础类型校验）
#[tauri::command]
pub async fn set_plugin_config(
    app: AppHandle,
    id: String,
    config: serde_json::Value,
) -> Result<(), String> {
    validate_id(&id)?;
    let dir = plugins_root(&app).join(&id);
    if !dir.is_dir() {
        return Err("插件未安装".to_string());
    }
    let obj = config.as_object().ok_or_else(|| "配置必须是 JSON 对象".to_string())?;
    // 读取 schema 做键与类型校验（若有）
    let manifest_path = dir.join("plugin.json");
    if let Ok(text) = fs::read_to_string(&manifest_path) {
        if let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&text) {
            let schema = manifest
                .get("configSchema")
                .or_else(|| manifest.get("config_schema"))
                .and_then(|v| v.as_array());
            if let Some(schema) = schema {
                for field in schema {
                    let key = match field.get("key").and_then(|v| v.as_str()) { Some(k) => k, None => continue };
                    let ty = field.get("type").and_then(|v| v.as_str()).unwrap_or("string");
                    if let Some(val) = obj.get(key) {
                        let ok = match ty {
                            "boolean" => val.is_boolean(),
                            "number" => val.is_number(),
                            "string" | "select" => val.is_string(),
                            "string[]" => val.is_array(),
                            _ => true,
                        };
                        if !ok {
                            return Err(format!("字段 {} 类型应为 {}", key, ty));
                        }
                    }
                }
            }
        }
    }
    fs::write(
        dir.join("config.json"),
        serde_json::to_string_pretty(obj).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("保存插件配置失败: {}", e))
}

/// 导出所有插件配置（含清单与 config，不含任何凭据）
#[tauri::command]
pub async fn export_plugin_configs(app: AppHandle) -> Result<serde_json::Value, String> {
    let root = plugins_root(&app);
    let mut items = Vec::new();
    if root.exists() {
        for entry in fs::read_dir(&root).map_err(|e| e.to_string())?.flatten() {
            if !entry.path().is_dir() { continue; }
            let manifest = match read_manifest(&entry.path()) { Some(m) => m, None => continue };
            let config = fs::read_to_string(entry.path().join("config.json"))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_else(|| serde_json::json!({}));
            items.push(serde_json::json!({
                "id": manifest.id,
                "enabled": manifest.enabled,
                "config": config,
            }));
        }
    }
    Ok(serde_json::json!({ "schema": 1, "plugins": items }))
}

/// 导入插件配置（仅覆盖已安装插件的 enabled 与 config，不安装新插件）
#[tauri::command]
pub async fn import_plugin_configs(
    app: AppHandle,
    payload: serde_json::Value,
) -> Result<usize, String> {
    let items = payload
        .get("plugins")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "导入格式错误：缺少 plugins 数组".to_string())?;
    let mut count = 0usize;
    for item in items {
        let id = match item.get("id").and_then(|v| v.as_str()) { Some(i) => i, None => continue };
        validate_id(id)?;
        let dir = plugins_root(&app).join(id);
        if !dir.is_dir() { continue; }
        // 更新 enabled
        if let Some(enabled) = item.get("enabled").and_then(|v| v.as_bool()) {
            let mp = dir.join("plugin.json");
            if let Ok(text) = fs::read_to_string(&mp) {
                if let Ok(mut manifest) = serde_json::from_str::<serde_json::Value>(&text) {
                    if manifest.get("required").and_then(|v| v.as_bool()) != Some(true) {
                        manifest["enabled"] = serde_json::Value::Bool(enabled);
                        let _ = fs::write(&mp, serde_json::to_string_pretty(&manifest).unwrap_or_default());
                    }
                }
            }
        }
        if let Some(config) = item.get("config") {
            let _ = fs::write(
                dir.join("config.json"),
                serde_json::to_string_pretty(config).unwrap_or_default(),
            );
        }
        count += 1;
    }
    Ok(count)
}

/// 检查单个插件是否有更新（基于其 repo 的 release；未配置 repo 返回 None）
#[tauri::command]
pub async fn check_plugin_update(app: AppHandle, id: String) -> Result<Option<PluginRelease>, String> {
    let manifest = get_plugin_manifest(app.clone(), id).await?;
    match manifest.repo {
        Some(repo) if !repo.is_empty() => check_plugin_repo_updates(app, repo).await,
        _ => Ok(None),
    }
}

/// 校验 manifest 合法性（DSL v1 严格模式）：返回错误列表
#[tauri::command]
pub async fn validate_plugin_manifest(manifest: serde_json::Value) -> Result<Vec<String>, String> {
    let get = |key: &str| {
        manifest
            .get(key)
            .or_else(|| manifest.get(to_snake(key)))
    };
    let mut errors = Vec::new();
    let id = get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if let Err(e) = validate_id(&id) {
        errors.push(e);
    }
    if get("name").and_then(|v| v.as_str()).unwrap_or("").trim().is_empty() {
        errors.push("插件缺少 name".to_string());
    }
    let schema = get("schema").and_then(|v| v.as_u64()).unwrap_or(1);
    if schema > 1 {
        errors.push("manifest schema 版本不受支持".to_string());
    }
    let category = get("category").and_then(|v| v.as_str()).unwrap_or("");
    if !["core", "launcher", "server"].contains(&category) {
        errors.push("category 应为 core / launcher / server".to_string());
    }
    let layer = get("layer").and_then(|v| v.as_str()).unwrap_or("feature");
    if !["harness", "module", "feature"].contains(&layer) {
        errors.push("layer 应为 harness / module / feature".to_string());
    }
    let allowed_caps = [
        "page", "settings", "resourceCategory", "theme", "command", "launch",
        "download", "storage", "event", "i18n", "config",
    ];
    let capabilities = get("capabilities").and_then(|v| v.as_array().cloned()).unwrap_or_default();
    for value in &capabilities {
        if let Some(cap) = value.as_str() {
            if !allowed_caps.contains(&cap) {
                errors.push(format!("未知能力: {}", cap));
            }
        }
    }
    let allowed_hooks = [
        "onLoad", "onEnable", "onDisable", "onBeforeLaunch", "onLaunch", "onAfterLaunch",
        "onBeforeResourceInstall", "onResourceInstalled", "onUpdateCheck", "onSettingsSave",
    ];
    let hooks = get("hooks").and_then(|v| v.as_array().cloned()).unwrap_or_default();
    for value in &hooks {
        if let Some(hook) = value.as_str() {
            if !allowed_hooks.contains(&hook) {
                errors.push(format!("未知生命周期钩子: {}", hook));
            }
        }
    }
    let allowed_points = [
        "page", "settings", "resource.category", "launch.step", "download.source", "theme", "command",
    ];
    let points = get("extensionPoints").and_then(|v| v.as_array().cloned()).unwrap_or_default();
    for value in &points {
        let point = value.get("point").and_then(|v| v.as_str()).unwrap_or("");
        if !allowed_points.contains(&point) {
            errors.push(format!("未知扩展点: {}", point));
        }
        if value.get("key").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
            errors.push("扩展点缺少 key".to_string());
        }
    }
    let dependencies = get("dependencies").and_then(|v| v.as_array().cloned()).unwrap_or_default();
    for value in &dependencies {
        if let Some(dep) = value.as_str() {
            if let Err(e) = validate_id(dep) {
                errors.push(format!("依赖 ID 非法: {}", e));
            }
        }
    }
    let allowed_types = ["string", "number", "boolean", "select", "string[]"];
    let fields = get("configSchema").and_then(|v| v.as_array().cloned()).unwrap_or_default();
    for value in &fields {
        let key = value.get("key").and_then(|v| v.as_str()).unwrap_or("");
        if key.is_empty() {
            errors.push("configSchema 字段缺少 key".to_string());
        }
        let ty = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if !allowed_types.contains(&ty) {
            errors.push(format!("字段 {} 的类型不受支持: {}", key, ty));
        }
        if value.get("default").is_none() {
            errors.push(format!("字段 {} 缺少默认值", key));
        }
    }
    Ok(errors)
}

fn to_snake(key: &str) -> &str {
    match key {
        "extensionPoints" => "extension_points",
        "configSchema" => "config_schema",
        _ => key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_plugin_id() {
        assert!(validate_id("plugin-hub").is_ok());
        assert!(validate_id("Bad_ID").is_err());
        assert!(validate_id("").is_err());
    }

    #[test]
    fn validate_repo() {
        assert!(is_valid_repo("https://github.com/mscl-team/mscl-plugins"));
        assert!(!is_valid_repo("https://example.com/a/b"));
        assert!(!is_valid_repo("https://github.com/only-owner"));
    }

    #[test]
    fn build_api_url() {
        assert_eq!(
            github_api_releases("https://github.com/mscl-team/mscl-plugins").unwrap(),
            "https://api.github.com/repos/mscl-team/mscl-plugins/releases"
        );
    }
}
