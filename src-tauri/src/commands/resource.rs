//! 资源中心（阶段 13–15）
//!
//! 支持多来源：
//!   - Modrinth（及其兼容镜像）
//!   - CurseForge（需要 API Key）
//! 来源在设置里自定义（id / 名称 / 类型 / API 地址 / Key），首个为默认来源。
//!
//! 资源类型：mod / shader / resourcepack / datapack / modpack / plugin（服务端插件）

use tauri::{AppHandle, Emitter, Manager};

use super::instance::emit_log;
use super::modpack::import_mrpack;
use crate::models::{
    DetectedResource, ImportMrpackResult, ImportMrpackUrlRequest, InstallResourceRequest,
    InstallResult, ListResourceVersionsRequest, ResourceDetail, ResourceHit, ResourceSearchRequest,
    ResourceSearchResult, ResourceSource, ResourceVersionInfo,
};
use crate::state::AppState;

fn user_agent() -> String {
    format!(
        "mscl-launcher/{} (Minecraft launcher; +https://github.com/mscl)",
        env!("CARGO_PKG_VERSION")
    )
}

fn subdir_for(kind: &str) -> &'static str {
    match kind {
        "shader" => "shaderpacks",
        "resourcepack" => "resourcepacks",
        "datapack" => "datapacks",
        "mod" | "plugin" => "mods",
        _ => "mods",
    }
}

fn subdir_for_target(kind: &str, target_kind: Option<&str>) -> &'static str {
    if kind == "plugin" && matches!(target_kind, Some("server") | Some("multiplayer")) {
        return "plugins";
    }
    subdir_for(kind)
}

/// 实例已安装资源清单（用于重复/依赖判断）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct ManifestEntry {
    project_id: String,
    #[serde(default)]
    file_name: String,
    #[serde(default)]
    kind: String,
}

fn manifest_path(game_root: &std::path::Path, version_id: &str) -> std::path::PathBuf {
    game_root
        .join("instances")
        .join(version_id)
        .join(".mscl-resources.json")
}

fn read_manifest(game_root: &std::path::Path, version_id: &str) -> Vec<ManifestEntry> {
    std::fs::read_to_string(manifest_path(game_root, version_id))
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<ManifestEntry>>(&text).ok())
        .unwrap_or_default()
}

fn write_manifest(
    game_root: &std::path::Path,
    version_id: &str,
    entries: &[ManifestEntry],
) -> Result<(), String> {
    let path = manifest_path(game_root, version_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(entries).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("写入资源清单失败: {}", e))
}

/// 解析使用哪个资源来源
fn resolve_source(app: &AppHandle, source_id: Option<&str>) -> ResourceSource {
    let root = app.state::<AppState>().game_root.clone();
    let settings = crate::models::AppSettings::load_or_default(&root.join("settings.json"));
    settings
        .resource_sources
        .iter()
        .find(|s| Some(s.id.as_str()) == source_id)
        .or_else(|| settings.resource_sources.first())
        .cloned()
        .unwrap_or_else(ResourceSource::modrinth_official)
}

fn resolve_source_for_type(
    app: &AppHandle,
    source_id: Option<&str>,
    project_type: &str,
) -> Result<ResourceSource, String> {
    let root = app.state::<AppState>().game_root.clone();
    let settings = crate::models::AppSettings::load_or_default(&root.join("settings.json"));
    let source = source_id
        .and_then(|id| settings.resource_sources.iter().find(|item| item.id == id))
        .or_else(|| {
            settings
                .resource_sources
                .iter()
                .find(|item| item.supports(project_type))
        })
        .cloned()
        .unwrap_or_else(ResourceSource::modrinth_official);
    if !source.supports(project_type) {
        return Err(format!(
            "来源「{}」没有启用「{}」分类，请在 设置 → 资源来源 中勾选对应分类",
            source.name, project_type
        ));
    }
    Ok(source)
}

fn source_base(source: &ResourceSource) -> String {
    let base = source.api_base.trim().trim_end_matches('/');
    if base.is_empty() {
        match source.kind.as_str() {
            "curseforge" => "https://api.curseforge.com".to_string(),
            "minebbs" => "https://api.minebbs.com/api/openapi/v1".to_string(),
            "mcmod" => "https://www.mcmod.cn".to_string(),
            "spigot" => "https://api.spiget.org/v2".to_string(),
            _ => "https://api.modrinth.com/v2".to_string(),
        }
    } else {
        base.to_string()
    }
}

/// 从用户输入中解析 MineBBS 资源 ID（支持纯数字、资源链接）
fn parse_minebbs_id(query: &str) -> Option<u64> {
    let trimmed = query.trim();
    if let Ok(id) = trimmed.parse::<u64>() {
        return Some(id);
    }
    let after = trimmed.split("/resources/").nth(1)?;
    let token = after.split('/').next()?;
    // 形如 xxx.123 时取点号后的数字
    let id_part = token.rsplit('.').next().unwrap_or(token);
    id_part.parse::<u64>().ok()
}

