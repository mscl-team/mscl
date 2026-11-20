//! 游戏类命令：安装、启动
//!
//! 编排 core + platform 的能力，把多步骤流程封装成单个 command，
//! 进度通过 Tauri event 实时推送给前端。

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use crate::models::{
    AppSettings, GameOutputEvent, InstallProgressEvent, InstallResult, InstanceLaunchOptions,
    InstanceMetrics, LaunchGameRequest, LaunchResult, LaunchStatusEvent, ModInfo, RunningGameInfo,
    RuntimeMetrics, VerifyReportDto,
};
use crate::state::{self, AppState};

use super::isolation;
use super::instance::{emit_log, emit_update_log};

/// 获取已安装的版本列表（扫描 versions 目录）
#[tauri::command]
pub async fn get_installed_versions(app: AppHandle) -> Result<Vec<String>, String> {
    let versions_dir = app.state::<AppState>().game_root.join("versions");
    if !versions_dir.exists() {
        return Ok(Vec::new());
    }
    let mut versions = Vec::new();
    let entries = std::fs::read_dir(&versions_dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let id = entry.file_name().to_string_lossy().into_owned();
        // 只認含对应 json 的目录为已安装
        if entry.path().join(format!("{}.json", id)).exists() {
            versions.push(id);
        }
    }
    versions.sort();
    Ok(versions)
}

/// 读取本地已安装的版本元数据
fn load_installed_version(
    game_root: &std::path::Path,
    version_id: &str,
) -> Result<mscl_core::Version, String> {
    let path = game_root.join(format!("versions/{0}/{0}.json", version_id));
    let json = std::fs::read_to_string(&path)
        .map_err(|_| format!("版本 {} 尚未安装，请先安装", version_id))?;
    serde_json::from_str(&json).map_err(|e| format!("版本元数据解析失败: {}", e))
}

/// Java 检测缓存
///
/// `detect_java` 会启动 `java -version` 子进程（100~300ms），
/// 若每次点"开始游戏"都重跑，冷启动会白白多一段时间。
/// 这里缓存 5 分钟，启动与 Forge 安装共用。
static JAVA_CACHE: std::sync::OnceLock<
    std::sync::Mutex<Option<(mscl_platform::JavaRuntime, std::time::Instant)>>,
> = std::sync::OnceLock::new();

const JAVA_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

fn detect_java_cached() -> Result<mscl_platform::JavaRuntime, String> {
    let cache = JAVA_CACHE.get_or_init(|| std::sync::Mutex::new(None));

    if let Ok(guard) = cache.lock() {
        if let Some((runtime, at)) = guard.as_ref() {
            if at.elapsed() < JAVA_CACHE_TTL {
                return Ok(runtime.clone());
            }
        }
    }

    let runtime = mscl_platform::detect_java().map_err(|e| format!("{}", e))?;
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((runtime.clone(), std::time::Instant::now()));
    }
    Ok(runtime)
}

/// 读取版本并合并 inheritsFrom 继承链
///
/// Fabric / Forge 的版本 JSON 只声明自己的库与主类，其余（原版库、assets、
/// client.jar）都通过 inheritsFrom 继承。启动与校验前必须先合并。
fn load_installed_version_with_inherit(
    game_root: &std::path::Path,
    version_id: &str,
) -> Result<mscl_core::Version, String> {
    let mut current = load_installed_version(game_root, version_id)?;
    let mut merged = current.clone();
    let mut depth = 0;

    while let Some(parent_id) = current.inherits_from.clone() {
        depth += 1;
        if depth > 10 {
            return Err("版本继承链过深（超过 10 层），请检查版本文件".to_string());
        }
        let parent = load_installed_version(game_root, &parent_id)?;
        merged = mscl_core::merge_inherited(&merged, &parent);
        current = parent;
    }

    Ok(merged)
}

