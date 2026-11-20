//! 实例创建（阶段 13）
//!
//! 一个实例 = 原版 + 可选加载器 + 可选附属 API，并带用户自定义名称。
//! 实例目录沿用版本隔离的约定（`instances/{version_id}`），
//! 展示名称与来源记录在 `instances/{version_id}/instance.json`。
//!
//! 冲突规则：
//!   - 主加载器互斥（vanilla / fabric / forge / neoforge 只能选一个）
//!   - Fabric API 仅对 fabric 生效
//!   - OptiFine 在 Fabric 下需额外搭配 OptiFabric（这里只下载并提示）

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::game::{install_fabric, install_forge, install_game};
use crate::models::{
    CreateInstanceRequest, InstallFabricRequest, InstallForgeRequest, InstallGameRequest,
    InstallLogEvent, InstallProgressEvent, InstallResult,
};
use crate::state::{self, AppState};

/// 实例元数据（写入 instances/{version_id}/instance.json）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceMeta {
    pub name: String,
    #[serde(rename = "minecraftVersion")]
    pub minecraft_version: String,
    pub loader: String,
    #[serde(rename = "loaderVersion", default)]
    pub loader_version: Option<String>,
    #[serde(default)]
    pub apis: Vec<String>,
    #[serde(rename = "createdAt", default)]
    pub created_at: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// 实例级"启动前校验完整性"覆盖；None 表示继承全局
    #[serde(rename = "verifyGameOnLaunch", default)]
    pub verify_game_on_launch: Option<bool>,
    #[serde(rename = "verifyResourcesOnLaunch", default)]
    pub verify_resources_on_launch: Option<bool>,
    #[serde(rename = "checkDependencies", default)]
    pub check_dependencies: Option<bool>,
}

pub(crate) fn emit_log(app: &AppHandle, level: &str, message: impl Into<String>) {
    emit_log_with_category(app, "install", level, message);
}

/// 更新类日志（版本校验 / 修复 / 更新），前端可单独隐藏
pub(crate) fn emit_update_log(app: &AppHandle, level: &str, message: impl Into<String>) {
    emit_log_with_category(app, "update", level, message);
}

pub(crate) fn emit_log_with_category(
    app: &AppHandle,
    category: &str,
    level: &str,
    message: impl Into<String>,
) {
    let message = message.into();
    let _ = app.emit(
        "install-log",
        InstallLogEvent {
            level: level.to_string(),
            message: message.clone(),
            category: category.to_string(),
        },
    );

    // 同步落盘到当前日志目录；写文件失败不应影响安装流程
    let dir = state::current_log_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let stamp = now.format(&Rfc3339).unwrap_or_default();
    let day = stamp.get(..10).unwrap_or("unknown");
    let path = dir.join(format!("mscl-{}.log", day));
    let line = format!("{} [{}] {}\n", stamp, level.to_uppercase(), message);
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(line.as_bytes());
    }
}

/// 推送安装控制状态（running / paused / cancelled），前端据此切换按钮
fn emit_control(app: &AppHandle, state: &str) {
    let _ = app.emit(
        "install-progress",
        InstallProgressEvent::Control {
            state: state.to_string(),
        },
    );
}

