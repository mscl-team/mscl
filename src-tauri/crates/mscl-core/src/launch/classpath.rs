//! Classpath 构建（阶段 3）
//!
//! Minecraft 启动时 -cp 参数需要所有 library jar + client.jar，
//! 用平台相关的分隔符连接（Windows 分号，Unix 冒号）。

use std::path::{Path, PathBuf};

use crate::download::{plan_all_libraries, DownloadTask};
use crate::model::Version;
use crate::rules::TargetPlatform;

/// 获取 classpath 分隔符
pub fn classpath_separator(platform: TargetPlatform) -> &'static str {
    match platform {
        TargetPlatform::Windows => ";",
        _ => ":",
    }
}

/// 从版本、游戏根目录、平台构建完整 classpath 字符串
///
/// 逻辑：
///   1. 规划所有适用 library 的下载任务（本地路径与下载路径一致）
///   2. 过滤出 jar 文件（Library + Native，跳过 Asset 等）
///   3. 加上 client.jar
///   4. 用绝对路径 + 平台分隔符连接
pub fn build_classpath(
    version: &Version,
    game_root: &Path,
    platform: TargetPlatform,
) -> String {
    let mut paths: Vec<PathBuf> = Vec::new();

    // 所有 library jar（含 native classifier jar）
    for task in plan_all_libraries(version, platform) {
        if matches!(task.kind, crate::download::DownloadKind::Library | crate::download::DownloadKind::Native) {
            paths.push(game_root.join(&task.relative_path));
        }
    }

    // client.jar
    if let Some(client) = DownloadTask::plan_client(version) {
        paths.push(game_root.join(&client.relative_path));
    }

    let separator = classpath_separator(platform);
    paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(separator)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separator_by_platform() {
        assert_eq!(classpath_separator(TargetPlatform::Windows), ";");
        assert_eq!(classpath_separator(TargetPlatform::MacOs), ":");
        assert_eq!(classpath_separator(TargetPlatform::Linux), ":");
    }
}
