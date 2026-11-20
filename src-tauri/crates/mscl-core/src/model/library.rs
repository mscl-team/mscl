//! Library 与 Rule 模型 —— 对应版本 JSON 中的 libraries[] 元素
//!
//! Library 真实结构（现代版本，含 natives）：
//! {
//!   "name": "org.lwjgl:lwjgl:3.2.1",
//!   "downloads": {
//!     "artifact": {"path": "...", "sha1": "...", "size": 317748, "url": "..."},
//!     "classifiers": {
//!       "natives-macos": {"path": "...", "sha1": "...", "size": 39835, "url": "..."}
//!     }
//!   },
//!   "natives": {"osx": "natives-macos"},
//!   "rules": [{"action": "allow", "os": {"name": "osx"}}]
//! }
//!
//! 注意：最新版（26.x）已无 natives/classifiers，原生库作为普通 artifact 提供，
//! 因此这些字段都是 Option，保证向后兼容。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 一个可下载产物（jar / 原生库），对应 downloads.artifact 或 classifiers 里的值
///
/// 注意：`path` 并非总是存在——版本顶层的 downloads.client/server 就没有 path，
/// 只有 libraries 里的 artifact 才带 path（建议的相对存储路径）。故用 Option。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    #[serde(default)]
    pub path: Option<String>,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

/// classifiers：key 为 classifier 名（如 natives-windows），value 为产物
pub type Classifiers = HashMap<String, Artifact>;

/// downloads 字段：artifact 是主 jar，classifiers 是附属产物（含 natives）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: Option<Classifiers>,
}

/// 操作系统匹配条件
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OsRule {
    /// os 名称：windows / osx / linux
    #[serde(default)]
    pub name: Option<String>,
    /// CPU 架构：x86 / x64 / arm64（旧版用 x86 表示 32 位）
    #[serde(default)]
    pub arch: Option<String>,
    /// 系统版本正则（极少用）
    #[serde(default)]
    pub version: Option<String>,
}

/// 单条规则：action 为 allow 或 disallow，os 为可选的匹配条件
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    #[serde(default)]
    pub os: Option<OsRule>,
    /// 特性条件（如 has_custom_resolution），旧版/Forge 用，阶段 1 不判定
    #[serde(default)]
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

/// 单个 library
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    /// Maven 坐标名，如 "org.lwjgl:lwjgl:3.2.1"
    pub name: String,
    /// 仓库根地址；Fabric / 部分旧版本只提供 name + url，没有 downloads.artifact
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub downloads: LibraryDownloads,
    /// natives 映射：key 为 os 名，value 为 classifier 名
    #[serde(default)]
    pub natives: Option<HashMap<String, String>>,
    /// 适用规则；为空表示所有平台都适用
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_library_with_natives() {
        let json = r#"{
            "name": "org.lwjgl:lwjgl:3.2.1",
            "downloads": {
                "artifact": {
                    "path": "org/lwjgl/lwjgl/3.2.1/lwjgl-3.2.1.jar",
                    "sha1": "2bb514e444994c6fece99a21f76e0c90438e377f",
                    "size": 317748,
                    "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.2.1/lwjgl-3.2.1.jar"
                },
                "classifiers": {
                    "natives-macos": {
                        "path": "org/lwjgl/lwjgl/3.2.1/lwjgl-3.2.1-natives-macos.jar",
                        "sha1": "5a4c271d150906858d475603dcb9479453c60555",
                        "size": 39835,
                        "url": "https://libraries.minecraft.net/x.jar"
                    }
                }
            },
            "natives": {"osx": "natives-macos"},
            "rules": [{"action": "allow", "os": {"name": "osx"}}]
        }"#;
        let lib: Library = serde_json::from_str(json).unwrap();
        assert_eq!(lib.name, "org.lwjgl:lwjgl:3.2.1");
        assert!(lib.downloads.artifact.is_some());
        assert_eq!(lib.downloads.classifiers.as_ref().unwrap().len(), 1);
        assert_eq!(lib.natives.as_ref().unwrap().get("osx").unwrap(), "natives-macos");
        assert_eq!(lib.rules[0].action, RuleAction::Allow);
        assert_eq!(lib.rules[0].os.as_ref().unwrap().name.as_deref(), Some("osx"));
    }

    #[test]
    fn parses_library_without_optional_fields() {
        let json = r#"{
            "name": "at.yawk.lz4:lz4-java:1.10.1",
            "downloads": {
                "artifact": {"path":"p","sha1":"s","size":1,"url":"u"}
            }
        }"#;
        let lib: Library = serde_json::from_str(json).unwrap();
        assert!(lib.natives.is_none());
        assert!(lib.rules.is_empty());
        assert!(lib.downloads.classifiers.is_none());
    }
}