/// 读取本地资源索引；缺失或损坏时返回 None（报告里会把索引本身标为缺失）
fn load_local_asset_index(
    game_root: &std::path::Path,
    version: &mscl_core::Version,
) -> Option<mscl_core::AssetIndexFile> {
    let idx = version.asset_index.as_ref()?;
    let path = game_root.join(format!("assets/indexes/{}.json", idx.id));
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// 从本地版本元数据出发，下载 / 补全全部文件（安装与修复共用）
///
/// 下载引擎会先校验并跳过完好文件，所以"修复"只需再跑一次该流程。
async fn download_version_files(
    app: &AppHandle,
    version_id: &str,
    version: &mscl_core::Version,
    game_root: &std::path::Path,
    http_client: &reqwest::Client,
    settings: &AppSettings,
) -> Result<InstallResult, String> {
    let platform = mscl_platform::current_core_platform();
    let cancel_flag = app.state::<AppState>().install_cancel.clone();
    let options = mscl_core::DownloadOptions {
        concurrency: settings.download_concurrency.clamp(1, 32) as usize,
        cancel: Some(cancel_flag),
        ..Default::default()
    };
    let source = mscl_core::DownloadSource::from_id(&settings.download_source);

    // 1. 先确保资源索引存在（解析资源列表的前提）
    let asset_index_task = mscl_core::DownloadTask::plan_asset_index(version);
    if let Some(ref idx_task) = asset_index_task {
        let engine = mscl_core::DownloadEngine::new(http_client.clone(), game_root.to_path_buf())
            .with_options(options.clone())
            .with_source(source.clone());
        let idx_result = engine
            .download_all(std::slice::from_ref(idx_task), None)
            .await
            .map_err(|e| format!("下载资源索引失败: {}", e))?;
        if !idx_result.is_full_success() {
            return Err(format!("资源索引下载失败: {:?}", idx_result.errors));
        }
    }

    // 2. 解析索引并规划全部任务
    let asset_index = match &asset_index_task {
        Some(task) => {
            let idx_path = game_root.join(&task.relative_path);
            let idx_json = std::fs::read_to_string(&idx_path)
                .map_err(|e| format!("读取资源索引失败: {}", e))?;
            Some(
                serde_json::from_str::<mscl_core::AssetIndexFile>(&idx_json)
                    .map_err(|e| format!("解析资源索引失败: {}", e))?,
            )
        }
        None => None,
    };
    let tasks = mscl_core::plan_version_tasks(version, asset_index.as_ref(), platform);
    let total = tasks.len();

    // 3. 推送"开始"事件
    let _ = app.emit(
        "install-progress",
        InstallProgressEvent::Started {
            total,
            to_download: total, // 引擎内部会跳过已校验的，这里给总数即可
            version_id: version_id.to_string(),
        },
    );

    // 4. 进度回调：core 事件 → channel → 主流程转发给前端
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<InstallProgressEvent>();
    let vid = version_id.to_string();
    let progress: mscl_core::ProgressCallback = Arc::new(move |event| {
        let frontend_event = match event {
            mscl_core::DownloadEvent::Started { total, to_download } => {
                Some(InstallProgressEvent::Started {
                    total,
                    to_download,
                    version_id: vid.clone(),
                })
            }
            mscl_core::DownloadEvent::TaskDone {
                path,
                result,
                completed,
                total,
            } => Some(InstallProgressEvent::TaskDone {
                path,
                skipped: result == mscl_core::TaskResult::Skipped,
                completed,
                total,
            }),
            mscl_core::DownloadEvent::Finished { .. } => None, // 汇总在下方单独发
        };
        if let Some(e) = frontend_event {
            let _ = tx.send(e);
        }
    });

    // 5. 执行下载 + 并发转发进度事件
    let engine = mscl_core::DownloadEngine::new(http_client.clone(), game_root.to_path_buf())
        .with_options(options)
        .with_source(source);

    let app_for_forward = app.clone();
    let forward_task = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let _ = app_for_forward.emit("install-progress", event);
        }
    });

    let download_task =
        tokio::spawn(async move { engine.download_all(&tasks, Some(progress)).await });

    let result = download_task
        .await
        .map_err(|e| format!("下载任务异常: {}", e))?
        .map_err(|e| format!("下载失败: {}", e))?;
    // progress 闭包已随 download_all 结束而释放，tx 被 drop，channel 关闭，转发任务自然结束
    let _ = forward_task.await;

    // 6. 推送"完成"事件
    let _ = app.emit(
        "install-progress",
        InstallProgressEvent::Finished {
            downloaded: result.downloaded,
            skipped: result.skipped,
            failed: result.failed,
        },
    );

    let error_messages: Vec<String> = result
        .errors
        .iter()
        .map(|(path, err)| format!("{}: {}", path, err))
        .collect();

    Ok(InstallResult {
        version_id: version_id.to_string(),
        downloaded: result.downloaded,
        skipped: result.skipped,
        failed: result.failed,
        errors: error_messages,
        game_root: game_root.to_string_lossy().into_owned(),
    })
}

/// 安装游戏（拉取元数据 → 保存版本 JSON → 下载全部文件）
#[tauri::command]
pub async fn install_game(
    app: AppHandle,
    req: crate::models::InstallGameRequest,
) -> Result<InstallResult, String> {
    // 从 AppHandle 获取状态，并 clone 出 owned 值，避免 async block 捕获 State 借用
    // 独立安装（非实例安装流程内）先清掉上一次遗留的暂停/取消标记
    let (http_client, game_root, settings) = {
        let state = app.state::<AppState>();
        if !state.is_install_active() {
            state.begin_install();
        }
        let game_root = state.game_root.clone();
        let settings = AppSettings::load_or_default(&game_root.join("settings.json"));
        (state.http_client.clone(), game_root, settings)
    };
    let version_id = req.version_id.clone();
    let source = mscl_core::DownloadSource::from_id(&settings.download_source);

    // 1. 拉取清单（走所选源）并找到目标版本
    let manifest = mscl_core::fetch_manifest_from(&http_client, &source.manifest_url())
        .await
        .map_err(|e| format!("获取版本清单失败: {}", e))?;
    let summary = manifest
        .find(&version_id)
        .ok_or_else(|| format!("找不到版本: {}", version_id))?;

    // 2. 拉取版本 JSON（同样走所选源）
    let version_url = source.rewrite(&summary.url);
    let version = mscl_core::fetch_version(&http_client, &version_url)
        .await
        .map_err(|e| format!("获取版本元数据失败: {}", e))?;

    // 3. 保存版本 JSON 到本地（供启动、校验与"已安装"判定用）
    let version_json_path = game_root.join(format!("versions/{0}/{0}.json", version_id));
    if let Some(parent) = version_json_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建版本目录失败: {}", e))?;
    }
    let version_json = serde_json::to_string_pretty(&version).map_err(|e| e.to_string())?;
    std::fs::write(&version_json_path, version_json)
        .map_err(|e| format!("写入版本 JSON 失败: {}", e))?;

    // 4. 下载 / 补全所有文件
    download_version_files(&app, &version_id, &version, &game_root, &http_client, &settings).await
}