/// MineBBS 开放接口没有搜索，只能按 ID 查询单个资源
async fn search_minebbs(
    client: &reqwest::Client,
    source: &ResourceSource,
    req: &ResourceSearchRequest,
) -> Result<(Vec<ResourceHit>, u64), String> {
    let id = parse_minebbs_id(&req.query).ok_or_else(|| {
        "MineBBS 开放接口不支持关键词搜索：请填写资源 ID 或资源链接（例如 https://www.minebbs.com/resources/xxx.123/）".to_string()
    })?;

    let base = source_base(source);
    let mut request = client
        .get(format!("{}/resources/{}", base, id))
        .header("User-Agent", user_agent());
    if !source.api_key.trim().is_empty() {
        request = request.header("Authorization", source.api_key.trim());
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("查询 MineBBS 资源失败: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("MineBBS 返回状态 {}", response.status()));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("解析 MineBBS 响应失败: {}", e))?;
    let data = json
        .get("data")
        .ok_or_else(|| "MineBBS 未返回资源数据（资源可能不存在）".to_string())?;

    Ok((
        vec![ResourceHit {
            project_id: id.to_string(),
            slug: data
                .get("view_url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            title: data
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            description: data
                .get("tag_line")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            icon_url: data
                .get("icon_url")
                .and_then(|v| v.as_str())
                .map(String::from),
            downloads: data
                .get("download_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            project_type: req.project_type.clone(),
        }],
        1,
    ))
}

async fn search_spigot(
    client: &reqwest::Client,
    source: &ResourceSource,
    req: &ResourceSearchRequest,
) -> Result<(Vec<ResourceHit>, u64), String> {
    if req.project_type != "plugin" {
        return Err("Spigot 来源只支持服务端插件".to_string());
    }
    let base = source_base(source);
    let limit = req.limit.unwrap_or(20).min(50);
    let offset = req.offset.unwrap_or(0);
    let page = offset / limit.max(1) + 1;
    let response = client
        .get(format!("{}/search/resources/{}", base, req.query))
        .header("User-Agent", user_agent())
        .query(&[
            ("field", "name".to_string()),
            ("size", limit.to_string()),
            ("page", page.to_string()),
        ])
        .send()
        .await
        .map_err(|e| format!("搜索 Spigot 失败: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("Spigot 返回状态 {}", response.status()));
    }
    let values: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("解析 Spigot 搜索结果失败: {}", e))?;
    let array = values.as_array().cloned().unwrap_or_default();
    let hits: Vec<ResourceHit> = array
        .iter()
        .map(|item| {
            let id = item.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            let icon = item
                .get("icon")
                .and_then(|value| value.get("url"))
                .and_then(|value| value.as_str())
                .unwrap_or("");
            ResourceHit {
                project_id: id.to_string(),
                slug: id.to_string(),
                title: item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                description: item
                    .get("tag")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                icon_url: if icon.is_empty() {
                    None
                } else if icon.starts_with("http") {
                    Some(icon.to_string())
                } else {
                    Some(format!("https://www.spigotmc.org/{}", icon.trim_start_matches('/')))
                },
                downloads: item.get("downloads").and_then(|v| v.as_u64()).unwrap_or(0),
                project_type: "plugin".to_string(),
            }
        })
        .collect();
    let total = if hits.len() < limit as usize {
        offset as u64 + hits.len() as u64
    } else {
        offset as u64 + limit as u64 + 1
    };
    Ok((hits, total))
}

/// CurseForge 分类 ID
fn curseforge_class_id(project_type: &str) -> u32 {    match project_type {
        "mod" => 6,
        "plugin" => 5,
        "modpack" => 4471,
        "resourcepack" => 12,
        "datapack" => 6945,
        "shader" => 6552,
        "skin" => 4546,
        _ => 6,
    }
}

fn need_key(source: &ResourceSource) -> Result<(), String> {
    if source.kind == "curseforge" && source.api_key.trim().is_empty() {
        return Err(
            "该来源需要 API Key：请到「设置 → 资源来源」为这一来源填写 Key".to_string(),
        );
    }
    Ok(())
}

/// MC百科没有公开直链下载接口：返回站内搜索入口，点击后在浏览器打开。
fn search_mcmod(
    source: &ResourceSource,
    req: &ResourceSearchRequest,
) -> Result<(Vec<ResourceHit>, u64), String> {
    let base = source_base(source);
    let url = reqwest::Url::parse_with_params(
        &format!("{}/s", base.trim_end_matches('/')),
        &[("key", req.query.as_str())],
    )
    .map_err(|e| format!("生成 MC百科 搜索链接失败: {}", e))?
    .to_string();
    Ok((
        vec![ResourceHit {
            project_id: "mcmod-search".to_string(),
            slug: url,
            title: format!("在 MC百科 中搜索「{}」", req.query),
            description: "MC百科不提供直链下载，打开网页后可手动下载资源并拖入启动器。".to_string(),
            icon_url: None,
            downloads: 0,
            project_type: req.project_type.clone(),
        }],
        1,
    ))
}

/// 搜索资源
#[tauri::command]
pub async fn search_resources(
    app: AppHandle,
    req: ResourceSearchRequest,
) -> Result<ResourceSearchResult, String> {
    let source = resolve_source_for_type(&app, req.source_id.as_deref(), &req.project_type)?;
    let client = app.state::<AppState>().http_client.clone();

    let (hits, total) = match source.kind.as_str() {
        "curseforge" => search_curseforge(&client, &source, &req).await,
        "minebbs" => search_minebbs(&client, &source, &req).await,
        "mcmod" => search_mcmod(&source, &req),
        "spigot" => search_spigot(&client, &source, &req).await,
        _ => search_modrinth(&client, &source, &req).await,
    }?;
    Ok(ResourceSearchResult { hits, total })
}

async fn search_modrinth(
    client: &reqwest::Client,
    source: &ResourceSource,
    req: &ResourceSearchRequest,
) -> Result<(Vec<ResourceHit>, u64), String> {
    let base = source_base(source);
    let mut facets: Vec<String> = vec![format!("[\"project_type:{}\"]", req.project_type)];
    if let Some(version) = req.game_version.as_deref().filter(|v| !v.is_empty()) {
        facets.push(format!("[\"versions:{}\"]", version));
    }
    if let Some(loader) = req.loader.as_deref().filter(|v| !v.is_empty()) {
        facets.push(format!("[\"categories:{}\"]", loader));
    }
    if let Some(category) = req.category.as_deref().filter(|v| !v.is_empty()) {
        facets.push(format!("[\"categories:{}\"]", category));
    }

    let limit = req.limit.unwrap_or(20).min(50).to_string();
    let offset = req.offset.unwrap_or(0).to_string();
    let mut request = client
        .get(format!("{}/search", base))
        .header("User-Agent", user_agent())
        .query(&[
            ("query", req.query.as_str()),
            ("limit", limit.as_str()),
            ("index", "relevance"),
            ("offset", offset.as_str()),
            ("facets", &format!("[{}]", facets.join(","))),
        ]);
    if !source.api_key.trim().is_empty() {
        request = request.header("Authorization", source.api_key.trim());
    }

    let response = request.send().await.map_err(|e| format!("搜索失败: {}", e))?;
    if !response.status().is_success() {
        return Err(format!(
            "资源站返回状态 {}（可在设置里检查来源地址与 Key）",
            response.status()
        ));
    }
    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("解析搜索结果失败: {}", e))?;

    let total = json.get("total_hits").and_then(|v| v.as_u64()).unwrap_or(0);
    let hits = json
        .get("hits")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|h| ResourceHit {
            project_id: h.get("project_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            slug: h.get("slug").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            title: h.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            description: h.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            icon_url: h.get("icon_url").and_then(|v| v.as_str()).map(String::from),
            downloads: h.get("downloads").and_then(|v| v.as_u64()).unwrap_or(0),
            project_type: h.get("project_type").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        })
        .collect();
    Ok((hits, total))
}

async fn search_curseforge(
    client: &reqwest::Client,
    source: &ResourceSource,
    req: &ResourceSearchRequest,
) -> Result<(Vec<ResourceHit>, u64), String> {
    need_key(source)?;
    let base = source_base(source);
    let class_id = curseforge_class_id(&req.project_type);

    let response = client
        .get(format!("{}/v1/mods/search", base))
        .header("x-api-key", source.api_key.trim())
        .header("Accept", "application/json")
        .query(&[
            ("gameId", "432".to_string()),
            ("classId", class_id.to_string()),
            ("searchFilter", req.query.clone()),
            ("pageSize", req.limit.unwrap_or(20).to_string()),
            ("index", req.offset.unwrap_or(0).to_string()),
            ("sortField", "2".to_string()),
            ("sortOrder", "desc".to_string()),
        ])
        .send()
        .await
        .map_err(|e| format!("搜索失败: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "CurseForge 返回状态 {}（请检查 API Key 是否正确）",
            response.status()
        ));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("解析搜索结果失败: {}", e))?;

    let total = json
        .get("pagination")
        .and_then(|p| p.get("totalCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let hits = json
        .get("data")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|m| ResourceHit {
            project_id: m
                .get("id")
                .and_then(|v| v.as_u64())
                .map(|n| n.to_string())
                .unwrap_or_default(),
            slug: m.get("slug").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            title: m.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            description: m.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            icon_url: m
                .get("logo")
                .and_then(|l| l.get("thumbnailUrl"))
                .and_then(|v| v.as_str())
                .map(String::from),
            downloads: m.get("downloadCount").and_then(|v| v.as_u64()).unwrap_or(0),
            project_type: req.project_type.clone(),
        })
        .collect();
    Ok((hits, total))
}

/// 列出某个资源的所有可用版本
#[tauri::command]
pub async fn list_resource_versions(
    app: AppHandle,
    req: ListResourceVersionsRequest,
) -> Result<Vec<ResourceVersionInfo>, String> {
    let source = resolve_source_for_type(&app, req.source_id.as_deref(), &req.project_type)?;
    let client = app.state::<AppState>().http_client.clone();
    let base = source_base(&source);

    if source.kind == "spigot" {
        let values: serde_json::Value = client
            .get(format!("{}/resources/{}/versions", base, req.project_id))
            .header("User-Agent", user_agent())
            .query(&[("size", "40".to_string())])
            .send()
            .await
            .map_err(|e| format!("获取 Spigot 版本列表失败: {}", e))?
            .json()
            .await
            .map_err(|e| format!("解析 Spigot 版本列表失败: {}", e))?;
        return Ok(values
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|item| {
                let id = item.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                let name = item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| id.to_string());
                ResourceVersionInfo {
                    id: id.to_string(),
                    name: name.clone(),
                    version_number: name,
                    game_versions: item
                        .get("testedVersions")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|x| x.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    loaders: vec!["spigot".to_string()],
                    downloads: 0,
                    file_name: format!("spigot-{}.jar", req.project_id),
                    published: item
                        .get("releaseDate")
                        .and_then(|v| v.as_u64())
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                }
            })
            .collect());
    }

    if source.kind == "curseforge" {
        need_key(&source)?;
        let json: serde_json::Value = client
            .get(format!("{}/v1/mods/{}/files", base, req.project_id))
            .header("x-api-key", source.api_key.trim())
            .query(&[("pageSize", "40".to_string())])
            .send()
            .await
            .map_err(|e| format!("获取版本列表失败: {}", e))?
            .json()
            .await
            .map_err(|e| format!("解析版本列表失败: {}", e))?;

        return Ok(json
            .get("data")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|f| ResourceVersionInfo {
                id: f.get("id").and_then(|v| v.as_u64()).map(|n| n.to_string()).unwrap_or_default(),
                name: f.get("displayName").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                version_number: f
                    .get("displayName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                game_versions: f
                    .get("gameVersions")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
                loaders: Vec::new(),
                downloads: f.get("downloadCount").and_then(|v| v.as_u64()).unwrap_or(0),
                file_name: f.get("fileName").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                published: f
                    .get("fileDate")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
            .collect());
    }

    let mut request = client
        .get(format!("{}/project/{}/version", base, req.project_id))
        .header("User-Agent", user_agent());
    if !source.api_key.trim().is_empty() {
        request = request.header("Authorization", source.api_key.trim());
    }
    let json: serde_json::Value = request
        .send()
        .await
        .map_err(|e| format!("获取版本列表失败: {}", e))?
        .json()
        .await
        .map_err(|e| format!("解析版本列表失败: {}", e))?;

    Ok(json
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|v| {
            let file = v
                .get("files")
                .and_then(|f| f.as_array())
                .and_then(|arr| {
                    arr.iter()
                        .find(|f| f.get("primary").and_then(|p| p.as_bool()) == Some(true))
                        .or_else(|| arr.first())
                });
            ResourceVersionInfo {
                id: v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                name: v.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                version_number: v
                    .get("version_number")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                game_versions: v
                    .get("game_versions")
                    .and_then(|x| x.as_array())
                    .map(|arr| arr.iter().filter_map(|y| y.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
                loaders: v
                    .get("loaders")
                    .and_then(|x| x.as_array())
                    .map(|arr| arr.iter().filter_map(|y| y.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
                downloads: v.get("downloads").and_then(|x| x.as_u64()).unwrap_or(0),
                file_name: file
                    .and_then(|f| f.get("filename"))
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                published: v
                    .get("date_published")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
            }
        })
        .collect())
}

/// 解析出下载地址与文件名
async fn resolve_download(
    client: &reqwest::Client,
    source: &ResourceSource,
    project_id: &str,
    version_id: Option<&str>,
) -> Result<(String, String), String> {
    let base = source_base(source);

    if source.kind == "spigot" {
        let version = version_id.filter(|value| !value.is_empty());
        let url = match version {
            Some(id) => format!("{}/resources/{}/download?version={}", base, project_id, id),
            None => format!("{}/resources/{}/download", base, project_id),
        };
        return Ok((url, format!("spigot-{}.jar", project_id)));
    }

    if source.kind == "curseforge" {
        need_key(source)?;
        let file_id: u64 = match version_id.filter(|v| !v.is_empty()) {
            Some(v) => v.parse().map_err(|_| "非法的文件 ID".to_string())?,
            None => {
                let json: serde_json::Value = client
                    .get(format!("{}/v1/mods/{}/files", base, project_id))
                    .header("x-api-key", source.api_key.trim())
                    .query(&[("pageSize", "1".to_string())])
                    .send()
                    .await
                    .map_err(|e| format!("获取文件列表失败: {}", e))?
                    .json()
                    .await
                    .map_err(|e| format!("解析文件列表失败: {}", e))?;
                json.get("data")
                    .and_then(|d| d.as_array())
                    .and_then(|a| a.first())
                    .and_then(|f| f.get("id"))
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| "该资源没有可用文件".to_string())?
            }
        };

        // 先查文件详情里的 downloadUrl，为空则请求下载地址接口
        let detail: serde_json::Value = client
            .get(format!("{}/v1/mods/{}/files/{}", base, project_id, file_id))
            .header("x-api-key", source.api_key.trim())
            .send()
            .await
            .map_err(|e| format!("获取文件详情失败: {}", e))?
            .json()
            .await
            .map_err(|e| format!("解析文件详情失败: {}", e))?;
        let data = detail.get("data").cloned().unwrap_or_default();
        let file_name = data
            .get("fileName")
            .and_then(|v| v.as_str())
            .unwrap_or("download.bin")
            .to_string();
        if let Some(url) = data.get("downloadUrl").and_then(|v| v.as_str()) {
            if !url.is_empty() {
                return Ok((url.to_string(), file_name));
            }
        }

        let url_json: serde_json::Value = client
            .get(format!(
                "{}/v1/mods/{}/files/{}/download-url",
                base, project_id, file_id
            ))
            .header("x-api-key", source.api_key.trim())
            .send()
            .await
            .map_err(|e| format!("获取下载地址失败: {}", e))?
            .json()
            .await
            .map_err(|e| format!("解析下载地址失败: {}", e))?;
        let url = url_json
            .get("data")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "该文件不提供直接下载（可能需要作者授权）".to_string())?;
        return Ok((url.to_string(), file_name));
    }

    // Modrinth 及兼容源
    let mut request = if let Some(vid) = version_id.filter(|v| !v.is_empty()) {
        client
            .get(format!("{}/version/{}", base, vid))
            .header("User-Agent", user_agent())
    } else {
        let arr: serde_json::Value = client
            .get(format!("{}/project/{}/version", base, project_id))
            .header("User-Agent", user_agent())
            .send()
            .await
            .map_err(|e| format!("获取版本列表失败: {}", e))?
            .json()
            .await
            .map_err(|e| format!("解析版本列表失败: {}", e))?;
        let first = arr
            .as_array()
            .and_then(|a| a.first())
            .cloned()
            .ok_or_else(|| "该资源没有可用文件".to_string())?;
        return primary_file(&first);
    };
    if !source.api_key.trim().is_empty() {
        request = request.header("Authorization", source.api_key.trim());
    }
    let version: serde_json::Value = request
        .send()
        .await
        .map_err(|e| format!("获取版本详情失败: {}", e))?
        .json()
        .await
        .map_err(|e| format!("解析版本详情失败: {}", e))?;
    primary_file(&version)
}

fn primary_file(version: &serde_json::Value) -> Result<(String, String), String> {
    let files = version
        .get("files")
        .and_then(|f| f.as_array())
        .ok_or_else(|| "该版本没有文件".to_string())?;
    let file = files
        .iter()
        .find(|f| f.get("primary").and_then(|p| p.as_bool()) == Some(true))
        .or_else(|| files.first())
        .ok_or_else(|| "该版本没有文件".to_string())?;
    let url = file
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "文件缺少下载地址".to_string())?;
    let filename = file
        .get("filename")
        .and_then(|v| v.as_str())
        .unwrap_or("download.bin");
    Ok((url.to_string(), filename.to_string()))
}

/// 批量取项目标题（依赖提示用，仅 Modrinth）
async fn fetch_titles(client: &reqwest::Client, base: &str, ids: &[String]) -> Vec<String> {
    if ids.is_empty() {
        return Vec::new();
    }
    let ids_json = serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_string());
    let result = client
        .get(format!("{}/projects", base))
        .header("User-Agent", user_agent())
        .query(&[("ids", ids_json.as_str())])
        .send()
        .await;
    match result {
        Ok(resp) => resp
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|json| json.as_array().cloned())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| p.get("title").and_then(|t| t.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| ids.to_vec()),
        Err(_) => ids.to_vec(),
    }
}

/// 安装前检查：是否已安装同项目 + 缺失的必需前置
#[tauri::command]
pub async fn check_resource_install(
    app: AppHandle,
    req: InstallResourceRequest,
) -> Result<crate::models::ResourceCheckResult, String> {
    let game_root = app.state::<AppState>().game_root.clone();
    let settings = crate::models::AppSettings::load_or_default(&game_root.join("settings.json"));

    // 实例级覆盖优先：instance.json 的 checkDependencies
    let instance_check = std::fs::read_to_string(
        game_root
            .join("instances")
            .join(&req.instance_version_id)
            .join("instance.json"),
    )
    .ok()
    .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    .and_then(|json| json.get("checkDependencies").and_then(|v| v.as_bool()));

    if !instance_check.unwrap_or(settings.check_dependencies) {
        return Ok(crate::models::ResourceCheckResult {
            already_installed: false,
            existing_file: None,
            missing_dependencies: Vec::new(),
        });
    }

    // 外部目录没有资源清单，跳过重复检查
    if req.instance_version_id.trim().is_empty() {
        return Ok(crate::models::ResourceCheckResult {
            already_installed: false,
            existing_file: None,
            missing_dependencies: Vec::new(),
        });
    }

    let manifest = read_manifest(&game_root, &req.instance_version_id);
    if let Some(existing) = manifest.iter().find(|e| e.project_id == req.project_id) {
        return Ok(crate::models::ResourceCheckResult {
            already_installed: true,
            existing_file: Some(existing.file_name.clone()),
            missing_dependencies: Vec::new(),
        });
    }

    let source = resolve_source(&app, req.source_id.as_deref());
    if source.kind != "modrinth" {
        // 非 Modrinth 来源暂不做依赖解析
        return Ok(crate::models::ResourceCheckResult {
            already_installed: false,
            existing_file: None,
            missing_dependencies: Vec::new(),
        });
    }

    let base = source_base(&source);
    let http = app.state::<AppState>().http_client.clone();

    // 取版本信息；失败时不阻塞安装（仅跳过依赖检查）
    let version_value: Option<serde_json::Value> =
        if let Some(vid) = req.version_id.as_deref().filter(|v| !v.is_empty()) {
            match http
                .get(format!("{}/version/{}", base, vid))
                .header("User-Agent", user_agent())
                .send()
                .await
            {
                Ok(resp) => resp.json::<serde_json::Value>().await.ok(),
                Err(_) => None,
            }
        } else {
            match http
                .get(format!("{}/project/{}/version", base, req.project_id))
                .header("User-Agent", user_agent())
                .send()
                .await
            {
                Ok(resp) => resp
                    .json::<serde_json::Value>()
                    .await
                    .ok()
                    .and_then(|v| v.as_array().and_then(|a| a.first()).cloned()),
                Err(_) => None,
            }
        };

    let required: Vec<String> = version_value
        .and_then(|v| v.get("dependencies").cloned())
        .and_then(|d| d.as_array().cloned())
        .map(|arr| {
            arr.iter()
                .filter(|d| d.get("dependency_type").and_then(|t| t.as_str()) == Some("required"))
                .filter_map(|d| d.get("project_id").and_then(|p| p.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let missing: Vec<String> = required
        .into_iter()
        .filter(|id| !manifest.iter().any(|e| &e.project_id == id))
        .collect();

    let titles = fetch_titles(&http, &base, &missing).await;

    Ok(crate::models::ResourceCheckResult {
        already_installed: false,
        existing_file: None,
        missing_dependencies: titles,
    })
}

/// 安装资源到实例
#[tauri::command]
pub async fn install_resource(
    app: AppHandle,
    req: InstallResourceRequest,
) -> Result<InstallResult, String> {
    let (http, game_root) = {
        let state = app.state::<AppState>();
        (state.http_client.clone(), state.game_root.clone())
    };
    let source = resolve_source_for_type(&app, req.source_id.as_deref(), &req.project_type)?;

    if source.kind == "minebbs" {
        return Err(
            "MineBBS 不提供直链下载：请在结果里点「打开网页」，登录后手动下载并拖入实例"
                .to_string(),
        );
    }

    emit_log(
        &app,
        "info",
        format!("获取资源版本信息（{} / {}）…", source.name, req.project_type),
    );

    let (url, filename) = resolve_download(
        &http,
        &source,
        &req.project_id,
        req.version_id.as_deref(),
    )
    .await?;

    // 整合包：下载后用 mrpack 流程导入
    if req.project_type == "modpack" {
        emit_log(&app, "info", format!("下载整合包 {}…", filename));
        let tmp = std::env::temp_dir().join(format!(
            "mscl-res-{}-{}",
            std::process::id(),
            filename.trim_end_matches('/')
        ));
        let bytes = http
            .get(&url)
            .header("User-Agent", user_agent())
            .send()
            .await
            .map_err(|e| format!("下载整合包失败: {}", e))?
            .bytes()
            .await
            .map_err(|e| format!("读取整合包失败: {}", e))?;
        std::fs::write(&tmp, &bytes).map_err(|e| format!("写入整合包失败: {}", e))?;

        let result = import_mrpack(
            app.clone(),
            tmp.to_string_lossy().into_owned(),
            req.target_dir.clone(),
        )
        .await?;
        let _ = std::fs::remove_file(&tmp);
        emit_log(
            &app,
            "success",
            format!("整合包「{}」导入完成", result.pack_name),
        );
        return Ok(InstallResult {
            version_id: result.target_version,
            downloaded: result.downloaded,
            skipped: result.skipped,
            failed: result.errors.len(),
            errors: result.errors,
            game_root: game_root.to_string_lossy().into_owned(),
        });
    }

    let subdir = subdir_for_target(&req.project_type, req.target_kind.as_deref());
    // 外部目录优先：用户可以把资源直接装到本地资源目录、服务器或多人游戏目录
    let dest_root = match super::plugin::resolve_install_root(&app, req.target_dir.as_deref())? {
        Some(dir) => dir,
        None => game_root.join("instances").join(&req.instance_version_id),
    };
    let filename = std::path::Path::new(&filename)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "资源文件名无效".to_string())?
        .to_string();
    if filename.is_empty() || filename.contains("..") {
        return Err("资源文件名无效".to_string());
    }
    let dest = dest_root.join(subdir).join(&filename);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    emit_log(&app, "info", format!("下载 {}…", filename));
    let _ = app.emit(
        "install-progress",
        serde_json::json!({
            "stage": "task_done",
            "path": "", "skipped": false,
            "completed": 0, "total": 1
        }),
    );

    let bytes = http
        .get(&url)
        .header("User-Agent", user_agent())
        .send()
        .await
        .map_err(|e| format!("下载失败: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("读取下载内容失败: {}", e))?;
    std::fs::write(&dest, &bytes).map_err(|e| format!("写入文件失败: {}", e))?;

    let _ = app.emit(
        "install-progress",
        serde_json::json!({
            "stage": "task_done",
            "path": "", "skipped": false,
            "completed": 1, "total": 1
        }),
    );

    if !req.instance_version_id.trim().is_empty() {
        let mut manifest = read_manifest(&game_root, &req.instance_version_id);
        if let Some(pos) = manifest.iter().position(|e| e.project_id == req.project_id) {
            manifest[pos].file_name = filename.clone();
            manifest[pos].kind = req.project_type.clone();
        } else {
            manifest.push(ManifestEntry {
                project_id: req.project_id.clone(),
                file_name: filename.clone(),
                kind: req.project_type.clone(),
            });
        }
        let _ = write_manifest(&game_root, &req.instance_version_id, &manifest);
    }

    emit_log(
        &app,
        "success",
        format!("{} 已安装到 {}（{}）", filename, subdir, req.instance_version_id),
    );

    Ok(InstallResult {
        version_id: req.instance_version_id,
        downloaded: 1,
        skipped: 0,
        failed: 0,
        errors: Vec::new(),
        game_root: game_root.to_string_lossy().into_owned(),
    })
}

/// 通过外链导入整合包
#[tauri::command]
pub async fn import_mrpack_url(
    app: AppHandle,
    req: ImportMrpackUrlRequest,
) -> Result<ImportMrpackResult, String> {
    let url = req.url.trim();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("请填写 http(s) 开头的整合包直链".to_string());
    }

    let http = app.state::<AppState>().http_client.clone();
    emit_log(&app, "info", "下载外链整合包…");

    let response = http
        .get(url)
        .header("User-Agent", user_agent())
        .send()
        .await
        .map_err(|e| format!("下载整合包失败: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("下载整合包返回状态 {}", response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("读取整合包失败: {}", e))?;

    let tmp = std::env::temp_dir().join(format!("mscl-url-pack-{}.mrpack", std::process::id()));
    std::fs::write(&tmp, &bytes).map_err(|e| format!("写入整合包失败: {}", e))?;

    let result = import_mrpack(
        app.clone(),
        tmp.to_string_lossy().into_owned(),
        req.target_dir.clone(),
    )
    .await?;
    let _ = std::fs::remove_file(&tmp);
    emit_log(
        &app,
        "success",
        format!("整合包「{}」导入完成（{}）", result.pack_name, result.target_version),
    );
    Ok(result)
}

/// 获取资源详情（点击资源后展示描述与前置依赖）
#[tauri::command]
pub async fn get_resource_detail(
    app: AppHandle,
    req: crate::models::ResourceDetailRequest,
) -> Result<ResourceDetail, String> {
    let source = resolve_source(&app, req.source_id.as_deref());
    let client = app.state::<AppState>().http_client.clone();
    let base = source_base(&source);

    if source.kind == "spigot" {
        let d: serde_json::Value = client
            .get(format!("{}/resources/{}", base, req.project_id))
            .header("User-Agent", user_agent())
            .send()
            .await
            .map_err(|e| format!("查询 Spigot 资源详情失败: {}", e))?
            .json()
            .await
            .map_err(|e| format!("解析 Spigot 资源详情失败: {}", e))?;
        let icon = d
            .get("icon")
            .and_then(|value| value.get("url"))
            .and_then(|value| value.as_str())
            .unwrap_or("");
        return Ok(ResourceDetail {
            project_id: req.project_id.clone(),
            title: d
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            description: d
                .get("tag")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            icon_url: if icon.is_empty() {
                None
            } else if icon.starts_with("http") {
                Some(icon.to_string())
            } else {
                Some(format!("https://www.spigotmc.org/{}", icon.trim_start_matches('/')))
            },
            downloads: d.get("downloads").and_then(|v| v.as_u64()).unwrap_or(0),
            dependencies: Vec::new(),
            source_kind: "spigot".to_string(),
        });
    }

    if source.kind == "minebbs" {
        let id = req.project_id.parse::<u64>().unwrap_or(0);
        let json: serde_json::Value = client
            .get(format!("{}/resources/{}", base, id))
            .header("User-Agent", user_agent())
            .send()
            .await
            .map_err(|e| format!("查询资源详情失败: {}", e))?
            .json()
            .await
            .map_err(|e| format!("解析资源详情失败: {}", e))?;
        let d = json.get("data").cloned().unwrap_or_default();
        return Ok(ResourceDetail {
            project_id: req.project_id.clone(),
            title: d.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            description: d.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            icon_url: d.get("icon_url").and_then(|v| v.as_str()).map(String::from),
            downloads: d.get("download_count").and_then(|v| v.as_u64()).unwrap_or(0),
            dependencies: Vec::new(),
            source_kind: "minebbs".to_string(),
        });
    }

    if source.kind == "curseforge" {
        need_key(&source)?;
        let json: serde_json::Value = client
            .get(format!("{}/v1/mods/{}", base, req.project_id))
            .header("x-api-key", source.api_key.trim())
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("查询资源详情失败: {}", e))?
            .json()
            .await
            .map_err(|e| format!("解析资源详情失败: {}", e))?;
        let d = json.get("data").cloned().unwrap_or_default();
        let deps: Vec<String> = d
            .get("dependencies")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.get("mod").and_then(|m| m.get("name")).and_then(|n| n.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        return Ok(ResourceDetail {
            project_id: req.project_id.clone(),
            title: d.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            description: d.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            icon_url: d.get("logo").and_then(|l| l.get("thumbnailUrl")).and_then(|v| v.as_str()).map(String::from),
            downloads: d.get("downloadCount").and_then(|v| v.as_u64()).unwrap_or(0),
            dependencies: deps,
            source_kind: "curseforge".to_string(),
        });
    }

    // Modrinth
    let json: serde_json::Value = client
        .get(format!("{}/project/{}", base, req.project_id))
        .header("User-Agent", user_agent())
        .send()
        .await
        .map_err(|e| format!("查询资源详情失败: {}", e))?
        .json()
        .await
        .map_err(|e| format!("解析资源详情失败: {}", e))?;

    let dep_ids: Vec<String> = json
        .get("dependencies")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|d| d.get("dependency_type").and_then(|t| t.as_str()) == Some("required"))
                .filter_map(|d| d.get("project_id").and_then(|p| p.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let dependencies = fetch_titles(&client, &base, &dep_ids).await;

    Ok(ResourceDetail {
        project_id: req.project_id.clone(),
        title: json.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        description: json
            .get("body")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| json.get("description").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string(),
        icon_url: json.get("icon_url").and_then(|v| v.as_str()).map(String::from),
        downloads: json.get("downloads").and_then(|v| v.as_u64()).unwrap_or(0),
        dependencies,
        source_kind: "modrinth".to_string(),
    })
}

/// 从拖入的本地文件路径识别资源类型（按扩展名 + 文件名启发式）
#[tauri::command]
pub fn detect_resource_file(path: String) -> Result<DetectedResource, String> {
    let p = std::path::Path::new(&path);
    let file_name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.clone());
    let lower = file_name.to_lowercase();

    let (project_type, kind, loader) = if lower.ends_with(".mrpack") {
        ("modpack".to_string(), "mods".to_string(), None)
    } else if lower.ends_with(".jar") {
        // 常见加载器命名线索；识别不出则由用户手动选
        let guessed = if lower.contains("fabric") {
            Some("fabric".to_string())
        } else if lower.contains("neoforge") {
            Some("neoforge".to_string())
        } else if lower.contains("forge") {
            Some("forge".to_string())
        } else {
            None
        };
        ("mod".to_string(), "mods".to_string(), guessed)
    } else if lower.ends_with(".zip") {
        if lower.contains("shader") {
            ("shader".to_string(), "shaderpacks".to_string(), None)
        } else if lower.contains("data") || lower.contains("datapack") {
            ("datapack".to_string(), "datapacks".to_string(), None)
        } else {
            ("resourcepack".to_string(), "resourcepacks".to_string(), None)
        }
    } else {
        ("unknown".to_string(), "mods".to_string(), None)
    };

    Ok(DetectedResource {
        path,
        file_name,
        project_type,
        kind,
        loader,
    })
}