/// 暂停后等待用户继续或取消；返回 Err 表示用户取消
async fn wait_for_resume(app: &AppHandle) -> Result<(), String> {
    loop {
        {
            let state = app.state::<AppState>();
            if state.is_install_cancelled() {
                return Err("已取消安装".to_string());
            }
            if !state.is_install_paused() {
                return Ok(());
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
}

/// 取实例目录
fn instance_dir(game_root: &std::path::Path, version_id: &str) -> PathBuf {
    game_root.join("instances").join(version_id)
}

/// 推送多步骤任务（创建实例）的阶段进度：step 从 1 开始
fn emit_step(app: &AppHandle, label: impl Into<String>, step: usize, steps: usize) {
    emit_step_detail(app, label, step, steps, 0, 0);
}

/// 推送某步骤内的细化进度；total = 0 表示只有阶段进度
fn emit_step_detail(
    app: &AppHandle,
    label: impl Into<String>,
    step: usize,
    steps: usize,
    completed: usize,
    total: usize,
) {
    let _ = app.emit(
        "install-progress",
        InstallProgressEvent::Step {
            label: label.into(),
            step,
            steps,
            completed,
            total,
        },
    );
}

/// 校验实例 / 版本 ID，防止路径穿越
fn is_safe_instance_id(id: &str) -> bool {
    !id.trim().is_empty()
        && !id.contains('/')
        && !id.contains('\\')
        && !id.contains("..")
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}

/// 从 NeoForge maven-metadata 里挑与 MC 版本匹配的最高版本
fn neoforge_prefix(minecraft_version: &str) -> Vec<String> {
    // 1.20.1 是历史特例：NeoForge 发布在 47.x
    if minecraft_version == "1.20.1" {
        return vec!["47.".to_string()];
    }
    let parts: Vec<&str> = minecraft_version.split('.').collect();
    if parts.len() >= 3 {
        // 1.21.1 -> 21.1.
        vec![format!("{}.{}.", parts[1], parts[2])]
    } else if parts.len() == 2 {
        vec![format!("{}.0.", parts[1])]
    } else {
        Vec::new()
    }
}

async fn neoforge_versions(
    client: &reqwest::Client,
    minecraft_version: &str,
) -> Result<Vec<String>, String> {
    let url = "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";
    let text = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("获取 NeoForge 版本列表失败: {}", e))?
        .text()
        .await
        .map_err(|e| format!("读取 NeoForge 版本列表失败: {}", e))?;

    let all: Vec<String> = text
        .split("<version>")
        .skip(1)
        .filter_map(|chunk| chunk.split("</version>").next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let prefixes = neoforge_prefix(minecraft_version);
    let mut matched: Vec<String> = all
        .into_iter()
        .filter(|v| prefixes.iter().any(|p| v.starts_with(p.as_str())))
        .collect();

    if matched.is_empty() {
        return Err(format!(
            "NeoForge 没有匹配 Minecraft {} 的版本（可尝试 1.20.2 及以上，或改用 Forge）",
            minecraft_version
        ));
    }
    matched.sort_by_key(|v| version_key(v));
    Ok(matched)
}

/// 版本号排序键：按数字段逐段比较
fn version_key(v: &str) -> Vec<u64> {
    v.split(['.', '-', '+'])
        .map(|p| p.parse::<u64>().unwrap_or(0))
        .collect()
}

/// 安装 NeoForge（复用 installer 的静默安装方式）
async fn install_neoforge(
    app: &AppHandle,
    minecraft_version: &str,
    wanted: Option<String>,
) -> Result<String, String> {
    let (http, game_root) = {
        let state = app.state::<AppState>();
        (state.http_client.clone(), state.game_root.clone())
    };

    let base_json = game_root.join(format!("versions/{0}/{0}.json", minecraft_version));
    if !base_json.exists() {
        return Err(format!("请先安装原版 {} 后再安装 NeoForge", minecraft_version));
    }

    let versions = neoforge_versions(&http, minecraft_version).await?;
    let forge_version = match wanted.as_deref().map(str::trim) {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => versions.last().cloned().unwrap_or_default(),
    };
    emit_log(
        app,
        "info",
        format!("准备 NeoForge {}（Minecraft {}）", forge_version, minecraft_version),
    );

    // installer 要求 launcher_profiles.json
    let profiles = game_root.join("launcher_profiles.json");
    if !profiles.exists() {
        let minimal = serde_json::json!({ "profiles": {}, "settings": {}, "version": 3 });
        std::fs::write(
            &profiles,
            serde_json::to_string_pretty(&minimal).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("创建 launcher_profiles.json 失败: {}", e))?;
    }

    let installer_url = format!(
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/{0}/neoforge-{0}-installer.jar",
        forge_version
    );
    let bytes = http
        .get(&installer_url)
        .send()
        .await
        .map_err(|e| format!("下载 NeoForge 安装器失败: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("读取 NeoForge 安装器失败: {}", e))?;
    let installer_path =
        std::env::temp_dir().join(format!("mscl-neoforge-{}-installer.jar", forge_version));
    std::fs::write(&installer_path, &bytes)
        .map_err(|e| format!("写入 NeoForge 安装器失败: {}", e))?;
    emit_log(app, "info", "已下载 NeoForge 安装器，开始执行…");

    let java = mscl_platform::detect_java().map_err(|e| format!("未找到 Java: {}", e))?;
    let mut command = tokio::process::Command::new(&java.path);
    command
        .arg("-jar")
        .arg(&installer_path)
        .arg("--installClient")
        .arg(game_root.as_os_str());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = tokio::time::timeout(std::time::Duration::from_secs(900), command.output())
        .await
        .map_err(|_| "NeoForge 安装器超时（15 分钟）".to_string())?
        .map_err(|e| format!("执行 NeoForge 安装器失败: {}", e))?;

    let _ = std::fs::remove_file(&installer_path);

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "NeoForge 安装器失败（{:?}）\n{}\n{}",
            output.status.code(),
            stdout.chars().rev().take(800).collect::<String>().chars().rev().collect::<String>(),
            stderr.chars().rev().take(800).collect::<String>().chars().rev().collect::<String>()
        ));
    }

    // 版本目录名形如 neoforge-21.1.80
    let expected = format!("neoforge-{}", forge_version);
    if game_root
        .join("versions")
        .join(&expected)
        .join(format!("{}.json", expected))
        .exists()
    {
        emit_log(app, "success", format!("NeoForge 安装完成：{}", expected));
        return Ok(expected);
    }
    // 兜底：扫描 neoforge- 前缀
    if let Ok(entries) = std::fs::read_dir(game_root.join("versions")) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("neoforge-")
                && entry.path().join(format!("{}.json", name)).exists()
            {
                emit_log(app, "success", format!("NeoForge 安装完成：{}", name));
                return Ok(name);
            }
        }
    }
    Err("NeoForge 安装器未生成版本目录".to_string())
}

