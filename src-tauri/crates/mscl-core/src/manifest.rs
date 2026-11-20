//! 版本清单拉取与解析（阶段 1）
//!
//! 职责边界：
//!   - core 层负责：HTTP 拉取 JSON + 解析为领域模型 + 校验 sha1
//!   - 不负责：本地文件缓存、存储路径（这些交给 platform/app 层）
//!
//! 数据源：Mojang 官方 API（已验证国内可直接访问）

use crate::error::CoreError;
use crate::model::{Version, VersionManifest};

/// Mojang 官方版本清单地址
pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// 从字符串解析版本清单（纯逻辑，便于本地样本测试）
pub fn parse_manifest(json: &str) -> Result<VersionManifest, CoreError> {
    serde_json::from_str(json).map_err(|e| CoreError::Parse(format!("版本清单解析失败: {}", e)))
}

/// 从字符串解析完整版本 JSON（纯逻辑）
pub fn parse_version(json: &str) -> Result<Version, CoreError> {
    serde_json::from_str(json).map_err(|e| CoreError::Parse(format!("版本 JSON 解析失败: {}", e)))
}

/// 拉取版本清单（官方地址，异步）
pub async fn fetch_manifest(client: &reqwest::Client) -> Result<VersionManifest, CoreError> {
    fetch_manifest_from(client, MANIFEST_URL).await
}

/// 从指定 URL 拉取版本清单（支持镜像源）
pub async fn fetch_manifest_from(
    client: &reqwest::Client,
    url: &str,
) -> Result<VersionManifest, CoreError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| CoreError::Network(format!("拉取版本清单失败 ({}): {}", url, e)))?;

    let status = response.status();
    if !status.is_success() {
        return Err(CoreError::Network(format!(
            "版本清单返回非成功状态 ({}): {}",
            url, status
        )));
    }

    let json = response
        .text()
        .await
        .map_err(|e| CoreError::Network(format!("读取版本清单响应失败: {}", e)))?;

    parse_manifest(&json)
}

/// 按给定 URL 拉取完整版本 JSON
pub async fn fetch_version(
    client: &reqwest::Client,
    url: &str,
) -> Result<Version, CoreError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| CoreError::Network(format!("拉取版本 JSON 失败 ({}): {}", url, e)))?;

    let status = response.status();
    if !status.is_success() {
        return Err(CoreError::Network(format!("版本 JSON 返回非成功状态: {}", status)));
    }

    let json = response
        .text()
        .await
        .map_err(|e| CoreError::Network(format!("读取版本 JSON 响应失败: {}", e)))?;

    parse_version(&json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_real_manifest_sample() {
        // 用真实样本中截取的最小结构验证解析（完整样本 279KB，单测不必全载入）
        let json = r#"{
            "latest": {"release": "26.3", "snapshot": "26.4-snapshot-8"},
            "versions": [{
                "id": "26.3",
                "type": "release",
                "url": "https://piston-meta.mojang.com/v1/packages/abc/26.3.json",
                "time": "2026-11-10T14:28:50+00:00",
                "releaseTime": "2026-11-01T10:00:00+00:00",
                "sha1": "abc",
                "complianceLevel": 1
            }]
        }"#;
        let manifest = parse_manifest(json).unwrap();
        assert_eq!(manifest.latest_release().unwrap().id, "26.3");
        assert_eq!(manifest.versions.len(), 1);
    }

    #[test]
    fn parse_invalid_json_returns_error() {
        let result = parse_manifest("not json");
        assert!(matches!(result, Err(CoreError::Parse(_))));
    }
}