/// 安装 Fabric 加载器（基于 Fabric Meta 的 profile JSON，不执行 installer）
#[tauri::command]
pub async fn install_fabric(
    app: AppHandle,
    req: crate::models::InstallFabricRequest,
) -> Result<InstallResult, String> {
    let (http_client, game_root, settings) = {
        let state = app.state::<AppState>();
        let game_root = state.game_root.clone();
        let settings = AppSettings::load_or_default(&game_root.join("settings.json"));
        (state.http_client.clone(), game_root, settings)
    };

    // 1. 父版本必须已安装（Fabric 只是叠加在原版之上）
    let base_json = game_root.join(format!(
        "versions/{0}/{0}.json",
        req.minecraft_version
    ));
    if !base_json.exists() {
        return Err(format!(
            "请先安装原版 {} 后再安装 Fabric",
            req.minecraft_version
        ));
    }

    // 2. 未指定 loader 时取最新稳定版
    //    下载源为镜像时，Meta 查询也走 BMCLAPI 的 fabric-meta，避免直连 meta.fabricmc.net 过慢
    let meta_base = if settings.download_source == "bmclapi" {
        "https://bmclapi2.bangbang93.com/fabric-meta/v2"
    } else {
        mscl_core::FABRIC_META
    };
    let loader = match req.loader_version.as_deref().map(str::trim) {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => {
            let loaders = match mscl_core::fetch_fabric_loaders_from(
                &http_client,
                meta_base,
                &req.minecraft_version,
            )
            .await
            {
                Ok(list) => list,
                Err(e) if meta_base != mscl_core::FABRIC_META => {
                    emit_log(&app, "warn", format!("镜像查询 Fabric 版本失败，改用官方源：{}", e));
                    mscl_core::fetch_fabric_loaders(&http_client, &req.minecraft_version)
                        .await
                        .map_err(|e| format!("获取 Fabric 版本列表失败: {}", e))?
                }
                Err(e) => return Err(format!("获取 Fabric 版本列表失败: {}", e)),
            };
            loaders
                .iter()
                .find(|l| l.stable)
                .or_else(|| loaders.first())
                .map(|l| l.version.clone())
                .ok_or_else(|| {
                    format!("{} 没有可用的 Fabric loader", req.minecraft_version)
                })?
        }
    };

    // 3. 拉取 profile（含 inheritsFrom）
    let profile = match mscl_core::fetch_fabric_profile_from(
        &http_client,
        meta_base,
        &req.minecraft_version,
        &loader,
    )
    .await
    {
        Ok(profile) => profile,
        Err(e) if meta_base != mscl_core::FABRIC_META => {
            emit_log(&app, "warn", format!("镜像获取 Fabric profile 失败，改用官方源：{}", e));
            mscl_core::fetch_fabric_profile(&http_client, &req.minecraft_version, &loader)
                .await
                .map_err(|e| format!("获取 Fabric profile 失败: {}", e))?
        }
        Err(e) => return Err(format!("获取 Fabric profile 失败: {}", e)),
    };

    if profile.inherits_from.as_deref() != Some(req.minecraft_version.as_str()) {
        return Err(format!(
            "Fabric profile 的继承版本异常（期望 {}，实际 {:?}）",
            req.minecraft_version, profile.inherits_from
        ));
    }

    // 4. 保存为本地版本，供启动与版本选择使用
    let version_id = profile.id.clone();
    let version_json_path = game_root.join(format!("versions/{0}/{0}.json", version_id));
    if let Some(parent) = version_json_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建版本目录失败: {}", e))?;
    }
    let json = serde_json::to_string_pretty(&profile).map_err(|e| e.to_string())?;
    std::fs::write(&version_json_path, json)
        .map_err(|e| format!("写入 Fabric 版本 JSON 失败: {}", e))?;

    // 5. 下载 Fabric 自身依赖（原版库与资源已在父版本中）
    download_version_files(&app, &version_id, &profile, &game_root, &http_client, &settings).await
}

/// 从 Forge 官方 promotions 取推荐版本（优先 recommend，回退 latest）
async fn fetch_forge_recommended(
    client: &reqwest::Client,
    minecraft_version: &str,
) -> Result<String, String> {
    let url = "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
    let response = client
        .get(url)
        // 该站点会拦截非浏览器 UA（直接返回 403）
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
        )
        .send()
        .await
        .map_err(|e| format!("获取 Forge 版本信息失败: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("Forge 版本信息返回状态 {}", response.status()));
    }
    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("解析 Forge 版本信息失败: {}", e))?;

    // 官方字段名是 promos；兼容少数文档写成 promotions 的情况
    let promotions = json
        .get("promos")
        .or_else(|| json.get("promotions"))
        .and_then(|v| v.as_object())
        .ok_or_else(|| "Forge 版本信息格式异常".to_string())?;

    // 键名是 "{mc}-recommended" / "{mc}-latest"（官方带 d 后缀）
    let recommended = format!("{}-recommended", minecraft_version);
    let latest = format!("{}-latest", minecraft_version);
    promotions
        .get(&recommended)
        .or_else(|| promotions.get(&latest))
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| format!("{} 没有可用的 Forge 版本", minecraft_version))
}

/// 定位 Forge 安装器生成的版本目录名
fn find_forge_version(
    game_root: &std::path::Path,
    minecraft_version: &str,
    forge_version: &str,
) -> Option<String> {
    let candidates = [
        format!("{}-forge-{}", minecraft_version, forge_version),
        format!("{}-forge{}", minecraft_version, forge_version),
    ];
    for name in candidates {
        if game_root
            .join("versions")
            .join(&name)
            .join(format!("{}.json", name))
            .exists()
        {
            return Some(name);
        }
    }
    // 兜底扫描：以 "{mc}-forge" 开头且含 json 的目录
    let prefix = format!("{}-forge", minecraft_version);
    if let Ok(entries) = std::fs::read_dir(game_root.join("versions")) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&prefix)
                && entry.path().join(format!("{}.json", name)).exists()
            {
                return Some(name);
            }
        }
    }
    None
}

/// 截取字符串末尾若干字符（用于限制安装器输出长度）
fn tail_chars(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        text.to_string()
    } else {
        chars[chars.len() - max_chars..].iter().collect()
    }
}

