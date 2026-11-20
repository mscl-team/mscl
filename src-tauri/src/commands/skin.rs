use base64::Engine;
use tauri::{AppHandle, Manager};

use crate::models::{SkinAssetDto, SkinProfileDto};
use crate::state::{self, AppState};

const USER_AGENT: &str = "MSCL/0.1.0 (skin viewer)";

/// 昵称查询专用客户端：短连接/总超时，避免网络不通时长时间卡住。
/// 与下载大文件用的共享 client 分开，查询应在数秒内给出结果。
fn lookup_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(std::time::Duration::from_secs(6))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// 把网络错误分类成对用户有意义的提示：区分超时、连接失败与其他。
fn describe_network_error(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "查询超时（网络不可达），请检查网络后重试或改用 NameMC 来源".to_string()
    } else if e.is_connect() {
        "无法连接皮肤服务器，请检查网络或代理设置".to_string()
    } else {
        format!("网络请求失败: {}", e)
    }
}

fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    Some((width, height))
}

fn is_skin_png(bytes: &[u8]) -> bool {
    matches!(png_size(bytes), Some((64, 64)) | Some((64, 32)))
}

fn data_url(bytes: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('.').trim();
    if trimmed.is_empty() {
        "skin".to_string()
    } else {
        trimmed.to_string()
    }
}

fn asset_dir(app: &AppHandle, kind: &str) -> std::path::PathBuf {
    app.state::<AppState>()
        .game_root
        .join(if kind == "cape" { "capes" } else { "skins" })
}

fn save_png(
    app: &AppHandle,
    name: &str,
    bytes: &[u8],
    kind: &str,
    source: &str,
) -> Result<SkinAssetDto, String> {
    if !is_skin_png(bytes) {
        return Err("不是有效的 Minecraft 皮肤/披风 PNG（需要 64x64 或 64x32）".to_string());
    }
    let dir = asset_dir(app, kind);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建资源目录失败: {}", e))?;
    let base = safe_name(name);
    let mut path = dir.join(format!("{}.png", base));
    let mut index = 2;
    while path.exists() {
        path = dir.join(format!("{}-{}.png", base, index));
        index += 1;
    }
    std::fs::write(&path, bytes).map_err(|e| format!("保存文件失败: {}", e))?;
    Ok(SkinAssetDto {
        name: path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(&base)
            .to_string(),
        path: path.to_string_lossy().into_owned(),
        data_url: data_url(bytes),
        source: source.to_string(),
        kind: kind.to_string(),
    })
}

async fn download_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("皮肤地址无效".to_string());
    }
    let response = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| format!("下载皮肤失败: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("皮肤服务器返回状态 {}", response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("读取皮肤数据失败: {}", e))?;
    if bytes.len() > 5 * 1024 * 1024 {
        return Err("皮肤文件过大".to_string());
    }
    Ok(bytes.to_vec())
}

/// 查询正版账号皮肤；source = namemc 时优先抓取 NameMC 页面
#[tauri::command]
pub async fn fetch_skin_profile(
    app: AppHandle,
    username: String,
    source: Option<String>,
) -> Result<SkinProfileDto, String> {
    let name = username.trim();
    if name.is_empty() {
        return Err("请输入游戏昵称".to_string());
    }
    let client = app.state::<AppState>().http_client.clone();
    let source = source.unwrap_or_else(|| "mojang".to_string());

    if source == "namemc" {
        let lookup = lookup_client();
        let response = lookup
            .get(format!("https://namemc.com/profile/{}", name))
            .header("User-Agent", USER_AGENT)
            .send()
            .await
            .map_err(|e| format!("连接 NameMC 失败: {}", describe_network_error(&e)))?;
        if response.status().is_success() {
            let html = response.text().await.unwrap_or_default();
            if let Some(hash) = extract_texture_hash(&html) {
                let skin_url = format!("https://textures.minecraft.net/texture/{}", hash);
                let bytes = download_bytes(&client, &skin_url).await.ok();
                return Ok(SkinProfileDto {
                    name: name.to_string(),
                    uuid: String::new(),
                    skin_url,
                    skin_data_url: bytes.as_deref().map(data_url).unwrap_or_default(),
                    cape_url: None,
                    cape_data_url: String::new(),
                    model: if html.contains("slim") { "slim".to_string() } else { "classic".to_string() },
                    source: "namemc".to_string(),
                });
            }
        }
        return Err("NameMC 没有解析到该玩家的皮肤（页面可能被 Cloudflare 拦截或昵称不存在）".to_string());
    }

    let profile = fetch_mojang_profile(&client, name).await?;
    Ok(profile)
}

fn extract_texture_hash(html: &str) -> Option<String> {
    let marker = "textures.minecraft.net/texture/";
    let start = html.find(marker)? + marker.len();
    let hash: String = html[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_hexdigit())
        .collect();
    if hash.len() >= 32 {
        Some(hash)
    } else {
        None
    }
}

