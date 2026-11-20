//! 实际启动游戏验证（阶段 7）
//!
//! 复用 platform 层 GameProcess 启动已下载的最新版 Minecraft，
//! 实时捕获 stdout/stderr，检测关键错误（原生库/类缺失/异常），
//! 观察一段固定时长后结束进程。
//!
//! 运行：cargo run --example e2e_launch
//! 前置：先运行 e2e_live 完成下载

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mscl_core::{build_launch_command, Account, DownloadTask, LaunchConfig, Version};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let game_root = mscl_platform::default_data_dir()?;
    let platform = mscl_platform::current_core_platform();
    let version_id = "26.3";

    // 读取已下载的版本 JSON
    let version_path = game_root.join(format!("versions/{0}/{0}.json", version_id));
    if !version_path.exists() {
        return Err(format!("版本 {} 未下载，请先运行 e2e_live", version_id).into());
    }
    let version: Version = serde_json::from_str(&std::fs::read_to_string(&version_path)?)?;
    println!("[launch] 版本: {} (Java {:?})", version.id,
        version.java_version.as_ref().map(|j| j.major_version));

    // 校验 client.jar 存在
    let client = DownloadTask::plan_client(&version).ok_or("无 client 下载定义")?;
    let client_path = game_root.join(&client.relative_path);
    if !client_path.exists() {
        return Err("client.jar 不存在".into());
    }
    println!("[launch] client.jar: {:.1} MB",
        std::fs::metadata(&client_path)?.len() as f64 / 1024.0 / 1024.0);

    // 检测 Java
    let java = mscl_platform::detect_java()?;
    println!("[launch] Java: {}", java.path.display());

    // 构建命令
    let natives_dir = game_root.join(format!("versions/{}/natives", version_id));
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
    println!("[launch] 启动进程，参数 {} 个...", command.args.len());

    // 用 platform 层启动，捕获输出
    let logs: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let logs_clone = logs.clone();
    let callback: mscl_platform::OutputCallback = Arc::new(move |out| {
        let line = match out {
            mscl_platform::ProcessOutput::Stdout(s) => format!("[out] {}", s),
            mscl_platform::ProcessOutput::Stderr(s) => format!("[err] {}", s),
        };
        if let Ok(mut l) = logs_clone.lock() {
            l.push(line.clone());
        }
        println!("[launch] {}", line);
    });

    let mut process = mscl_platform::GameProcess::spawn(
        &command.executable,
        &command.args,
        &command.working_dir,
        Some(callback),
    )?;
    println!("[launch] 游戏 PID: {}", process.id());

    // 观察 90 秒或直到进程退出
    let observe_secs = 90;
    let started = Instant::now();
    let mut exited_early = false;
    while started.elapsed() < Duration::from_secs(observe_secs) {
        if !process.is_running() {
            exited_early = true;
            break;
        }
        std::thread::sleep(Duration::from_secs(2));
    }

    // 分析日志
    let snapshot = logs.lock().unwrap().clone();
    let fatal_keywords = [
        "UnsatisfiedLinkError",
        "NoClassDefFoundError",
        "ClassNotFoundException",
        "Exception in thread",
        "GLFW error",
    ];
    let mut fatal_hits = Vec::new();
    for line in &snapshot {
        for kw in &fatal_keywords {
            if line.contains(kw) {
                fatal_hits.push((*kw, line.clone()));
            }
        }
    }
    // 就绪信号
    let ready_signals = ["Sound engine started", "OpenAL", "LWJGL", "Render thread", " net.minecraft.client.main.Main", "Setting user"];
    let ready_hits: Vec<&String> = snapshot.iter()
        .filter(|l| ready_signals.iter().any(|s| l.contains(s)))
        .collect();

    println!("[launch] ===== 观察结束 =====");
    println!("[launch] 运行时长: {:.0} 秒, 提前退出: {}", started.elapsed().as_secs(), exited_early);
    println!("[launch] 日志行数: {}", snapshot.len());
    println!("[launch] 就绪信号命中: {}", ready_hits.len());
    for h in ready_hits.iter().take(6) {
        println!("[launch]   {}", h);
    }
    if fatal_hits.is_empty() {
        println!("[launch] ✅ 未发现致命错误（原生库/类缺失/异常）");
    } else {
        println!("[launch] ⚠️  发现 {} 处疑似致命错误:", fatal_hits.len());
        for (kw, line) in fatal_hits.iter().take(10) {
            println!("[launch]   [{}] {}", kw, line);
        }
    }

    // 结束游戏进程
    if process.is_running() {
        println!("[launch] 结束测试进程...");
        let _ = process.kill();
    }

    // 只要有就绪信号且无致命错误即视为启动成功
    let has_ready = snapshot.iter().any(|l| l.contains("Setting user") || l.contains("Sound engine started") || l.contains("LWJGL"));
    if fatal_hits.is_empty() && has_ready {
        println!("[launch] ===== 实际启动验证通过 =====");
        Ok(())
    } else if fatal_hits.is_empty() && !exited_early {
        println!("[launch] ===== 进程稳定运行未报错（未捕捉到明确就绪信号） =====");
        Ok(())
    } else {
        Err("启动过程发现错误或提前退出".into())
    }
}
