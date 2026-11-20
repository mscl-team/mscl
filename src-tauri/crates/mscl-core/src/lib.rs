//! MSCL 核心领域层
//!
//! 这一层是纯逻辑，不依赖 Tauri、不依赖文件系统、不依赖具体平台 API。
//! 平台信息由调用方（platform 层）通过参数传入（如 TargetPlatform）。
//!
//! 职责（按阶段实现）：
//!   - 阶段 1 ✅：版本清单与元数据解析、rules 过滤
//!   - 阶段 2 ✅：下载任务规划、SHA1/size 校验、并发下载引擎
//!   - 阶段 3 ✅：启动命令构建、${} 占位符替换
//!
//! 保持这一层的纯粹性，是本架构"扎实、少 BUG、可长期演进"的基石。

pub mod download;
pub mod error;
pub mod fabric;
pub mod launch;
pub mod manifest;
pub mod model;
pub mod rules;

pub use fabric::{
    fetch_fabric_loaders, fetch_fabric_loaders_from, fetch_fabric_profile,
    fetch_fabric_profile_from, FabricLoader, FabricLoaderEntry, FABRIC_META,
};

pub use download::{
    asset_download_url, asset_object_path, native_library_extensions, plan_all_libraries,
    plan_assets, plan_library, plan_native_jars, plan_version_tasks, resolve_path, verify_file,
    verify_tasks, AssetIndexFile, AssetObject, DownloadEngine, DownloadEvent, DownloadKind,
    DownloadOptions, DownloadSource, DownloadSummary, DownloadTask, ProgressCallback, TaskResult,
    VerifyOutcome, VerifyReport, OFFICIAL_MANIFEST_URL,
};
pub use error::{CoreError, Result};
pub use launch::{
    build_classpath, build_launch_command, game_arguments, jvm_arguments, Account, LaunchCommand,
    LaunchConfig, TokenResolver, Tokens,
};
pub use manifest::{
    fetch_manifest, fetch_manifest_from, fetch_version, parse_manifest, parse_version, MANIFEST_URL,
};
pub use model::{
    merge_inherited, Artifact, AssetIndex, JavaVersion, LatestVersions, Library, LoggingClient,
    LoggingConfig, LoggingFile, OsRule, Rule, RuleAction, Version, VersionArguments,
    VersionManifest, VersionSummary, VersionType,
};
pub use rules::{evaluate_rules, os_rule_matches, rule_allows, TargetPlatform};

/// 占位函数：保留给桥接层健康检查使用
pub fn core_ready() -> &'static str {
    "mscl-core ready (阶段3: 版本解析 + 下载 + 启动命令已就绪)"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_compiles() {
        assert!(core_ready().contains("mscl-core ready"));
    }
}
