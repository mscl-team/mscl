//! 完整版本 JSON 模型 —— 对应 26.3.json / 1.16.5.json 等
//!
//! 顶层字段：arguments, assetIndex, assets, complianceLevel, downloads, id,
//!           javaVersion, libraries, logging, mainClass, minimumLauncherVersion,
//!           releaseTime, time, type

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::library::{Artifact, Library};
use crate::rules::TargetPlatform;

/// downloads 字段：不同版本可能含 client/server/windows_server 等，用 HashMap 容错
pub type VersionDownloads = HashMap<String, Artifact>;

/// 资源索引描述
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetIndex {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    #[serde(rename = "totalSize", default)]
    pub total_size: Option<u64>,
    pub url: String,
}

/// Java 版本要求
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavaVersion {
    pub component: String,
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

/// 日志配置（log4j）
/// 真实结构：logging.client = { argument, file:{...}, type }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Forge 等加载器会生成 `"logging": {}`，因此这里必须可缺省
    #[serde(default)]
    pub client: Option<LoggingClient>,
}

/// logging.client 包装层
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingClient {
    pub argument: String,
    pub file: LoggingFile,
    #[serde(rename = "type")]
    pub log_type: String,
}

/// 日志文件描述（真正含 sha1/size/url 的层）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingFile {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

/// 启动参数：现代版本是 {game:[], jvm:[]}；旧版是 minecraftArguments 字符串
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct VersionArguments {
    #[serde(default)]
    pub game: Vec<serde_json::Value>,
    #[serde(default)]
    pub jvm: Vec<serde_json::Value>,
}

/// 完整版本模型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: super::manifest::VersionType,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(default)]
    pub arguments: Option<VersionArguments>,
    /// 旧版本的字符串参数（1.12 及更早），现代版本没有
    #[serde(rename = "minecraftArguments", default)]
    pub minecraft_arguments: Option<String>,
    /// 加载器版本（Fabric/Forge）经常省略该字段，缺失时视为空
    #[serde(default)]
    pub downloads: VersionDownloads,
    #[serde(rename = "assetIndex", default)]
    pub asset_index: Option<AssetIndex>,
    /// assets 字段是资源索引 id（如 "34"）
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(rename = "javaVersion", default)]
    pub java_version: Option<JavaVersion>,
    #[serde(default)]
    pub logging: Option<LoggingConfig>,
    #[serde(rename = "minimumLauncherVersion", default)]
    pub minimum_launcher_version: Option<u32>,
    #[serde(default)]
    pub compliance_level: Option<u32>,
    /// 继承的父版本 id（Fabric / Forge 等加载器使用）
    #[serde(rename = "inheritsFrom", default)]
    pub inherits_from: Option<String>,
    /// 合并继承后 client.jar 实际所属的版本 id（通常是原版版本号）。
    /// 不参与 JSON 序列化：普通版本为 None，表示与自身 id 相同。
    #[serde(skip)]
    pub client_jar_from: Option<String>,
}

impl Version {
    /// 获取 client.jar 的下载描述
    pub fn client_download(&self) -> Option<&Artifact> {
        self.downloads.get("client")
    }

    /// 按目标平台过滤出适用的 libraries
    /// 纯逻辑：平台信息由调用方（platform 层）传入，core 自身不探测系统
    /// 只做 rules 过滤；classifier/natives 的选择留给阶段 2
    pub fn applicable_libraries(&self, platform: TargetPlatform) -> Vec<&Library> {
        use crate::rules::rule_allows;
        self.libraries
            .iter()
            .filter(|lib| {
                if lib.rules.is_empty() {
                    true
                } else {
                    lib.rules.iter().any(|r| rule_allows(r, platform))
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_version() {
        let json = r#"{
            "id": "26.3",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "arguments": {"game": ["--username", "${auth_player_name}"], "jvm": ["-Xmx2G"]},
            "downloads": {
                "client": {"path":"client.jar","sha1":"abc","size":100,"url":"http://x/client.jar"}
            },
            "assetIndex": {"id":"34","sha1":"s","size":1,"totalSize":2,"url":"u"},
            "assets": "34",
            "libraries": [],
            "javaVersion": {"component":"java-runtime-epsilon","majorVersion":25}
        }"#;
        let v: Version = serde_json::from_str(json).unwrap();
        assert_eq!(v.id, "26.3");
        assert_eq!(v.main_class, "net.minecraft.client.main.Main");
        assert!(v.client_download().is_some());
        assert_eq!(v.java_version.unwrap().major_version, 25);
        assert_eq!(v.arguments.unwrap().game.len(), 2);
    }

    #[test]
    fn parses_legacy_version_with_minecraft_arguments() {
        let json = r#"{
            "id": "1.7.10",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "minecraftArguments": "--username ${auth_player_name} --version ${version_name}",
            "downloads": {}
        }"#;
        let v: Version = serde_json::from_str(json).unwrap();
        assert!(v.minecraft_arguments.unwrap().contains("${auth_player_name}"));
        assert!(v.arguments.is_none());
        assert!(v.asset_index.is_none());
    }
}