/// 流式下载单个文件：临时文件 + 重试 + 不整块读内存
///
/// 直接 `response.bytes()` 会把整个文件读进内存，慢网络下还容易在读取阶段
/// 被连接中断打断（报 "error decoding response body"）。这里改成边收边写，
/// 失败重试 3 次，成功后再原子替换目标文件。
async fn download_file_with_retry(
    client: &reqwest::Client,
    url: &str,
    dest: &std::path::Path,
    label: &str,
    stop: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<u64, String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    let tmp = dest.with_extension("part");
    let mut last_err = String::new();

    for attempt in 0..4u32 {
        if attempt > 0 {
            let delay = 500u64 * (1 << (attempt - 1).min(3));
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }

        let attempt_result: Result<u64, String> = async {
            let mut response = client
                .get(url)
                // 要求不压缩，避免个别 CDN 返回的编码让 reqwest 解码失败
                .header(reqwest::header::ACCEPT_ENCODING, "identity")
                .send()
                .await
                .map_err(|e| format!("连接失败: {}", e))?;
            if !response.status().is_success() {
                return Err(format!("服务器返回 {}", response.status()));
            }

            let mut file = tokio::fs::File::create(&tmp)
                .await
                .map_err(|e| format!("创建临时文件失败: {}", e))?;
            let mut written: u64 = 0;
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|e| format!("读取数据中断: {}", e))?
            {
                if stop
                    .as_ref()
                    .map(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
                    .unwrap_or(false)
                {
                    return Err("安装已暂停".to_string());
                }
                use tokio::io::AsyncWriteExt;
                file.write_all(&chunk)
                    .await
                    .map_err(|e| format!("写入文件失败: {}", e))?;
                written += chunk.len() as u64;
            }
            {
                use tokio::io::AsyncWriteExt;
                file.flush().await.map_err(|e| format!("写入文件失败: {}", e))?;
            }
            if written == 0 {
                return Err("下载内容为空".to_string());
            }
            Ok(written)
        }
        .await;

        match attempt_result {
            Ok(size) => {
                std::fs::rename(&tmp, dest).map_err(|e| format!("保存文件失败: {}", e))?;
                return Ok(size);
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                last_err = e;
            }
        }
    }

    Err(format!("{} 下载失败（已重试 3 次）: {}", label, last_err))
}

/// 从 Modrinth 安装 Fabric API 到实例 mods
async fn install_fabric_api(
    app: &AppHandle,
    minecraft_version: &str,
    version_id: &str,
    api_base: &str,
    step: usize,
    steps: usize,
) -> Result<(), String> {
    let (http, game_root) = {
        let state = app.state::<AppState>();
        (state.http_client.clone(), state.game_root.clone())
    };
    emit_log(app, "info", "安装 Fabric API…");
    emit_step_detail(app, "安装 Fabric API", step, steps, 0, 1);

    let search_url = format!(
        "{}/project/fabric-api/version?game_versions=%5B%22{}%22%5D&loaders=%5B%22fabric%22%5D",
        api_base.trim_end_matches('/'),
        minecraft_version
    );
    let versions: serde_json::Value = http
        .get(&search_url)
        .send()
        .await
        .map_err(|e| format!("查询 Fabric API 版本失败: {}", e))?
        .json()
        .await
        .map_err(|e| format!("解析 Fabric API 版本失败: {}", e))?;

    let file = versions
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("files"))
        .and_then(|f| f.as_array())
        .and_then(|arr| arr.iter().find(|f| f.get("primary").and_then(|p| p.as_bool()) == Some(true)).or_else(|| arr.first()))
        .ok_or_else(|| "未找到适配当前版本的 Fabric API".to_string())?;

    let url = file
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Fabric API 缺少下载地址".to_string())?;
    let filename = file
        .get("filename")
        .and_then(|v| v.as_str())
        .unwrap_or("fabric-api.jar");

    let dest = instance_dir(&game_root, version_id).join("mods").join(filename);
    // 已存在且非空：直接复用，避免重复下载拖慢安装
    if dest
        .metadata()
        .map(|m| m.len() > 0)
        .unwrap_or(false)
    {
        emit_step_detail(app, "安装 Fabric API", step, steps, 1, 1);
        emit_log(
            app,
            "success",
            format!("Fabric API 已存在，跳过下载：{}", filename),
        );
        return Ok(());
    }

    let size = download_file_with_retry(
        &http,
        url,
        &dest,
        "Fabric API",
        Some(app.state::<AppState>().install_cancel.clone()),
    )
    .await?;
    emit_step_detail(app, "安装 Fabric API", step, steps, 1, 1);
    emit_log(
        app,
        "success",
        format!("Fabric API 已安装：{}（{} KB）", filename, size / 1024),
    );
    Ok(())
}