/// 尝试多个端点按昵称解析 UUID。
/// 返回 Ok(None) 表示端点可用但账号确实不存在（404）；Err 表示网络/服务问题。
async fn lookup_uuid(client: &reqwest::Client, name: &str) -> Result<Option<(String, String)>, String> {
    // 端点1：Mojang 官方 API（现由 api.minecraftservices.com 提供服务）。
    // 端点2：旧版 api.mojang.com，作为兼容回退。
    // 端点3：PlayerDB 公共 API，按昵称查 UUID（Crafatar 只认 UUID，不支持昵称）。
    let endpoints = [
        format!("https://api.minecraftservices.com/minecraft/profile/lookup/name/{}", name),
        format!("https://api.mojang.com/users/profiles/minecraft/{}", name),
        format!("https://playerdb.co/api/player/minecraft/{}", name),
    ];

    let mut last_network_error: Option<String> = None;
    for url in endpoints {
        let response = match client.get(&url).header("User-Agent", USER_AGENT).send().await {
            Ok(response) => response,
            Err(e) => {
                last_network_error = Some(describe_network_error(&e));
                continue;
            }
        };
        // 404 / 204 表示该端点明确“账号不存在”，可据此确认，无需再试。
        if response.status() == reqwest::StatusCode::NOT_FOUND
            || response.status() == reqwest::StatusCode::NO_CONTENT
        {
            return Ok(None);
        }
        if !response.status().is_success() {
            last_network_error = Some(format!("皮肤服务返回状态 {}", response.status()));
            continue;
        }
        let json: serde_json::Value = match response.json().await {
            Ok(value) => value,
            Err(_) => continue, // 空正文（部分端点 204 用 200 返回）视作该端点未命中
        };
        // PlayerDB 结构：{ data: { player: { id, username } } }，且 id 带横线；
        // Mojang 结构：{ id, name }（无横线）。统一去掉横线便于后续拼接 URL。
        let player = json.get("data").and_then(|v| v.get("player")).unwrap_or(&json);
        let uuid = player
            .get("id")
            .and_then(|v| v.as_str())
            .or_else(|| player.get("uuid").and_then(|v| v.as_str()))
            .map(|v| v.replace('-', ""));
        if let Some(uuid) = uuid {
            let real_name = player
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| player.get("username").and_then(|v| v.as_str()))
                .unwrap_or(name)
                .to_string();
            return Ok(Some((uuid, real_name)));
        }
        // 200 但明确带错误标记（如 PlayerDB 限流）：记录为服务问题，不能当成“不存在”。
        if json.get("error").and_then(|v| v.as_bool()).unwrap_or(false) {
            let reason = json
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("皮肤查询服务暂时不可用");
            last_network_error = Some(reason.to_string());
        }
    }

    // 所有端点都因网络/服务问题失败：不要把网络故障误报成“账号不存在”。
    match last_network_error {
        Some(reason) => Err(reason),
        None => Ok(None),
    }
}

async fn fetch_mojang_profile(
    download_client: &reqwest::Client,
    name: &str,
) -> Result<SkinProfileDto, String> {
    let lookup = lookup_client();
    let (uuid, real_name) = match lookup_uuid(&lookup, name).await? {
        Some(found) => found,
        None => {
            return Err(format!(
                "没有找到正版账号「{}」（离线账号没有皮肤与披风数据）",
                name
            ));
        }
    };

    let mut skin_url = format!("https://crafatar.com/skins/{}?overlay", uuid);
    let mut model = "classic".to_string();
    let mut cape_url: Option<String> = None;
    if let Ok(response) = lookup
        .get(format!(
            "https://sessionserver.mojang.com/session/minecraft/profile/{}",
            uuid
        ))
        .header("User-Agent", USER_AGENT)
        .send()
        .await
    {
        if let Ok(json) = response.json::<serde_json::Value>().await {
            if let Some(encoded) = json
                .get("properties")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|value| value.get("value"))
                .and_then(|value| value.as_str())
            {
                if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(encoded) {
                    if let Ok(textures) = serde_json::from_slice::<serde_json::Value>(&decoded) {
                        if let Some(url) = textures
                            .get("textures")
                            .and_then(|value| value.get("SKIN"))
                            .and_then(|value| value.get("url"))
                            .and_then(|value| value.as_str())
                        {
                            skin_url = url.to_string();
                        }
                        if let Some(url) = textures
                            .get("textures")
                            .and_then(|value| value.get("CAPE"))
                            .and_then(|value| value.get("url"))
                            .and_then(|value| value.as_str())
                        {
                            cape_url = Some(url.to_string());
                        }
                        if let Some(value) = textures
                            .get("textures")
                            .and_then(|value| value.get("SKIN"))
                            .and_then(|value| value.get("metadata"))
                            .and_then(|value| value.get("model"))
                            .and_then(|value| value.as_str())
                        {
                            model = value.to_string();
                        }
                    }
                }
            }
        }
    }

    let skin_data_url = download_bytes(download_client, &skin_url)
        .await
        .ok()
        .filter(|bytes| is_skin_png(bytes))
        .map(|bytes| data_url(&bytes))
        .unwrap_or_default();

    let cape_url = cape_url.or_else(|| Some(format!("https://crafatar.com/capes/{}", uuid)));
    let cape_data_url = match cape_url.as_deref() {
        Some(url) => download_bytes(download_client, url)
            .await
            .ok()
            .filter(|bytes| png_size(bytes).is_some())
            .map(|bytes| data_url(&bytes))
            .unwrap_or_default(),
        None => String::new(),
    };

    Ok(SkinProfileDto {
        name: real_name,
        uuid: uuid.clone(),
        skin_url,
        skin_data_url,
        cape_url,
        cape_data_url,
        model,
        source: "mojang".to_string(),
    })
}

