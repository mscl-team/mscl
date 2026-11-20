//! 阶段 2 真实数据集成测试：验证下载任务规划
//! 读取 .cache/ 下的真实 JSON，文件不存在则跳过

use std::path::PathBuf;

use mscl_core::{
    parse_version, plan_all_libraries, plan_assets, AssetIndexFile, DownloadKind, DownloadTask,
    TargetPlatform,
};

fn load(name: &str) -> Option<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(".cache")
        .join(name);
    std::fs::read_to_string(path).ok()
}

#[test]
fn plans_real_latest_version_downloads() {
    let Some(json) = load("latest_version.json") else {
        eprintln!("[skip] latest_version.json 不存在");
        return;
    };
    let version = parse_version(&json).unwrap();

    let client = DownloadTask::plan_client(&version).expect("应有 client.jar");
    assert_eq!(client.kind, DownloadKind::ClientJar);
    assert_eq!(client.relative_path, format!("versions/{0}/{0}.jar", version.id));
    assert_eq!(client.sha1.len(), 40);
    assert!(client.size > 1_000_000, "client.jar 应大于 1MB");

    let asset_idx = DownloadTask::plan_asset_index(&version).expect("应有 assetIndex");
    assert_eq!(asset_idx.relative_path, format!("assets/indexes/{}.json", version.asset_index.as_ref().unwrap().id));

    let libs = plan_all_libraries(&version, TargetPlatform::Windows);
    assert!(libs.len() >= 40, "Windows 库数量应 >= 40, 实际 {}", libs.len());
    // 所有库都应有合法路径和 sha1
    for t in &libs {
        assert!(t.relative_path.starts_with("libraries/"), "库路径异常: {}", t.relative_path);
        assert_eq!(t.sha1.len(), 40, "库 sha1 异常: {}", t.sha1);
        assert!(!t.url.is_empty(), "库 URL 不应为空");
    }

    println!(
        "[ok] 最新版本下载规划: client.jar {} MB, assetIndex={}, Windows 库任务={}",
        client.size / 1024 / 1024,
        asset_idx.relative_path,
        libs.len()
    );
}

#[test]
fn plans_real_assets() {
    let Some(json) = load("asset_index_34.json") else {
        eprintln!("[skip] asset_index_34.json 不存在");
        return;
    };
    let index: AssetIndexFile = serde_json::from_str(&json).unwrap();
    let tasks = plan_assets(&index);

    assert!(tasks.len() > 4000, "资源数量应达数千, 实际 {}", tasks.len());
    // 抽查：路径与 URL 的 hash 应一致，且目录为 hash 前两位
    for t in tasks.iter().take(20) {
        assert!(t.url.starts_with("https://resources.download.minecraft.net/"));
        assert_eq!(t.url.len(), "https://resources.download.minecraft.net/".len() + 2 + 1 + 40);
        // relative_path = assets/objects/{2位}/{40位}
        let parts: Vec<&str> = t.relative_path.split('/').collect();
        assert_eq!(parts.len(), 4, "路径分段异常: {}", t.relative_path);
        assert_eq!(parts[2].len(), 2);
        assert_eq!(parts[3].len(), 40);
        assert_eq!(parts[2], &t.sha1[..2]);
        assert_eq!(parts[3], t.sha1.as_str());
        // URL 末尾应与完整 hash 一致
        assert!(t.url.ends_with(&t.sha1));
    }

    println!("[ok] 资源规划: 共 {} 个资源文件, 路径与 URL 规则校验通过", tasks.len());
}

#[test]
fn legacy_version_has_native_tasks() {
    let Some(json) = load("1.16.5.json") else {
        eprintln!("[skip] 1.16.5.json 不存在");
        return;
    };
    let version = parse_version(&json).unwrap();

    let win = plan_all_libraries(&version, TargetPlatform::Windows);
    let natives: Vec<_> = win.iter().filter(|t| t.kind == DownloadKind::Native).collect();
    assert!(!natives.is_empty(), "1.16.5 Windows 应有 natives 任务");
    for n in &natives {
        assert!(n.relative_path.contains("natives-windows"), "native 路径异常: {}", n.relative_path);
    }

    println!("[ok] 1.16.5 Windows natives 任务数: {}", natives.len());
}
