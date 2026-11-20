//! 阶段 2 真实下载验证（访问 Mojang CDN）
//! 下载一个真实的小资源文件，验证下载引擎的：
//!   1. 实际下载 + 流式落盘 + SHA1 校验
//!   2. 二次运行时校验命中跳过（缓存逻辑）
//!
//! 该文件仅在 .cache/asset_index_34.json 存在时运行（避免数据缺失时误报）。
//! 下载量约 9KB，网络波动时可能因重试而稍慢。

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use mscl_core::{DownloadEngine, DownloadTask};

#[tokio::test]
async fn downloads_and_verifies_real_asset() {
    // 真实数据：icons/icon_128x128.png
    let hash = "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356";
    let size = 9101u64;

    let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(".cache")
        .join("asset_index_34.json");
    if !cache.exists() {
        eprintln!("[skip] asset_index_34.json 不存在，跳过真实下载测试");
        return;
    }

    // 用独立临时目录，避免污染
    let root = std::env::temp_dir().join(format!("mscl_download_live_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let task = DownloadTask {
        url: mscl_core::asset_download_url(hash),
        relative_path: mscl_core::asset_object_path(hash),
        sha1: hash.to_string(),
        size,
        kind: mscl_core::DownloadKind::Asset,
    };

    let client = reqwest::Client::new();
    let engine = DownloadEngine::new(client, root.clone());

    // 第一次：应实际下载
    let counter = AtomicUsize::new(0);
    let summary = engine
        .download_all(std::slice::from_ref(&task), None)
        .await
        .expect("下载不应返回致命错误");
    assert_eq!(summary.downloaded, 1, "第一次应下载 1 个文件");
    assert_eq!(summary.failed, 0, "不应有失败: {:?}", summary.errors);
    assert!(engine.is_task_valid(&task).unwrap(), "下载后文件应校验通过");

    // 物理文件确实存在
    let full = root.join(&task.relative_path);
    assert!(full.exists(), "文件应落盘: {}", full.display());
    assert_eq!(std::fs::metadata(&full).unwrap().len(), size);
    let _ = counter.fetch_add(1, Ordering::SeqCst);

    // 第二次：应校验命中跳过
    let summary2 = engine
        .download_all(&[task], None)
        .await
        .expect("二次下载不应返回错误");
    assert_eq!(summary2.skipped, 1, "第二次应跳过 1 个已校验文件");
    assert_eq!(summary2.downloaded, 0, "第二次不应重复下载");

    println!(
        "[ok] 真实下载验证通过: 9KB 资源下载成功 + SHA1 校验通过 + 二次跳过 (count={})",
        counter.load(Ordering::SeqCst)
    );

    // 清理
    let _ = std::fs::remove_dir_all(&root);
}