/// 安装 Forge（下载官方 installer 并静默执行 --installClient）
#[tauri::command]
pub async fn install_forge(
    app: AppHandle,
    req: crate::models::InstallForgeRequest,
) -> Result<InstallResult, String> {
    let (http_client, game_root) = {
        let state = app.state::<AppState>();
        (state.http_client.clone(), state.game_root.clone())
    };

    // 1. 父版本必须已安装
    let base_json = game_root.join(format!(
        "versions/{0}/{0}.json",
        req.minecraft_version
    ));
    if !base_json.exists() {
        return Err(format!(
            "请先安装原版 {} 后再安装 Forge",
            req.minecraft_version
        ));
    }

    // 2. 确定 Forge 版本
    let forge_version = match req.forge_version.as_deref().map(str::trim) {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => fetch_forge_recommended(&http_client, &req.minecraft_version).await?,
    };

    // 3. 下载 installer
    let installer_url = format!(
        "https://maven.minecraftforge.net/net/minecraftforge/forge/{0}-{1}/forge-{0}-{1}-installer.jar",
        req.minecraft_version, forge_version
    );
    let installer_bytes = http_client
        .get(&installer_url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
        )
        .send()
        .await
        .map_err(|e| format!("下载 Forge 安装器失败: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("读取 Forge 安装器失败: {}", e))?;
    let installer_path = std::env::temp_dir().join(format!(
        "mscl-forge-{}-{}.jar",
        req.minecraft_version, forge_version
    ));
    std::fs::write(&installer_path, &installer_bytes)
        .map_err(|e| format!("写入 Forge 安装器失败: {}", e))?;

    // 3.5 Forge 安装器要求存在 launcher_profiles.json（官方启动器产物），缺失会直接报错
    let profiles_path = game_root.join("launcher_profiles.json");
    if !profiles_path.exists() {
        let minimal = serde_json::json!({
            "profiles": {},
            "settings": {},
            "version": 3
        });
        std::fs::write(
            &profiles_path,
            serde_json::to_string_pretty(&minimal).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("创建 launcher_profiles.json 失败: {}", e))?;
    }

    // 4. 用检测到的 Java 静默执行
    let java = detect_java_cached()
        .map_err(|e| format!("未找到可用的 Java，无法运行 Forge 安装器: {}", e))?;

    let mut command = tokio::process::Command::new(&java.path);
    command
        .arg("-jar")
        .arg(&installer_path)
        .arg("--installClient")
        .arg(game_root.as_os_str());
    #[cfg(windows)]
    {
        // tokio::process::Command 自带 creation_flags（无需额外 trait）
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(600),
        command.output(),
    )
    .await
    .map_err(|_| "Forge 安装器执行超时（10 分钟）".to_string())?
    .map_err(|e| format!("执行 Forge 安装器失败: {}", e))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = std::fs::remove_file(&installer_path);
        return Err(format!(
            "Forge 安装器退出码 {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            output.status.code(),
            tail_chars(&stdout, 1500),
            tail_chars(&stderr, 1500)
        ));
    }

    let _ = std::fs::remove_file(&installer_path);

    // 5. 定位生成的版本
    let version_id = find_forge_version(
        &game_root,
        &req.minecraft_version,
        &forge_version,
    )
    .ok_or_else(|| "Forge 安装器未生成版本目录，请检查安装日志".to_string())?;

    Ok(InstallResult {
        version_id,
        downloaded: 0,
        skipped: 0,
        failed: 0,
        errors: Vec::new(),
        game_root: game_root.to_string_lossy().into_owned(),
    })
}

/// 修复已安装版本：重新校验并只补下缺失 / 损坏的文件
#[tauri::command]
pub async fn repair_version(app: AppHandle, version_id: String) -> Result<InstallResult, String> {
    let (http_client, game_root, settings) = {
        let state = app.state::<AppState>();
        // 独立修复时清掉遗留的暂停/取消标记，避免刚修就停
        if !state.is_install_active() {
            state.begin_install();
        }
        let game_root = state.game_root.clone();
        let settings = AppSettings::load_or_default(&game_root.join("settings.json"));
        (state.http_client.clone(), game_root, settings)
    };

    // 用合并后的版本（含继承链）规划任务：Fabric/Forge 实例的父版本库与资源
    // 也必须一起补齐，否则修复完再校验依旧会报缺失
    let version = load_installed_version_with_inherit(&game_root, &version_id)?;
    emit_update_log(&app, "info", format!("开始修复版本 {}…", version_id));
    let result =
        download_version_files(&app, &version_id, &version, &game_root, &http_client, &settings)
            .await?;
    emit_update_log(
        &app,
        "success",
        format!(
            "版本 {} 修复完成：下载 {}，跳过 {}，失败 {}",
            version_id, result.downloaded, result.skipped, result.failed
        ),
    );
    Ok(result)
}

/// 校验已安装版本的文件完整性
#[tauri::command]
pub async fn verify_version(app: AppHandle, version_id: String) -> Result<VerifyReportDto, String> {
    let game_root = app.state::<AppState>().game_root.clone();
    let version = load_installed_version_with_inherit(&game_root, &version_id)?;
    let asset_index = load_local_asset_index(&game_root, &version);

    let tasks = mscl_core::plan_version_tasks(
        &version,
        asset_index.as_ref(),
        mscl_platform::current_core_platform(),
    );

    // SHA1 计算是阻塞 IO，放到阻塞线程池，避免卡住 async 运行时
    let root = game_root.clone();
    let total = tasks.len();
    emit_update_log(&app, "info", format!("开始校验版本 {}（{} 个文件）…", version_id, total));
    let report = tokio::task::spawn_blocking(move || mscl_core::verify_tasks(&root, &tasks))
        .await
        .map_err(|e| format!("校验任务异常: {}", e))?
        .map_err(|e| format!("校验失败: {}", e))?;

    emit_update_log(
        &app,
        if report.corrupted > 0 || report.missing > 0 {
            "warn"
        } else {
            "success"
        },
        format!(
            "版本 {} 校验完成：正常 {}，缺失 {}，损坏 {}",
            version_id, report.valid, report.missing, report.corrupted
        ),
    );

    Ok(report.into())
}

/// 启动游戏
#[tauri::command]
pub async fn launch_game(
    app: AppHandle,
    req: LaunchGameRequest,
) -> Result<LaunchResult, String> {
    // 启动耗时基准：各阶段写 info 日志，便于定位瓶颈
    let started = std::time::Instant::now();

    // 提前 clone game_root，避免跨 await 持有 state 借用
    let game_root = match req.game_root.as_deref() {
        Some(path) if state::is_known_game_root(std::path::Path::new(path)) => {
            std::path::PathBuf::from(path)
        }
        Some(_) => return Err("该游戏目录尚未在启动器中注册".to_string()),
        None => app.state::<AppState>().game_root.clone(),
    };
    let settings = AppSettings::load_or_default(&game_root.join("settings.json"));

    // 实例级自定义启动参数
    let launch_options = std::fs::read_to_string(
        game_root
            .join("instances")
            .join(&req.version_id)
            .join("launch.json"),
    )
    .ok()
    .and_then(|text| serde_json::from_str::<InstanceLaunchOptions>(&text).ok())
    .unwrap_or_default();

    // 1. 读取本地版本 JSON（含继承链合并，支持 Fabric/Forge 版本）
    let version = load_installed_version_with_inherit(&game_root, &req.version_id)?;
    tracing::info!(
        elapsed_ms = started.elapsed().as_millis(),
        "启动准备：版本元数据就绪"
    );

    let _ = app.emit("launch-status", LaunchStatusEvent::Preparing {
        message: "检测 Java 运行时...".to_string(),
    });

    // 实例级验证覆盖：instance.json 里的 verifyGameOnLaunch 优先于全局设置
    let instance_meta_json = std::fs::read_to_string(
        game_root
            .join("instances")
            .join(&req.version_id)
            .join("instance.json"),
    )
    .ok()
    .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());

    let verify_game_on_launch = instance_meta_json
        .as_ref()
        .and_then(|json| json.get("verifyGameOnLaunch").and_then(|v| v.as_bool()))
        .unwrap_or(settings.verify_game_on_launch);

    let verify_resources_on_launch = instance_meta_json
        .as_ref()
        .and_then(|json| json.get("verifyResourcesOnLaunch").and_then(|v| v.as_bool()))
        .unwrap_or(settings.verify_resources_on_launch);

    // 启动前完整性检查（设置可关闭）：发现缺失/损坏先自动补全，补不齐才拦下
    if verify_game_on_launch {
        let _ = app.emit("launch-status", LaunchStatusEvent::Preparing {
            message: "校验游戏文件完整性…".to_string(),
        });
        let report = verify_version(app.clone(), req.version_id.clone()).await?;
        if report.missing > 0 || report.corrupted > 0 {
            let _ = app.emit("launch-status", LaunchStatusEvent::Preparing {
                message: format!(
                    "检测到缺失 {} 个 / 损坏 {} 个文件，正在自动补全…",
                    report.missing, report.corrupted
                ),
            });
            emit_update_log(
                &app,
                "warn",
                format!(
                    "启动前校验：缺失 {} 个、损坏 {} 个（约 {:.1} MB），正在自动补全…",
                    report.missing,
                    report.corrupted,
                    report.bytes_to_download as f64 / 1024.0 / 1024.0
                ),
            );

            let repaired = repair_version(app.clone(), req.version_id.clone())
                .await
                .map_err(|e| {
                    format!(
                        "自动补全失败：{}。请检查网络后在「版本」页手动修复",
                        e
                    )
                })?;
            if repaired.failed > 0 {
                return Err(format!(
                    "自动补全未完成：{} 个文件下载失败，请检查网络后重试",
                    repaired.failed
                ));
            }

            // 补全后再校验一次，确认真的齐了
            let after = verify_version(app.clone(), req.version_id.clone()).await?;
            if after.missing > 0 || after.corrupted > 0 {
                return Err(format!(
                    "自动补全后仍有 {} 个文件缺失、{} 个损坏。\
                     请到「版本」页手动修复，或在设置里关闭「启动前校验游戏完整性」",
                    after.missing, after.corrupted
                ));
            }
            emit_update_log(&app, "success", "自动补全完成，继续启动");
            tracing::info!(
                elapsed_ms = started.elapsed().as_millis(),
                "启动准备：缺失文件已自动补全"
            );
        }
        tracing::info!(
            elapsed_ms = started.elapsed().as_millis(),
            total = report.total,
            "启动准备：完整性校验通过"
        );
    }

    // 启动前资源校验：检查实例资源清单里的文件是否还在
    if verify_resources_on_launch {
        let _ = app.emit("launch-status", LaunchStatusEvent::Preparing {
            message: "校验实例资源完整性…".to_string(),
        });

        let instance_dir = game_root.join("instances").join(&req.version_id);
        if let Ok(text) = std::fs::read_to_string(instance_dir.join(".mscl-resources.json")) {
            if let Ok(entries) = serde_json::from_str::<Vec<serde_json::Value>>(&text) {
                let mut missing = Vec::new();
                for entry in entries {
                    let kind = entry
                        .get("kind")
                        .and_then(|v| v.as_str())
                        .unwrap_or("mod");
                    let file = entry
                        .get("file_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if file.is_empty() {
                        continue;
                    }
                    let subdir = match kind {
                        "shader" | "shaderpacks" => "shaderpacks",
                        "resourcepack" | "resourcepacks" => "resourcepacks",
                        "datapack" | "datapacks" => "datapacks",
                        _ => "mods",
                    };
                    if !instance_dir.join(subdir).join(file).exists() {
                        missing.push(format!("{}/{}", subdir, file));
                    }
                }
                if !missing.is_empty() {
                    return Err(format!(
                        "资源校验失败：{} 个文件缺失（{}）。请到资源中心重新安装，或在实例编辑里把「启动前校验资源」设为跳过",
                        missing.len(),
                        missing.iter().take(3).cloned().collect::<Vec<_>>().join("、")
                    ));
                }
                tracing::info!(
                    elapsed_ms = started.elapsed().as_millis(),
                    "启动准备：资源校验通过"
                );
            }
        }
    }

    // 2. 检测 Java
    let java = detect_java_cached()?;
    tracing::info!(
        elapsed_ms = started.elapsed().as_millis(),
        java = %java.path.display(),
        "启动准备：Java 就绪"
    );

    // 检查 Java 版本是否满足要求
    if let (Some(major), Some(required)) = (java.major_version, version.java_version.as_ref()) {
        if major < required.major_version {
            return Err(format!(
                "Java 版本不足: 需要 Java {}, 当前为 Java {}",
                required.major_version, major
            ));
        }
    }

    // 3. 构建配置
    let platform = mscl_platform::current_core_platform();
    let assets_root = game_root.join("assets");
    let natives_dir = game_root.join(format!("versions/{}/natives", req.version_id));
    std::fs::create_dir_all(&natives_dir).map_err(|e| e.to_string())?;

    // 版本隔离：按勾选的资源类型建立实例目录 / 共享链接
    let game_dir = if settings.version_isolation {
        let dir = game_root.join("instances").join(&req.version_id);
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建实例目录失败: {}", e))?;
        for warning in isolation::ensure_instance_links(&game_root, &req.version_id, &settings) {
            emit_log(&app, "warn", format!("版本隔离：{}", warning));
        }
        Some(dir)
    } else {
        None
    };

    // 3.1 解压 native 库（老版本必须，现代版本列表为空自动跳过）
    let native_jars: Vec<std::path::PathBuf> = mscl_core::plan_native_jars(&version, platform)
        .into_iter()
        .map(|p| game_root.join(p))
        .collect();
    let extensions = mscl_core::native_library_extensions(platform);
    let _ = app.emit("launch-status", LaunchStatusEvent::Preparing {
        message: format!("准备原生库（{} 个）...", native_jars.len()),
    });
    let extracted = mscl_platform::extract_native_jars(&native_jars, &natives_dir, extensions)
        .map_err(|e| format!("解压原生库失败: {}", e))?;
    tracing::info!(
        elapsed_ms = started.elapsed().as_millis(),
        jars = native_jars.len(),
        extracted,
        "启动准备：原生库完成"
    );
    // 把阶段耗时直接反馈到界面：跳过期能明显看出优化效果
    let _ = app.emit("launch-status", LaunchStatusEvent::Preparing {
        message: if extracted == 0 && !native_jars.is_empty() {
            format!(
                "原生库已是缓存，跳过解压（累计 {}ms）",
                started.elapsed().as_millis()
            )
        } else {
            format!(
                "原生库就绪：解压 {} 个（累计 {}ms）",
                extracted,
                started.elapsed().as_millis()
            )
        },
    });

    // 内存：请求值 > 实例启动配置 > 自动（取物理内存 1/4，最少 1GB，最多 8GB）
    let requested_memory = if req.max_memory_mb > 0 {
        req.max_memory_mb
    } else {
        launch_options.max_memory_mb.unwrap_or(0)
    };
    let max_memory = if requested_memory == 0 {
        auto_memory_mb()
    } else {
        requested_memory
    };

    let config = mscl_core::LaunchConfig {
        java_path: java.path.clone(),
        game_root: game_root.clone(),
        game_dir,
        assets_root,
        natives_dir,
        max_memory_mb: max_memory,
        window_width: 1280,
        window_height: 720,
        launcher_name: "MSCL".to_string(),
        launcher_version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let account = mscl_core::Account::offline(&req.username);
    let command = mscl_core::build_launch_command(&version, &config, &account, platform)
        .map_err(|e| format!("构建启动命令失败: {}", e))?;

    let command_line = command.to_command_line();
    let _ = app.emit("launch-status", LaunchStatusEvent::Generated {
        command_line: command_line.clone(),
    });
    tracing::info!(
        elapsed_ms = started.elapsed().as_millis(),
        "启动准备：启动命令构建完成"
    );

    // 4. 启动进程
    let app_for_output = app.clone();
    let output_callback: mscl_platform::OutputCallback = Arc::new(move |out| {
        let (stream, line) = match out {
            mscl_platform::ProcessOutput::Stdout(s) => ("stdout", s),
            mscl_platform::ProcessOutput::Stderr(s) => ("stderr", s),
        };
        let _ = app_for_output.emit("game-output", GameOutputEvent {
            stream: stream.to_string(),
            noise: is_offline_noise(&line),
            line,
        });
    });

    // 附加用户自定义启动参数：请求覆盖 > 实例 launch.json > 全局设置
    let jvm_extra = req
        .jvm_args
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let value = launch_options.jvm_args.trim();
            if value.is_empty() { None } else { Some(value) }
        })
        .unwrap_or(settings.extra_jvm_args.trim());
    let game_extra = req
        .game_args
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let value = launch_options.game_args.trim();
            if value.is_empty() { None } else { Some(value) }
        })
        .unwrap_or(settings.extra_game_args.trim());

    let mut launch_args = command.args.clone();
    if !jvm_extra.is_empty() {
        let extras: Vec<String> = jvm_extra.split_whitespace().map(String::from).collect();
        launch_args.splice(0..0, extras);
    }
    if !game_extra.is_empty() {
        let extras: Vec<String> = game_extra.split_whitespace().map(String::from).collect();
        launch_args.extend(extras);
    }

    let process = mscl_platform::GameProcess::spawn(
        &command.executable,
        &launch_args,
        &command.working_dir,
        Some(output_callback),
    ).map_err(|e| format!("{}", e))?;

    let pid = process.id();
    let started_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    let run_id = format!("{}-{}-{}", req.version_id, pid, started_at);
    let game_root_text = game_root.to_string_lossy().into_owned();
    app.state::<AppState>().add_running(RunningGameInfo {
        run_id: run_id.clone(),
        version_id: req.version_id.clone(),
        pid,
        started_at,
        game_root: game_root_text.clone(),
    });
    tracing::info!(
        elapsed_ms = started.elapsed().as_millis(),
        pid,
        run_id = %run_id,
        "启动准备：游戏进程已拉起"
    );
    let _ = app.emit("launch-status", LaunchStatusEvent::Started { pid });

    // 5. 开线程等待进程退出，推送退出事件
    // 注意：不能把 tauri::State 借用移到线程，用 app.state() 在线程内重新获取
    let app_for_exit = app.clone();
    let run_id_for_exit = run_id.clone();
    std::thread::spawn(move || {
        let exit_code = process.wait().unwrap_or(-1);
        let st = app_for_exit.state::<AppState>();
        st.remove_running(&run_id_for_exit);
        // 多开时只有最后一个实例退出才触发全局退出事件，避免首页误判
        if st.list_running().is_empty() {
            let _ = app_for_exit.emit("launch-status", LaunchStatusEvent::Exited { code: exit_code });
        }
    });

    Ok(LaunchResult {
        pid,
        version_id: req.version_id,
        run_id,
        game_root: game_root_text,
        command_line,
    })
}

