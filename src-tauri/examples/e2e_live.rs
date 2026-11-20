//! 端到端联调验证程序（阶段 7）
//!
//! 独立于 Tauri GUI，直接调用 core + platform 的完整链路：
//!   1. 拉取版本清单，定位最新正式版
//!   2. 拉取版本 JSON
//!   3. 下载 assetIndex
//!   4. 规划 client + logging + libraries + assets
//!   5. 并发下载 + SHA1 校验（带进度输出）
//!   6. 解压 native 库
//!   7. 构建启动命令，检查占位符与 classpath
//!
//! 运行：cargo run --example e2e_live --features custom-protocol
//! 数据目录：%APPDATA%/.mscl（与启动器一致）

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use mscl_core::{
    asset_object_path, build_launch_command, fetch_manifest, fetch_version,
    native_library_extensions, plan_all_libraries, plan_assets, plan_native_jars, Account,
    AssetIndexFile, DownloadEngine, DownloadEvent, DownloadOptions, DownloadTask, LaunchConfig,
    TargetPlatform, VerifyOutcome,
};

fn human_mb(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();

    // 数据目录与启动器保持一致
    let game_root = mscl_platform::default_data_dir()?;
    println!("[e2e] 游戏数据目录: {}", game_root.display());
    std::fs::create_dir_all(&game_root)?;

    let platform = mscl_platform::current_core_platform();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;

    // ---- 1. 版本清单 ----
    println!("[e2e] 1/7 拉取版本清单...");
    let manifest = fetch_manifest(&client).await?;
    let version_id = manifest
        .latest_release()
        .ok_or("找不到最新正式版")?
        .id
        .clone();
    println!("[e2e] 最新正式版: {}", version_id);

    // ---- 2. 版本 JSON ----
    println!("[e2e] 2/7 拉取版本元数据...");
    let summary = manifest.find(&version_id).ok_or("版本不在清单中")?;
    let version = fetch_version(&client, &summary.url).await?;
    println!(
        "[e2e] Java 要求: {:?}, 主类: {}, 库: {}",
        version.java_version.as_ref().map(|j| j.major_version),
        version.main_class,
        version.libraries.len()
    );

    // 保存版本 JSON（启动器启动时需要）
    let version_dir = game_root.join(format!("versions/{0}", version_id));
    std::fs::create_dir_all(&version_dir)?;
    let version_json = serde_json::to_string_pretty(&version)?;
    std::fs::write(version_dir.join(format!("{}.json", version_id)), version_json)?;

    // ---- 3. assetIndex ----
    println!("[e2e] 3/7 下载资源索引...");
    let asset_index_task = DownloadTask::plan_asset_index(&version).ok_or("版本无 assetIndex")?;
    let engine = DownloadEngine::new(client.clone(), game_root.clone());
    let idx_result = engine
        .download_all(std::slice::from_ref(&asset_index_task), None)
        .await?;
    if !idx_result.is_full_success() {
        return Err(format!("资源索引下载失败: {:?}", idx_result.errors).into());
    }
    let idx_path = game_root.join(&asset_index_task.relative_path);
    let idx_json = std::fs::read_to_string(&idx_path)?;
    let asset_index: AssetIndexFile = serde_json::from_str(&idx_json)?;
    println!("[e2e] 资源对象数: {}", asset_index.objects.len());

    // ---- 4. 规划任务 ----
    println!("[e2e] 4/7 规划下载任务...");
    let mut tasks: Vec<DownloadTask> = Vec::new();
    if let Some(t) = DownloadTask::plan_client(&version) {
        tasks.push(t);
    }
    if let Some(t) = DownloadTask::plan_logging(&version) {
        tasks.push(t);
    }
    tasks.extend(plan_all_libraries(&version, platform));
    tasks.extend(plan_assets(&asset_index));

    let total_bytes: u64 = tasks.iter().map(|t| t.size).sum();
    println!(
        "[e2e] 任务总数: {} (预计 {:.1} MB)",
        tasks.len(),
        human_mb(total_bytes)
    );

    // ---- 5. 下载 ----
    println!("[e2e] 5/7 开始下载（并发 8，带 SHA1 校验）...");
    let options = DownloadOptions {
        concurrency: 8,
        max_retries: 3,
        retry_base_delay_ms: 500,
        ..Default::default()
    };
    let engine = DownloadEngine::new(client.clone(), game_root.clone()).with_options(options);
    let last_pct = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let lp = last_pct.clone();
    let progress: mscl_core::ProgressCallback = Arc::new(move |event| {
        if let DownloadEvent::TaskDone { completed, total, .. } = event {
            let pct: u64 = ((completed * 100 / total.max(1)) / 5 * 5) as u64;
            let prev = lp.swap(pct, std::sync::atomic::Ordering::Relaxed);
            if pct != prev || completed == total {
                println!("[e2e]   进度 {}/{} ({}%)", completed, total, pct);
            }
        }
    });
    let result = engine.download_all(&tasks, Some(progress)).await?;
    println!(
        "[e2e] 下载完成: 下载 {} / 跳过 {} / 失败 {}",
        result.downloaded, result.skipped, result.failed
    );
    if !result.errors.is_empty() {
        println!("[e2e] 错误明细（前5条）:");
        for (path, err) in result.errors.iter().take(5) {
            println!("[e2e]   {} -> {}", path, err);
        }
    }
    if !result.is_full_success() {
        return Err("下载未全部成功".into());
    }

    // 抽查若干资源的 SHA1 落盘正确性
    let step = (tasks.len() / 8).max(1);
    let mut checked = 0;
    for task in tasks.iter().step_by(step).take(8) {
        let path = game_root.join(&task.relative_path);
        let outcome = mscl_core::verify_file(&path, &task.sha1, task.size)?;
        if outcome != VerifyOutcome::Valid {
            return Err(format!("校验失败: {}", task.relative_path).into());
        }
        checked += 1;
    }
    println!("[e2e] 抽查 {} 个文件 SHA1 校验通过", checked);
    if let Some(asset_task) = tasks.iter().find(|t| t.kind == mscl_core::DownloadKind::Asset) {
        let expected = asset_object_path(&asset_task.sha1);
        assert_eq!(asset_task.relative_path, expected);
        println!("[e2e] 资源路径规则正确: {}", expected);
    }

    // ---- 6. 解压 natives ----
    println!("[e2e] 6/7 解压 native 库...");
    let native_jars: Vec<PathBuf> = plan_native_jars(&version, platform)
        .into_iter()
        .map(|p| game_root.join(p))
        .collect();
    let natives_dir = version_dir.join("natives");
    let extensions = native_library_extensions(platform);
    let extracted = mscl_platform::extract_native_jars(&native_jars, &natives_dir, extensions)?;
    println!(
        "[e2e] native jar {} 个，解压出动态库 {} 个 ({:?})",
        native_jars.len(),
        extracted,
        extensions
    );

    // ---- 7. 构建启动命令 ----
    println!("[e2e] 7/7 构建启动命令...");
    let java = mscl_platform::detect_java()?;
    println!(
        "[e2e] Java: {:?} (主版本 {:?})",
        java.path, java.major_version
    );
    let config = LaunchConfig {
        java_path: java.path.clone(),
        game_root: game_root.clone(),
        game_dir: None,
        assets_root: game_root.join("assets"),
        natives_dir,
        max_memory_mb: 2048,
        window_width: 1280,
        window_height: 720,
        launcher_name: "MSCL".into(),
        launcher_version: env!("CARGO_PKG_VERSION").into(),
    };
    let command = build_launch_command(&version, &config, &Account::offline("MSCLTest"), platform)?;
    for arg in &command.args {
        if arg.contains("${") {
            return Err(format!("存在未替换占位符: {}", arg).into());
        }
    }
    println!("[e2e] 启动命令参数数: {}", command.args.len());
    println!("[e2e] java: {}", command.executable.display());
    println!("[e2e] 前 12 个参数: {:?}", &command.args[..12.min(command.args.len())]);

    // 检查 classpath 里的 jar 是否真实存在
    let sep = if platform == TargetPlatform::Windows { ';' } else { ':' };
    if let Some(cp_arg) = command.args.iter().find(|a| a.contains(&sep.to_string()) && a.contains(".jar")) {
        let jars: Vec<&str> = cp_arg.split(sep).collect();
        let mut missing = 0;
        for j in &jars {
            if !PathBuf::from(j).exists() {
                missing += 1;
            }
        }
        println!("[e2e] classpath 条目 {} 个，缺失 {} 个", jars.len(), missing);
        if missing > 0 {
            return Err(format!("classpath 有 {} 个缺失文件", missing).into());
        }
    }

    println!(
        "[e2e] ===== 端到端验证全部通过（耗时 {:.1} 秒） =====",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}
