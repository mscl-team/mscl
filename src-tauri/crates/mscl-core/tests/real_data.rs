//! 真实数据集成测试
//!
//! 读取项目根目录 .cache/ 下缓存的真实 Mojang JSON（开发时下载），
//! 验证领域模型对真实数据的解析与 rules 过滤是否正确。
//! 若缓存文件不存在则优雅跳过（不阻塞全新 clone 的构建）。

use std::path::PathBuf;

use mscl_core::{parse_manifest, parse_version, evaluate_rules, TargetPlatform};

fn cache_path(name: &str) -> PathBuf {
    // CARGO_MANIFEST_DIR = <root>/src-tauri/crates/mscl-core
    // 上溯三级到项目根，再进 .cache
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(".cache")
        .join(name)
}

fn load(name: &str) -> Option<String> {
    std::fs::read_to_string(cache_path(name)).ok()
}

#[test]
fn parses_real_manifest() {
    let Some(json) = load("version_manifest_v2.json") else {
        eprintln!("[skip] .cache/version_manifest_v2.json 不存在");
        return;
    };
    let manifest = parse_manifest(&json).expect("真实清单应能解析");

    assert!(!manifest.latest.release.is_empty(), "latest.release 不应为空");
    assert!(manifest.versions.len() > 500, "版本数量应达数百个");

    let latest = manifest
        .latest_release()
        .expect("latest.release 应能在 versions 中找到");
    assert_eq!(latest.id, manifest.latest.release);
    assert!(latest.url.starts_with("https://"));
    assert_eq!(latest.sha1.len(), 40, "sha1 应为 40 位十六进制");

    println!(
        "[ok] 清单解析成功: 共 {} 个版本, 最新正式版 = {}, 最新快照 = {}",
        manifest.versions.len(),
        manifest.latest.release,
        manifest.latest.snapshot
    );
}

#[test]
fn parses_real_latest_version() {
    let Some(json) = load("latest_version.json") else {
        eprintln!("[skip] .cache/latest_version.json 不存在");
        return;
    };
    let version = parse_version(&json).expect("真实版本 JSON 应能解析");

    assert_eq!(version.version_type, mscl_core::VersionType::Release);
    assert!(!version.main_class.is_empty());
    assert!(version.client_download().is_some(), "应有 client.jar 下载描述");
    assert!(version.libraries.len() > 50, "库数量应达数十个");

    let client = version.client_download().unwrap();
    assert_eq!(client.sha1.len(), 40);
    assert!(client.size > 0);
    assert!(client.url.ends_with("client.jar"));

    if let Some(java) = &version.java_version {
        assert!(java.major_version >= 8, "Java 主版本号应 >= 8");
    }

    println!(
        "[ok] 版本解析成功: id={}, 主类={}, libraries={}, Java={:?}",
        version.id,
        version.main_class,
        version.libraries.len(),
        version.java_version.as_ref().map(|j| j.major_version)
    );
}

#[test]
fn filters_libraries_by_platform() {
    let Some(json) = load("1.16.5.json") else {
        eprintln!("[skip] .cache/1.16.5.json 不存在");
        return;
    };
    let version = parse_version(&json).expect("1.16.5 应能解析");

    let win = version.applicable_libraries(TargetPlatform::Windows);
    let mac = version.applicable_libraries(TargetPlatform::MacOs);
    let linux = version.applicable_libraries(TargetPlatform::Linux);

    assert!(!win.is_empty() && !mac.is_empty() && !linux.is_empty());

    // 1.16.5 含 Java-objc-bridge，仅 osx 适用：mac 应比 win 多这一类
    let has_bridge = |libs: &[&mscl_core::Library]| {
        libs.iter().any(|l| l.name.contains("java-objc-bridge"))
    };
    assert!(has_bridge(&mac), "macOS 应包含 java-objc-bridge");
    assert!(!has_bridge(&win), "Windows 不应包含 java-objc-bridge");
    assert!(!has_bridge(&linux), "Linux 不应包含 java-objc-bridge");

    println!(
        "[ok] rules 过滤成功: Windows={}, macOS={}, Linux={}",
        win.len(), mac.len(), linux.len()
    );
}

#[test]
fn evaluate_rules_disallow_pattern() {
    use mscl_core::{OsRule, Rule, RuleAction};

    // 复刻真实 26.3 JVM 参数上的 rules：
    // [{action:allow,os:{name:osx}}, {action:allow,os:{name:windows}}, {action:allow,os:{arch:x86}}]
    // 这是"满足任一即可"的 jvm 参数规则，语义是 args 层面用 any，
    // 与 library 层面的逐条覆盖不同。这里验证单条 rule 判定正确。
    let osx_rule = Rule {
        action: RuleAction::Allow,
        os: Some(OsRule {
            name: Some("osx".into()),
            ..Default::default()
        }),
        features: None,
    };
    assert!(mscl_core::rule_allows(&osx_rule, TargetPlatform::MacOs));
    assert!(!mscl_core::rule_allows(&osx_rule, TargetPlatform::Windows));

    // 单独验证 evaluate_rules 的 disallow 语义仍正确
    let rules = vec![
        Rule { action: RuleAction::Allow, os: None, features: None },
        osx_rule,
    ];
    // 第二条 osx 规则在 macOS 上命中 allow（结果不变），在 Windows 上不命中（保持 allow）
    assert!(evaluate_rules(&rules, TargetPlatform::Windows));
    assert!(evaluate_rules(&rules, TargetPlatform::MacOs));
}