fn resolve_game_root(app: &AppHandle, requested: Option<&str>) -> Result<std::path::PathBuf, String> {
    match requested {
        Some(path) if state::is_known_game_root(std::path::Path::new(path)) => {
            Ok(std::path::PathBuf::from(path))
        }
        Some(_) => Err("该游戏目录尚未在启动器中注册".to_string()),
        None => Ok(app.state::<AppState>().game_root.clone()),
    }
}

fn launch_options_path(
    game_root: &std::path::Path,
    version_id: &str,
) -> Result<std::path::PathBuf, String> {
    if !is_safe_version_id(version_id) {
        return Err("非法的实例 ID".to_string());
    }
    Ok(game_root
        .join("instances")
        .join(version_id)
        .join("launch.json"))
}

/// 读取实例自定义启动参数
#[tauri::command]
pub async fn get_instance_launch_options(
    app: AppHandle,
    version_id: String,
    game_root: Option<String>,
) -> Result<InstanceLaunchOptions, String> {
    let root = resolve_game_root(&app, game_root.as_deref())?;
    let path = launch_options_path(&root, &version_id)?;
    Ok(std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default())
}

/// 保存实例自定义启动参数
#[tauri::command]
pub async fn save_instance_launch_options(
    app: AppHandle,
    version_id: String,
    game_root: Option<String>,
    options: InstanceLaunchOptions,
) -> Result<(), String> {
    if options.max_memory_mb.unwrap_or(0) > 65536 {
        return Err("内存分配不能超过 65536 MB".to_string());
    }
    let root = resolve_game_root(&app, game_root.as_deref())?;
    let path = launch_options_path(&root, &version_id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建实例目录失败: {}", e))?;
    }
    let text = serde_json::to_string_pretty(&options).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("保存启动配置失败: {}", e))
}

