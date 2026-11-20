//! 下载模块（阶段 2，9.1 扩展镜像与校验报告）
//!
//! 四层结构：
//!   - task.rs:   任务规划（纯逻辑）
//!   - verify.rs: SHA1/size 校验（纯逻辑，流式）
//!   - source.rs: 下载源（官方/镜像）URL 重写（纯逻辑）
//!   - engine.rs: 并发下载引擎（异步 IO）

mod engine;
mod source;
mod task;
mod verify;

pub use engine::{
    resolve_path, DownloadEngine, DownloadEvent, DownloadOptions, DownloadSummary, ProgressCallback,
    TaskResult,
};
pub use source::{DownloadSource, OFFICIAL_MANIFEST_URL};
pub use task::{
    asset_download_url, asset_object_path, native_library_extensions, plan_all_libraries,
    plan_assets, plan_library, plan_native_jars, plan_version_tasks, AssetIndexFile, AssetObject,
    DownloadKind, DownloadTask,
};
pub use verify::{sha1_of_file, verify_file, verify_tasks, VerifyOutcome, VerifyReport};