/// 通过 BMCLAPI 下载 OptiFine 到实例 mods
async fn install_optifine(
    app: &AppHandle,
    minecraft_version: &str,
    version_id: &str,
) -> Result<(), String> {
    let (http, game_root) = {
        let state = app.state::<AppState>();
        (state.http_client.clone(), state.game_root.clone())
    };
    emit_log(app, "info", "查询 OptiFine 版本…");

    let list_url = format!(
        "https://bmclapi2.bangbang93.com/optifine/{}",
        minecraft_version
    );
    let list: serde_json::Value = http
        .get(&list_url)
        .send()
        .await
        .map_err(|e| format!("查询 OptiFine 失败: {}", e))?
        .json()
        .await
        .map_err(|e| format!("解析 OptiFine 列表失败: {}", e))?;

    let first = list
        .as_array()
        .and_then(|arr| arr.first())
        .ok_or_else(|| format!("{} 没有可用的 OptiFine 版本", minecraft_version))?;
    let kind = first.get("type").and_then(|v| v.as_str()).unwrap_or("HD_U");
    let patch = first.get("patch").and_then(|v| v.as_str()).unwrap_or("");
    if patch.is_empty() {
        return Err("OptiFine 版本信息不完整".to_string());
    }

    let url = format!(
        "https://bmclapi2.bangbang93.com/optifine/{}/{}/{}",
        minecraft_version, kind, patch
    );
    let filename = format!("OptiFine_{}_{}.jar", minecraft_version, patch);
    let dest = instance_dir(&game_root, version_id)
        .join("mods")
        .join(&filename);
    download_file_with_retry(
        &http,
        &url,
        &dest,
        "OptiFine",
        Some(app.state::<AppState>().install_cancel.clone()),
    )
    .await?;
    emit_log(
        app,
        "warn",
        format!(
            "OptiFine 已放入 mods（{}）。Fabric 实例需另装 OptiFabric 才能生效",
            filename
        ),
    );
    Ok(())
}