/// 下载远程皮肤到本地 skins 目录
#[tauri::command]
pub async fn download_skin(
    app: AppHandle,
    url: String,
    name: String,
) -> Result<SkinAssetDto, String> {
    let client = app.state::<AppState>().http_client.clone();
    let bytes = download_bytes(&client, &url).await?;
    save_png(&app, &name, &bytes, "skin", "download")
}

/// 下载远程披风到本地 capes 目录
#[tauri::command]
pub async fn download_cape(
    app: AppHandle,
    url: String,
    name: String,
) -> Result<SkinAssetDto, String> {
    let client = app.state::<AppState>().http_client.clone();
    let bytes = download_bytes(&client, &url).await?;
    save_png(&app, &name, &bytes, "cape", "download")
}

/// 保存前端编辑器产出的 data URL
#[tauri::command]
pub fn save_skin_data(
    app: AppHandle,
    name: String,
    data: String,
) -> Result<SkinAssetDto, String> {
    let encoded = data
        .split_once(',')
        .map(|(_, value)| value)
        .unwrap_or(data.as_str());
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| format!("皮肤数据解码失败: {}", e))?;
    save_png(&app, &name, &bytes, "skin", "download")
}

/// 保存前端编辑器产出的披风 data URL
#[tauri::command]
pub fn save_cape_data(
    app: AppHandle,
    name: String,
    data: String,
) -> Result<SkinAssetDto, String> {
    let encoded = data
        .split_once(',')
        .map(|(_, value)| value)
        .unwrap_or(data.as_str());
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| format!("披风数据解码失败: {}", e))?;
    save_png(&app, &name, &bytes, "cape", "download")
}

/// 导入本地 PNG 皮肤
#[tauri::command]
pub fn import_skin_file(app: AppHandle, path: String) -> Result<SkinAssetDto, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("读取皮肤失败: {}", e))?;
    let name = std::path::Path::new(&path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("skin");
    save_png(&app, name, &bytes, "skin", "import")
}

/// 导入本地 PNG 披风
#[tauri::command]
pub fn import_cape_file(app: AppHandle, path: String) -> Result<SkinAssetDto, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("读取披风失败: {}", e))?;
    let name = std::path::Path::new(&path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("cape");
    save_png(&app, name, &bytes, "cape", "import")
}

fn push_skin(out: &mut Vec<SkinAssetDto>, path: &std::path::Path, source: &str, kind: &str) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if !meta.is_file() || meta.len() > 5 * 1024 * 1024 {
        return;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    if !is_skin_png(&bytes) {
        return;
    }
    if out.iter().any(|item| item.path == path.to_string_lossy()) {
        return;
    }
    out.push(SkinAssetDto {
        name: path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("skin")
            .to_string(),
        path: path.to_string_lossy().into_owned(),
        data_url: data_url(&bytes),
        source: source.to_string(),
        kind: kind.to_string(),
    });
}

fn scan_png_dir(out: &mut Vec<SkinAssetDto>, dir: &std::path::Path, source: &str, kind: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()).map(|value| value.eq_ignore_ascii_case("png")).unwrap_or(false) {
            push_skin(out, &path, source, kind);
        }
    }
}

/// 本地皮肤：skins/ + 主目录/实例 resourcepacks 中的皮肤 PNG
#[tauri::command]
pub fn list_local_skins(app: AppHandle) -> Result<Vec<SkinAssetDto>, String> {
    let game_root = app.state::<AppState>().game_root.clone();
    let mut out = Vec::new();
    scan_png_dir(&mut out, &game_root.join("skins"), "download", "skin");
    scan_png_dir(&mut out, &game_root.join("capes"), "download", "cape");
    scan_png_dir(&mut out, &game_root.join("resourcepacks"), "resourcepack", "skin");
    if let Ok(roots) = state::all_game_roots() {
        for root in roots {
            let instances = root.join("instances");
            if let Ok(entries) = std::fs::read_dir(instances) {
                for entry in entries.flatten() {
                    scan_png_dir(
                        &mut out,
                        &entry.path().join("resourcepacks"),
                        "instance",
                        "skin",
                    );
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}
