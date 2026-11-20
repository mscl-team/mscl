//! 阶段 3 真实数据集成测试：用真实版本 JSON 构建启动命令
//! 验证 classpath、占位符、平台参数、参数顺序等

use std::path::PathBuf;

use mscl_core::{
    build_classpath, build_launch_command, game_arguments, jvm_arguments, parse_version, Account,
    LaunchConfig, TargetPlatform,
};

fn load(name: &str) -> Option<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(".cache")
        .join(name);
    std::fs::read_to_string(path).ok()
}

fn config() -> LaunchConfig {
    LaunchConfig {
        java_path: PathBuf::from("C:/Program Files/Java/jdk-25/bin/java.exe"),
        game_root: PathBuf::from("C:/MSCL/game"),
        game_dir: None,
        assets_root: PathBuf::from("C:/MSCL/assets"),
        natives_dir: PathBuf::from("C:/MSCL/natives"),
        max_memory_mb: 4096,
        window_width: 1280,
        window_height: 720,
        launcher_name: "MSCL".into(),
        launcher_version: "0.1.0".into(),
    }
}

#[test]
fn builds_real_latest_command_windows() {
    let Some(json) = load("latest_version.json") else {
        eprintln!("[skip] latest_version.json 不存在");
        return;
    };
    let version = parse_version(&json).unwrap();
    let cmd = build_launch_command(
        &version, &config(), &Account::offline("月之海"), TargetPlatform::Windows,
    ).unwrap();

    // 基础结构
    assert_eq!(cmd.executable, config().java_path);
    assert!(cmd.args.contains(&"-Xmx4096M".to_string()), "应有内存参数");
    assert!(cmd.args.contains(&version.main_class), "应有主类");

    // 平台参数：Windows 应有 heapdump 参数，不应有 mac 的 -XstartOnFirstThread
    assert!(cmd.args.iter().any(|a| a.contains("HeapDumpPath")));
    assert!(!cmd.args.iter().any(|a| a.contains("XstartOnFirstThread")));

    // 所有占位符已替换
    for arg in &cmd.args {
        assert!(!arg.contains("${"), "残留占位符: {}", arg);
    }

    // game 参数关键项
    assert!(cmd.args.contains(&"--username".to_string()));
    assert!(cmd.args.contains(&"月之海".to_string()));
    assert!(cmd.args.contains(&"--assetIndex".to_string()));
    assert!(cmd.args.contains(&"34".to_string())); // asset index id

    // classpath 含 client jar（路径格式为 versions/{id}/{id}.jar），Windows 用分号
    let cp = cmd.args.iter().find(|a| a.contains("26.3.jar")).unwrap();
    assert!(cp.contains(';'), "Windows classpath 应用分号分隔");

    // 命令行可渲染
    assert!(cmd.to_command_line().contains("java.exe"));

    println!(
        "[ok] 26.3 Windows 启动命令构建成功: {} 个参数, classpath 长度 {}",
        cmd.args.len(), cp.len()
    );
}

#[test]
fn builds_real_latest_command_macos() {
    let Some(json) = load("latest_version.json") else { return; };
    let version = parse_version(&json).unwrap();
    let mut cfg = config();
    cfg.java_path = PathBuf::from("/Library/Java/java");
    cfg.game_root = PathBuf::from("/Users/me/.mscl/game");
    cfg.assets_root = PathBuf::from("/Users/me/.mscl/assets");
    cfg.natives_dir = PathBuf::from("/Users/me/.mscl/natives");

    let cmd = build_launch_command(
        &version, &cfg, &Account::offline("Steve"), TargetPlatform::MacOs,
    ).unwrap();

    // macOS 特有参数
    assert!(cmd.args.iter().any(|a| a.contains("XstartOnFirstThread")));
    // macOS 不应有 windows heapdump
    assert!(!cmd.args.iter().any(|a| a.contains("HeapDumpPath")));
    // classpath 用冒号
    let cp = cmd.args.iter().find(|a| a.contains("26.3.jar")).unwrap();
    assert!(cp.contains(':'), "macOS classpath 应用冒号分隔");
    assert!(!cp.contains(";/"), "macOS 不应有分号");

    println!("[ok] 26.3 macOS 启动命令构建成功: {} 个参数", cmd.args.len());
}

#[test]
fn builds_legacy_command() {
    let Some(json) = load("1.16.5.json") else { return; };
    let version = parse_version(&json).unwrap();
    let cmd = build_launch_command(
        &version, &config(), &Account::offline("Steve"), TargetPlatform::Windows,
    ).unwrap();

    assert!(cmd.args.contains(&version.main_class));
    for arg in &cmd.args {
        assert!(!arg.contains("${"), "老版本残留占位符: {}", arg);
    }
    // 老版本用 -Djava.library.path=${natives_directory}
    assert!(cmd.args.iter().any(|a| a.contains("java.library.path")));

    println!("[ok] 1.16.5 老版本启动命令构建成功: {} 个参数", cmd.args.len());
}

#[test]
fn jvm_game_args_platform_rules() {
    let Some(json) = load("latest_version.json") else { return; };
    let version = parse_version(&json).unwrap();

    let jvm_win = jvm_arguments(&version, TargetPlatform::Windows);
    let jvm_mac = jvm_arguments(&version, TargetPlatform::MacOs);
    assert!(jvm_win.iter().any(|a| a.contains("HeapDumpPath")));
    assert!(jvm_mac.iter().any(|a| a.contains("XstartOnFirstThread")));

    let game = game_arguments(&version, TargetPlatform::Windows);
    // feature 参数（--demo, --width 等）应被排除
    assert!(!game.iter().any(|a| a == "--demo"));
    assert!(!game.iter().any(|a| a == "--width"));
    println!("[ok] JVM/game 参数平台过滤正确: win jvm={}, game={}", jvm_win.len(), game.len());
}

#[test]
fn classpath_includes_all_libraries() {
    let Some(json) = load("latest_version.json") else { return; };
    let version = parse_version(&json).unwrap();
    let cp = build_classpath(&version, &PathBuf::from("C:/mscl"), TargetPlatform::Windows);
    let entry_count = cp.split(';').count();
    assert!(entry_count >= 70, "classpath 应包含至少70个条目(库+client), 实际 {}", entry_count);
    assert!(cp.contains("26.3.jar"), "classpath 应包含 client jar");
    println!("[ok] classpath 条目数: {}", entry_count);
}