/// 创建实例：原版 → 加载器 → 附属 API → 记录元数据
async fn create_instance_inner(
    app: AppHandle,
    req: CreateInstanceRequest,
) -> Result<InstallResult, String> {
    let (game_root, settings) = {
        let state = app.state::<AppState>();
        let root = state.game_root.clone();
        let settings = crate::models::AppSettings::load_or_default(&root.join("settings.json"));
        (root, settings)
    };

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err("实例名称不能为空".to_string());
    }

    // 清零上一次的中断标记，本次安装由用户单独控制
    app.state::<AppState>().begin_install();

    emit_log(&app, "info", format!("开始创建实例「{}」", name));

    // 整体进度：原版 → 加载器 → 附属 API（可选）→ 收尾
    let has_api = (req.install_fabric_api && req.loader == "fabric") || req.install_optifine;
    let steps = if has_api { 4usize } else { 3usize };
    emit_control(&app, "running");

    /// 执行一个阶段：被暂停就停下来等继续，被取消就结束流程。
    /// 继续时会重跑该阶段（下载引擎会跳过已校验文件，相当于断点续传）。
    macro_rules! run_step {
        ($label:expr, $fut:expr) => {{
            loop {
                let result = $fut.await;
                {
                    let state = app.state::<AppState>();
                    if state.is_install_cancelled() {
                        emit_control(&app, "cancelled");
                        return Err("已取消安装".to_string());
                    }
                    if state.is_install_paused() {
                        drop(state);
                        emit_control(&app, "paused");
                        emit_log(&app, "warn", format!("已暂停：{}", $label));
                        if let Err(e) = wait_for_resume(&app).await {
                            emit_control(&app, "cancelled");
                            return Err(e);
                        }
                        emit_control(&app, "running");
                        emit_log(&app, "info", format!("继续安装：{}", $label));
                        continue;
                    }
                }
                break result;
            }
        }};
    }

    // 1. 原版（缺失则自动安装）
    emit_step(&app, format!("安装原版 {}", req.minecraft_version), 1, steps);
    let base_json = game_root.join(format!(
        "versions/{0}/{0}.json",
        req.minecraft_version
    ));
    if !base_json.exists() {
        emit_log(&app, "info", format!("安装原版 {}…", req.minecraft_version));
        let base_result = run_step!(
            "安装原版",
            install_game(
                app.clone(),
                InstallGameRequest {
                    version_id: req.minecraft_version.clone(),
                },
            )
        );
        base_result?;
    } else {
        emit_log(&app, "info", format!("原版 {} 已就绪", req.minecraft_version));
    }

    // 2. 加载器
    let loader_version = req.loader_version.clone();
    emit_step(
        &app,
        match req.loader.as_str() {
            "fabric" => "安装 Fabric 加载器".to_string(),
            "forge" => "安装 Forge 加载器".to_string(),
            "neoforge" => "安装 NeoForge 加载器".to_string(),
            _ => format!("准备原版 {}", req.minecraft_version),
        },
        2,
        steps,
    );
    let version_id = match req.loader.as_str() {
        "vanilla" => {
            emit_log(&app, "info", "使用原版，不安装加载器");
            req.minecraft_version.clone()
        }
        "fabric" => {
            emit_log(&app, "info", "安装 Fabric…");
            let result = run_step!(
                "安装 Fabric 加载器",
                install_fabric(
                    app.clone(),
                    InstallFabricRequest {
                        minecraft_version: req.minecraft_version.clone(),
                        loader_version: loader_version.clone(),
                    },
                )
            );
            result?.version_id
        }
        "forge" => {
            emit_log(&app, "info", "安装 Forge（需要几分钟）…");
            let result = run_step!(
                "安装 Forge 加载器",
                install_forge(
                    app.clone(),
                    InstallForgeRequest {
                        minecraft_version: req.minecraft_version.clone(),
                        forge_version: loader_version.clone(),
                    },
                )
            );
            result?.version_id
        }
        "neoforge" => {
            let result = run_step!(
                "安装 NeoForge 加载器",
                install_neoforge(&app, &req.minecraft_version, loader_version.clone())
            );
            result?
        }
        other => return Err(format!("不支持的加载器: {}", other)),
    };

    // 3. 实例资源依赖版本隔离：首次创建时自动开启，避免 Mod/存档串版本
    if !settings.version_isolation {
        let mut next = settings.clone();
        next.version_isolation = true;
        if let Ok(text) = serde_json::to_string_pretty(&next) {
            let tmp = game_root.join("settings.json.tmp");
            if std::fs::write(&tmp, text).is_ok()
                && std::fs::rename(&tmp, game_root.join("settings.json")).is_ok()
            {
                emit_log(&app, "info", "已自动开启版本隔离（实例的 Mod 与存档独立）");
                // 同步所有既有实例，保证全局开关与目录链接一致
                for warning in isolation::sync_all_instances(&game_root, &next) {
                    emit_log(&app, "warn", format!("版本隔离：{}", warning));
                }
            }
        }
    } else {
        // 隔离已开启：确保新实例的目录链接符合全局配置
        for warning in isolation::ensure_instance_links(&game_root, &version_id, &settings) {
            emit_log(&app, "warn", format!("版本隔离：{}", warning));
        }
    }

    // 4. 实例目录 + 元数据
    if app.state::<AppState>().is_install_cancelled() {
        emit_control(&app, "cancelled");
        return Err("已取消安装".to_string());
    }
    let dir = instance_dir(&game_root, &version_id);
    let dir_existed = dir.exists();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建实例目录失败: {}", e))?;

    // 取消时清理本次新建的实例目录，避免留下半成品
    macro_rules! abort_if_cancelled {
        () => {
            if app.state::<AppState>().is_install_cancelled() {
                if !dir_existed {
                    let _ = std::fs::remove_dir_all(&dir);
                }
                emit_log(&app, "warn", "已取消安装，已清理未完成的实例目录");
                return Err("已取消安装".to_string());
            }
        };
    }

    let mut apis: Vec<String> = Vec::new();
    let api_step = 3usize;
    if req.install_fabric_api && req.loader == "fabric" {
        emit_step(&app, "安装 Fabric API", api_step, steps);
        let api_result = run_step!(
            "安装 Fabric API",
            install_fabric_api(
                &app,
                &req.minecraft_version,
                &version_id,
                &settings.modrinth_api,
                api_step,
                steps,
            )
        );
        match api_result {
            Ok(()) => apis.push("fabric-api".to_string()),
            Err(e) => {
                abort_if_cancelled!();
                emit_log(&app, "warn", format!("Fabric API 安装失败：{}", e));
            }
        }
    }
    if req.install_optifine {
        emit_step(&app, "安装 OptiFine", api_step, steps);
        let of_result = run_step!(
            "安装 OptiFine",
            install_optifine(&app, &req.minecraft_version, &version_id)
        );
        match of_result {
            Ok(()) => apis.push("optifine".to_string()),
            Err(e) => {
                abort_if_cancelled!();
                emit_log(&app, "warn", format!("OptiFine 安装失败：{}", e));
            }
        }
    }

    // 收尾：写入实例元数据，进度到 100%
    emit_step_detail(
        &app,
        "保存实例配置",
        if has_api { 4 } else { 3 },
        steps,
        1,
        1,
    );

    let meta = InstanceMeta {
        name: name.clone(),
        minecraft_version: req.minecraft_version.clone(),
        loader: req.loader.clone(),
        loader_version: req.loader_version.clone(),
        apis,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            .to_string(),
        description: req.description.clone(),
        icon: req.icon.clone(),
        tags: req.tags.clone(),
        verify_game_on_launch: None,
        verify_resources_on_launch: None,
        check_dependencies: None,
    };
    let meta_text = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("instance.json"), meta_text)
        .map_err(|e| format!("写入实例信息失败: {}", e))?;

    emit_log(
        &app,
        "success",
        format!("实例「{}」创建完成（{}）", name, version_id),
    );

    Ok(InstallResult {
        version_id,
        downloaded: 0,
        skipped: 0,
        failed: 0,
        errors: Vec::new(),
        game_root: game_root.to_string_lossy().into_owned(),
    })
}

