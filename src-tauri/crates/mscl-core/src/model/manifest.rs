//! 版本清单模型 —— 对应 version_manifest_v2.json
//!
//! 真实结构示例：
//! {
//!   "latest": { "release": "26.3", "snapshot": "26.4-snapshot-8" },
//!   "versions": [
//!     {
//!       "id": "26.4-snapshot-8",
//!       "type": "snapshot",
//!       "url": "https://piston-meta.mojang.com/v1/packages/.../26.4-snapshot-8.json",
//!       "time": "2026-11-10T14:28:50+00:00",
//!       "releaseTime": "2026-11-10T14:20:05+00:00",
//!       "sha1": "46f6...",
//!       "complianceLevel": 1
//!     }
//!   ]
//! }

use serde::{Deserialize, Serialize};

/// 版本类型：正式版 / 快照 / 老版本等
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionType {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
    #[serde(other)]
    Unknown,
}

impl VersionType {
    /// 返回 version_type token 的值（用于启动参数 ${version_type}）
    pub fn as_str(&self) -> &'static str {
        match self {
            VersionType::Release => "release",
            VersionType::Snapshot => "snapshot",
            VersionType::OldBeta => "old_beta",
            VersionType::OldAlpha => "old_alpha",
            VersionType::Unknown => "unknown",
        }
    }
}

/// 清单里的"最新版本"指针
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

/// 清单里的单个版本摘要（不含完整元数据，只有 id/url 等）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionSummary {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: VersionType,
    pub url: String,
    pub time: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
    pub sha1: String,
    /// 合规等级，新版本才有；老版本缺失，故用 Option
    #[serde(rename = "complianceLevel", default)]
    pub compliance_level: Option<u32>,
}

/// 版本清单根结构
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionSummary>,
}

impl VersionManifest {
    /// 取最新正式版的摘要
    pub fn latest_release(&self) -> Option<&VersionSummary> {
        self.versions
            .iter()
            .find(|v| v.id == self.latest.release)
    }

    /// 按 id 精确查找版本摘要
    pub fn find(&self, id: &str) -> Option<&VersionSummary> {
        self.versions.iter().find(|v| v.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest_sample() {
        let json = r#"{
            "latest": {"release": "1.7.10", "snapshot": "26.4-snapshot-8"},
            "versions": [
                {
                    "id": "26.4-snapshot-8",
                    "type": "snapshot",
                    "url": "https://example.com/26.4.json",
                    "time": "2026-11-10T14:28:50+00:00",
                    "releaseTime": "2026-11-10T14:20:05+00:00",
                    "sha1": "abc123",
                    "complianceLevel": 1
                },
                {
                    "id": "1.7.10",
                    "type": "release",
                    "url": "https://example.com/1.7.10.json",
                    "time": "2015-05-19T15:44:11+00:00",
                    "releaseTime": "2014-05-14T00:00:00+00:00",
                    "sha1": "def456"
                }
            ]
        }"#;

        let manifest: VersionManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.latest.release, "1.7.10");
        assert_eq!(manifest.versions.len(), 2);
        assert_eq!(manifest.versions[0].compliance_level, Some(1));
        assert_eq!(manifest.versions[1].compliance_level, None);
        assert_eq!(manifest.find("1.7.10").unwrap().version_type, VersionType::Release);
        assert_eq!(manifest.latest_release().unwrap().id, "1.7.10");
    }

    #[test]
    fn unknown_version_type_is_tolerated() {
        let json = r#"{
            "latest": {"release": "x", "snapshot": "y"},
            "versions": [{
                "id": "x", "type": "futureType", "url": "u",
                "time": "t", "releaseTime": "rt", "sha1": "s"
            }]
        }"#;
        let manifest: VersionManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.versions[0].version_type, VersionType::Unknown);
    }
}
