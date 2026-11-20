//! 领域模型模块
//!
//! 所有模型都用 serde 直接映射 Mojang 的 JSON 结构。
//! 设计原则：
//!   1. 字段用 Option 兜底未知/缺失字段，容忍 Mojang 新增字段（默认忽略未知字段）
//!   2. 类型贴近原始 JSON，解析层不做业务判断
//!   3. 业务判断（如 rules 过滤）放在独立的 rules 模块

pub mod inherit;
pub mod library;
pub mod manifest;
pub mod version;

pub use inherit::merge_inherited;
pub use library::{Artifact, Classifiers, Library, LibraryDownloads, OsRule, Rule, RuleAction};
pub use manifest::{LatestVersions, VersionSummary, VersionType, VersionManifest};
pub use version::{
    AssetIndex, JavaVersion, LoggingClient, LoggingConfig, LoggingFile, Version, VersionArguments,
};