/// 创建实例的对外入口：统一收口进度事件（成功/失败都要通知前端收尾）
#[tauri::command]
pub async fn create_instance(
    app: AppHandle,
    req: CreateInstanceRequest,
) -> Result<InstallResult, String> {
    app.state::<AppState>().set_install_active(true);
    let result = create_instance_inner(app.clone(), req).await;
    // 流程结束：退出“进行中”状态并清空暂停/取消标记，避免影响下一次安装
    {
        let state = app.state::<AppState>();
        state.set_install_active(false);
        state.begin_install();
    }
    match &result {
        Ok(_) => {
            let _ = app.emit(
                "install-progress",
                InstallProgressEvent::Finished {
                    downloaded: 0,
                    skipped: 0,
                    failed: 0,
                },
            );
        }
        Err(message) => {
            let _ = app.emit(
                "install-progress",
                InstallProgressEvent::Error {
                    message: message.clone(),
                },
            );
        }
    }
    result
}

/// 暂停当前实例安装：停止下载，已校验完成的文件保留，可随时继续
#[tauri::command]
pub fn pause_instance_install(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    if !state.is_install_cancelled() {
        state.pause_install();
        emit_log(&app, "warn", "已暂停安装（已完成的文件会保留）");
        emit_control(&app, "paused");
    }
    Ok(())
}

/// 继续当前实例安装：从暂停处接着跑，已校验文件自动跳过
#[tauri::command]
pub fn resume_instance_install(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    if !state.is_install_cancelled() {
        state.resume_install();
        emit_log(&app, "info", "继续安装…");
        emit_control(&app, "running");
    }
    Ok(())
}

/// 安全中断当前实例安装：
/// 只置位中断标记，正在进行的下载会在下一个分片/重试点自行停下，
/// 已经校验通过的缓存文件保留，未完成的临时文件丢弃。
#[tauri::command]
pub fn cancel_instance_install(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    if !state.is_install_cancelled() {
        state.request_cancel_install();
        // 立刻把状态推给前端，按钮不会停在“取消安装”上
        emit_control(&app, "cancelled");
        emit_log(&app, "warn", "正在安全中断安装…（已下载的缓存文件会保留）");
    }
    Ok(())
}

/// 资源 kind 白名单 → 目录名
fn kind_dir(kind: &str) -> Result<&'static str, String> {
    match kind {
        "mods" | "mod" => Ok("mods"),
        "shaderpacks" | "shader" => Ok("shaderpacks"),
        "resourcepacks" | "resourcepack" => Ok("resourcepacks"),
        "datapacks" | "datapack" => Ok("datapacks"),
        "plugins" | "plugin" => Ok("plugins"),
        other => Err(format!("不支持的资源类型: {}", other)),
    }
}

use super::isolation;

fn safe_file_name(name: &str) -> Result<&str, String> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err("非法的文件名".to_string());
    }
    Ok(name)
}

/// 编辑实例信息（名称 / 描述 / 图标 / 标签）
#[tauri::command]
pub async fn update_instance_meta(
    app: AppHandle,
    req: crate::models::UpdateInstanceMetaRequest,
) -> Result<(), String> {
    let game_root = app.state::<AppState>().game_root.clone();
    let dir = instance_dir(&game_root, &req.version_id);
    let meta_path = dir.join("instance.json");

    if req.name.trim().is_empty() {
        return Err("实例名称不能为空".to_string());
    }

    let mut meta: InstanceMeta = if meta_path.exists() {
        let text = std::fs::read_to_string(&meta_path).map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| e.to_string())?
    } else {
        // 已安装的原版版本首次编辑：按版本 id 推断并创建实例元数据
        let mc = req
            .minecraft_version
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| req.version_id.clone());
        let loader = req
            .loader
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "vanilla".to_string());
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建实例目录失败: {}", e))?;
        InstanceMeta {
            name: req.name.trim().to_string(),
            minecraft_version: mc,
            loader,
            loader_version: req.loader_version.clone(),
            apis: Vec::new(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
                .to_string(),
            description: String::new(),
            icon: String::new(),
            tags: Vec::new(),
            verify_game_on_launch: None,
            verify_resources_on_launch: None,
            check_dependencies: None,
        }
    };
    meta.name = req.name.trim().to_string();
    meta.description = req.description;
    meta.icon = req.icon;
    meta.tags = req.tags;
    meta.verify_game_on_launch = req.verify_game_on_launch;
    meta.verify_resources_on_launch = req.verify_resources_on_launch;
    meta.check_dependencies = req.check_dependencies;

    let out = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    std::fs::write(&meta_path, out).map_err(|e| format!("保存实例信息失败: {}", e))?;
    // 编辑实例时顺带修复该实例的隔离链接（目录可能被手动改动）
    let settings = crate::models::AppSettings::load_or_default(&game_root.join("settings.json"));
    if settings.version_isolation {
        for warning in isolation::ensure_instance_links(&game_root, &req.version_id, &settings) {
            emit_log(&app, "warn", format!("版本隔离：{}", warning));
        }
    }
    emit_log(&app, "success", format!("实例「{}」信息已更新", meta.name));
    Ok(())
}