/// 离线模式下常见的无害报错：认证服务不可用、皮肤拉取失败、Realms JWT 等。
/// 只标记为噪音（不丢弃），前端可选择在"详细日志"中查看原文。
fn is_offline_noise(line: &str) -> bool {
    let lower = line.to_lowercase();
    const PATTERNS: &[&str] = &[
        "failed to fetch user properties",
        "unable to authenticate",
        "authentication failed",
        "invalid token",
        "401 unauthorized",
        "realmstoken",
        "failed to authenticate",
        "could not authenticate",
        "skins restorer",
        // 离线/外置登录时 Mojang Realms 用占位 token 解析 JWT 的无害报错，
        // 堆栈形如：Failed to parse into SignedJWT: '00000000000000000000000000000000'
        "signedjwt",
        "realmsclient",
    ];
    PATTERNS.iter().any(|p| lower.contains(p))
}

/// 运行中的实例列表（多开时同一版本可能出现多条）
#[tauri::command]
pub async fn get_running_instances(app: AppHandle) -> Result<Vec<RunningGameInfo>, String> {
    Ok(app.state::<AppState>().list_running())
}

/// 实时资源占用：每个运行实例的 CPU / 内存 + 系统总量
#[tauri::command]
pub async fn get_runtime_metrics(app: AppHandle) -> Result<RuntimeMetrics, String> {
    use sysinfo::{Pid, ProcessesToUpdate};

    let running = app.state::<AppState>().list_running();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);

    if running.is_empty() {
        let state = app.state::<AppState>();
        let mut system = state.system.lock().map_err(|e| e.to_string())?;
        system.refresh_memory();
        system.refresh_cpu_usage();
        return Ok(RuntimeMetrics {
            timestamp,
            system_cpu_percent: system.global_cpu_usage(),
            system_memory_used: system.used_memory(),
            system_memory_total: system.total_memory(),
            instances: Vec::new(),
        });
    }

    let pids: Vec<Pid> = running
        .iter()
        .map(|item| Pid::from_u32(item.pid))
        .collect();
    let state = app.state::<AppState>();
    let mut system = state.system.lock().map_err(|e| e.to_string())?;
    system.refresh_processes(ProcessesToUpdate::Some(&pids), true);
    system.refresh_memory();
    system.refresh_cpu_usage();

    let instances = running
        .into_iter()
        .filter_map(|run| {
            let process = system.process(Pid::from_u32(run.pid))?;
            Some(InstanceMetrics {
                run_id: run.run_id,
                version_id: run.version_id,
                pid: run.pid,
                game_root: run.game_root,
                cpu_percent: process.cpu_usage(),
                memory_bytes: process.memory(),
                uptime_secs: process.run_time(),
            })
        })
        .collect();

    Ok(RuntimeMetrics {
        timestamp,
        system_cpu_percent: system.global_cpu_usage(),
        system_memory_used: system.used_memory(),
        system_memory_total: system.total_memory(),
        instances,
    })
}