/// 列出实例内的资源文件
#[tauri::command]
pub async fn list_instance_resources(
    app: AppHandle,
    version_id: String,
) -> Result<Vec<crate::models::InstanceResource>, String> {
    let game_root = app.state::<AppState>().game_root.clone();
    let root = instance_dir(&game_root, &version_id);
    let mut out = Vec::new();

    for kind in ["mods", "shaderpacks", "resourcepacks", "datapacks"] {
        let dir = root.join(kind);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let raw = entry.file_name().to_string_lossy().into_owned();
            let (file_name, enabled) = if let Some(base) = raw.strip_suffix(".disabled") {
                (base.to_string(), false)
            } else {
                (raw, true)
            };
            let lower = file_name.to_lowercase();
            if !(lower.ends_with(".jar") || lower.ends_with(".zip")) {
                continue;
            }
            out.push(crate::models::InstanceResource {
                kind: kind.to_string(),
                file_name,
                size: entry.metadata().map(|m| m.len()).unwrap_or(0),
                enabled,
            });
        }
    }

    out.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.file_name.cmp(&b.file_name)));
    Ok(out)
}

/// 删除实例内的某个资源文件（连同 .disabled 变体）
#[tauri::command]
pub async fn delete_instance_resource(
    app: AppHandle,
    version_id: String,
    kind: String,
    file_name: String,
) -> Result<(), String> {
    let dir_name = kind_dir(&kind)?;
    safe_file_name(&file_name)?;
    let game_root = app.state::<AppState>().game_root.clone();
    let dir = instance_dir(&game_root, &version_id).join(dir_name);

    let mut removed = 0;
    for candidate in [file_name.clone(), format!("{}.disabled", file_name)] {
        let path = dir.join(&candidate);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("删除失败: {}", e))?;
            removed += 1;
        }
    }
    if removed == 0 {
        return Err(format!("文件不存在: {}", file_name));
    }
    emit_log(&app, "info", format!("已删除 {}/{}", dir_name, file_name));
    Ok(())
}

/// 把资源移动到另一个实例
#[tauri::command]
pub async fn move_instance_resource(
    app: AppHandle,
    req: crate::models::MoveResourceRequest,
) -> Result<(), String> {
    let dir_name = kind_dir(&req.kind)?;
    safe_file_name(&req.file_name)?;
    if req.from_version == req.to_version {
        return Err("源实例与目标实例相同".to_string());
    }

    let game_root = app.state::<AppState>().game_root.clone();
    let from_dir = instance_dir(&game_root, &req.from_version).join(dir_name);
    let to_dir = instance_dir(&game_root, &req.to_version).join(dir_name);
    std::fs::create_dir_all(&to_dir).map_err(|e| format!("创建目标目录失败: {}", e))?;

    let mut moved = false;
    for candidate in [req.file_name.clone(), format!("{}.disabled", req.file_name)] {
        let src = from_dir.join(&candidate);
        if !src.exists() {
            continue;
        }
        let dst = to_dir.join(&candidate);
        if dst.exists() {
            return Err(format!("目标实例已存在同名文件: {}", candidate));
        }
        std::fs::rename(&src, &dst).map_err(|e| format!("移动失败: {}", e))?;
        moved = true;
    }
    if !moved {
        return Err(format!("源实例中找不到: {}", req.file_name));
    }
    emit_log(
        &app,
        "success",
        format!(
            "已将 {} 从 {} 移动到 {}",
            req.file_name, req.from_version, req.to_version
        ),
    );
    Ok(())
}