/// 停止指定运行实例（按 run_id）
#[tauri::command]
pub async fn stop_run(app: AppHandle, run_id: String) -> Result<(), String> {
    let instance = app
        .state::<AppState>()
        .remove_running(&run_id)
        .ok_or_else(|| "该实例已不在运行".to_string())?;
    mscl_platform::kill_process_by_pid(instance.pid)
        .map_err(|e| format!("终止游戏进程失败: {}", e))?;
    if app.state::<AppState>().list_running().is_empty() {
        let _ = app.emit("launch-status", LaunchStatusEvent::Exited { code: -1 });
    }
    Ok(())
}

/// 停止某个版本的全部运行实例
#[tauri::command]
pub async fn stop_version(app: AppHandle, version_id: String) -> Result<usize, String> {
    let instances = app.state::<AppState>().remove_running_by_version(&version_id);
    let mut stopped = 0;
    for instance in instances {
        if mscl_platform::kill_process_by_pid(instance.pid).is_ok() {
            stopped += 1;
        }
    }
    if app.state::<AppState>().list_running().is_empty() {
        let _ = app.emit("launch-status", LaunchStatusEvent::Exited { code: -1 });
    }
    Ok(stopped)
}

/// 停止全部运行中的游戏（按 PID 终止进程树）
#[tauri::command]
pub async fn stop_game(app: AppHandle) -> Result<(), String> {
    let instances = app.state::<AppState>().clear_running();
    if instances.is_empty() {
        return Err("当前没有正在运行的游戏".to_string());
    }
    for instance in instances {
        let _ = mscl_platform::kill_process_by_pid(instance.pid);
    }
    let _ = app.emit("launch-status", LaunchStatusEvent::Exited { code: -1 });
    Ok(())
}

/// 清空运行态（前端收到进程退出事件后的兜底同步）
#[tauri::command]
pub async fn clear_game_pid(app: AppHandle) -> Result<(), String> {
    app.state::<AppState>().clear_running();
    Ok(())
}

/// 校验版本 id 安全性，防止路径穿越（卸载时删除目录）
fn is_safe_version_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains('/')
        && !id.contains('\\')
        && !id.contains("..")
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}