/// 本地导入资源：文件 → 目标实例对应目录
#[tauri::command]
pub async fn import_local_resource(
    app: AppHandle,
    req: crate::models::ImportLocalResourceRequest,
) -> Result<crate::models::InstanceResource, String> {
    let dir_name = kind_dir(&req.kind)?;
    let src = std::path::Path::new(&req.path);
    if !src.is_file() {
        return Err(format!("文件不存在: {}", req.path));
    }
    let file_name = src
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "无法识别文件名".to_string())?
        .to_string();

    let game_root = app.state::<AppState>().game_root.clone();
    let base = match super::plugin::resolve_install_root(&app, req.target_dir.as_deref())? {
        Some(dir) => dir,
        None => instance_dir(&game_root, &req.version_id),
    };
    let to_dir = base.join(dir_name);
    std::fs::create_dir_all(&to_dir).map_err(|e| format!("创建目录失败: {}", e))?;

    let dst = to_dir.join(&file_name);
    std::fs::copy(src, &dst).map_err(|e| format!("复制文件失败: {}", e))?;
    let size = std::fs::metadata(&dst).map(|m| m.len()).unwrap_or(0);

    emit_log(
        &app,
        "success",
        format!("已导入 {} 到 {}/{}", file_name, req.version_id, dir_name),
    );

    Ok(crate::models::InstanceResource {
        kind: dir_name.to_string(),
        file_name,
        size,
        enabled: true,
    })
}

/// 扫描指定游戏目录下的实例（有 instance.json）
pub(crate) fn scan_instances(game_root: &std::path::Path) -> Vec<crate::models::InstanceInfo> {
    let mut out = Vec::new();
    let dir = game_root.join("instances");
    if !dir.exists() {
        return out;
    }
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let meta_path = entry.path().join("instance.json");
            let Ok(text) = std::fs::read_to_string(&meta_path) else {
                continue;
            };
            let Ok(meta) = serde_json::from_str::<InstanceMeta>(&text) else {
                continue;
            };
            out.push(crate::models::InstanceInfo {
                version_id: entry.file_name().to_string_lossy().into_owned(),
                name: meta.name,
                minecraft_version: meta.minecraft_version,
                loader: meta.loader,
                loader_version: meta.loader_version,
                description: meta.description,
                icon: meta.icon,
                tags: meta.tags,
                game_root: game_root.to_string_lossy().into_owned(),
            });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// 读取当前主目录的所有实例（有 instance.json 的版本）
#[tauri::command]
pub async fn list_instances(app: AppHandle) -> Result<Vec<crate::models::InstanceInfo>, String> {
    let game_root = app.state::<AppState>().game_root.clone();
    Ok(scan_instances(&game_root))
}

/// 兼容 HMCL / PCL 目录：没有 instance.json 的 versions/{id} 也作为实例展示
pub(crate) fn scan_versions(game_root: &std::path::Path) -> Vec<crate::models::InstanceInfo> {
    let mut out = scan_instances(game_root);
    let mut seen: std::collections::HashSet<String> =
        out.iter().map(|item| item.version_id.clone()).collect();
    let dir = game_root.join("versions");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let version_id = entry.file_name().to_string_lossy().into_owned();
        if !seen.insert(version_id.clone()) {
            continue;
        }
        let lower = version_id.to_lowercase();
        let loader = if lower.contains("neoforge") {
            "neoforge"
        } else if lower.contains("fabric") {
            "fabric"
        } else if lower.contains("quilt") {
            "quilt"
        } else if lower.contains("forge") {
            "forge"
        } else {
            "vanilla"
        };
        out.push(crate::models::InstanceInfo {
            version_id: version_id.clone(),
            name: version_id.clone(),
            minecraft_version: version_id,
            loader: loader.to_string(),
            loader_version: None,
            description: String::new(),
            icon: String::new(),
            tags: Vec::new(),
            game_root: game_root.to_string_lossy().into_owned(),
        });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// 删除实例：同时移除隔离目录（元数据 / 资源）与对应版本本体。
/// 兼容直装版本没有 instances/{id} 目录的情况，避免“点删除没反应”。
#[tauri::command]
pub async fn delete_instance(app: AppHandle, version_id: String) -> Result<(), String> {
    if !is_safe_instance_id(&version_id) {
        return Err("非法的实例 ID".to_string());
    }
    let game_root = app.state::<AppState>().game_root.clone();
    let instance_path = instance_dir(&game_root, &version_id);
    let version_path = game_root.join("versions").join(&version_id);

    let mut removed = false;
    let mut errors = Vec::new();

    if instance_path.exists() {
        match std::fs::remove_dir_all(&instance_path) {
            Ok(_) => removed = true,
            Err(e) => errors.push(format!("删除实例目录失败: {}", e)),
        }
    }
    if version_path.exists() {
        match std::fs::remove_dir_all(&version_path) {
            Ok(_) => removed = true,
            Err(e) => errors.push(format!("删除版本目录失败: {}", e)),
        }
    }

    if !removed && errors.is_empty() {
        return Err(format!("实例「{}」不存在", version_id));
    }
    if !errors.is_empty() {
        return Err(errors.join("；"));
    }

    emit_log(&app, "success", format!("实例「{}」已删除", version_id));
    Ok(())
}