/// 卸载指定版本（删除 versions/{id} 目录）
#[tauri::command]
pub async fn uninstall_version(app: AppHandle, version_id: String) -> Result<(), String> {
    if !is_safe_version_id(&version_id) {
        return Err("非法的版本 ID".to_string());
    }

    let game_root = app.state::<AppState>().game_root.clone();
    let version_dir = game_root.join("versions").join(&version_id);
    if !version_dir.exists() {
        return Err(format!("版本 {} 未安装", version_id));
    }

    std::fs::remove_dir_all(&version_dir)
        .map_err(|e| format!("卸载版本 {} 失败: {}", version_id, e))?;

    Ok(())
}

/// Mod 目录：隔离模式为 instances/{id}/mods，否则为共享根下的 mods
fn mods_dir(game_root: &std::path::Path, version_id: &str, isolated: bool) -> std::path::PathBuf {
    if isolated {
        game_root.join("instances").join(version_id).join("mods")
    } else {
        game_root.join("mods")
    }
}

/// 校验文件名安全性（只允许纯文件名，禁止路径穿越）
fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\\') && !name.contains("..")
}

/// 读取设置与共享根（多处复用）
fn load_root_and_settings(app: &AppHandle) -> (std::path::PathBuf, AppSettings) {
    let state = app.state::<AppState>();
    let game_root = state.game_root.clone();
    let settings = AppSettings::load_or_default(&game_root.join("settings.json"));
    (game_root, settings)
}

/// 列出某个版本的 Mod（支持 .jar 与 .jar.disabled 两种状态）
#[tauri::command]
pub async fn list_mods(app: AppHandle, version_id: String) -> Result<Vec<ModInfo>, String> {
    let (game_root, settings) = load_root_and_settings(&app);
    let dir = mods_dir(&game_root, &version_id, settings.version_isolation);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut mods = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("读取 Mod 目录失败: {}", e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let raw_name = entry.file_name().to_string_lossy().into_owned();
        let (file_name, enabled) = if let Some(base) = raw_name.strip_suffix(".jar.disabled") {
            (format!("{}.jar", base), false)
        } else if raw_name.ends_with(".jar") {
            (raw_name, true)
        } else {
            continue;
        };
        mods.push(ModInfo {
            file_name,
            enabled,
            size: entry.metadata().map(|m| m.len()).unwrap_or(0),
        });
    }
    mods.sort_by_key(|m| m.file_name.to_lowercase());
    Ok(mods)
}

/// 启用 / 禁用 Mod（重命名 .jar ↔ .jar.disabled）
#[tauri::command]
pub async fn toggle_mod(
    app: AppHandle,
    version_id: String,
    file_name: String,
    enabled: bool,
) -> Result<(), String> {
    if !is_safe_file_name(&file_name) || !file_name.ends_with(".jar") {
        return Err("非法的 Mod 文件名".to_string());
    }
    let (game_root, settings) = load_root_and_settings(&app);
    let dir = mods_dir(&game_root, &version_id, settings.version_isolation);

    let (from, to) = if enabled {
        (
            dir.join(format!("{}.disabled", file_name)),
            dir.join(&file_name),
        )
    } else {
        (
            dir.join(&file_name),
            dir.join(format!("{}.disabled", file_name)),
        )
    };

    if !from.exists() {
        return Err(format!("Mod 文件不存在: {}", file_name));
    }
    std::fs::rename(&from, &to).map_err(|e| format!("切换 Mod 状态失败: {}", e))?;
    Ok(())
}

/// 删除 Mod 文件
#[tauri::command]
pub async fn delete_mod(
    app: AppHandle,
    version_id: String,
    file_name: String,
) -> Result<(), String> {
    if !is_safe_file_name(&file_name) || !file_name.ends_with(".jar") {
        return Err("非法的 Mod 文件名".to_string());
    }
    let (game_root, settings) = load_root_and_settings(&app);
    let dir = mods_dir(&game_root, &version_id, settings.version_isolation);
    let target = dir.join(&file_name);
    if !target.exists() {
        return Err(format!("Mod 文件不存在: {}", file_name));
    }
    std::fs::remove_file(&target).map_err(|e| format!("删除 Mod 失败: {}", e))?;
    Ok(())
}

/// 在系统文件管理器中打开实例目录（隔离模式）或共享根目录
#[tauri::command]
pub async fn open_instance_dir(app: AppHandle, version_id: String) -> Result<(), String> {
    let (game_root, settings) = load_root_and_settings(&app);
    let dir = if settings.version_isolation {
        game_root.join("instances").join(&version_id)
    } else {
        game_root.clone()
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;
    mscl_platform::open_in_file_manager(&dir).map_err(|e| format!("{}", e))
}

/// 自动内存：默认 2048MB（未来可读取系统物理内存按 1/4 动态分配）
fn auto_memory_mb() -> u32 {
    2048
}

#[cfg(test)]
mod tests {
    use super::{is_offline_noise, is_safe_version_id};

    #[test]
    fn version_id_rejects_path_traversal() {
        assert!(!is_safe_version_id(""));
        assert!(!is_safe_version_id("../etc"));
        assert!(!is_safe_version_id("a/b"));
        assert!(!is_safe_version_id("a\\b"));
        assert!(!is_safe_version_id(".."));
    }

    #[test]
    fn version_id_accepts_minecraft_ids() {
        assert!(is_safe_version_id("1.21.11"));
        assert!(is_safe_version_id("26.1-snapshot"));
        assert!(is_safe_version_id("1_8_9"));
    }

    #[test]
    fn offline_noise_is_detected() {
        assert!(is_offline_noise(
            "[main/WARN]: Failed to fetch user properties for Player"
        ));
        assert!(is_offline_noise(
            "com.mojang.authlib.exceptions.AuthenticationException: 401 Unauthorized"
        ));
        assert!(is_offline_noise("Failed to authenticate with RealmsToken"));
        // 离线/外置登录时 Realms 解析占位 JWT 的堆栈（截图报错）
        assert!(is_offline_noise(
            "Failed to parse into SignedJWT: '00000000000000000000000000000000'"
        ));
        assert!(is_offline_noise(
            "at knot//com.mojang.realmsclient.client.RealmsClient.execute(RealmsClient.java:625)"
        ));
        // 真正的崩溃不能误判为噪音
        assert!(!is_offline_noise(
            "[main/FATAL]: java.lang.OutOfMemoryError: Java heap space"
        ));
        assert!(!is_offline_noise("Loaded 123 mods"));
    }
}
