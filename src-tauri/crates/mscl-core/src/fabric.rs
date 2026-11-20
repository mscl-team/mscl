//! Fabric 加载器元数据（阶段 10）
//!
//! 利用 Fabric Meta API 直接拿到 profile JSON，无需下载并执行 installer jar：
//!   - GET /v2/versions/loader/{game}                    → loader 列表
//!   - GET /v2/versions/loader/{game}/{loader}/profile/json → 继承原版的版本描述
//!
//! 这样跨平台且不依赖本地 Java，安装失败面更小。

use crate::error::CoreError;
use crate::model::Version;

/// Fabric Meta 根地址
pub const FABRIC_META: &str = "https://meta.fabricmc.net/v2";

/// loader 列表条目（只取关心字段）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FabricLoaderEntry {
    pub loader: FabricLoader,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FabricLoader {
    pub version: String,
    #[serde(default)]
    pub stable: bool,
}

/// 拉取某个游戏版本可用的 Fabric loader 列表
pub async fn fetch_fabric_loaders(
    client: &reqwest::Client,
    game_version: &str,
) -> Result<Vec<FabricLoader>, CoreError> {
    fetch_fabric_loaders_from(client, FABRIC_META, game_version).await
}

/// 同上，但可指定 Meta 根地址（走镜像时更稳更快）
pub async fn fetch_fabric_loaders_from(
    client: &reqwest::Client,
    meta_base: &str,
    game_version: &str,
) -> Result<Vec<FabricLoader>, CoreError> {
    let url = format!("{}/versions/loader/{}", meta_base, game_version);
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| CoreError::Network(format!("获取 Fabric loader 列表失败 ({}): {}", url, e)))?;

    if !response.status().is_success() {
        return Err(CoreError::Network(format!(
            "Fabric loader 列表返回非成功状态 ({}): {}",
            url,
            response.status()
        )));
    }

    let entries: Vec<FabricLoaderEntry> = response
        .json()
        .await
        .map_err(|e| CoreError::Network(format!("解析 Fabric loader 列表失败: {}", e)))?;

    Ok(entries.into_iter().map(|e| e.loader).collect())
}

/// 拉取 Fabric profile JSON（含 inheritsFrom，需与父版本合并后使用）
pub async fn fetch_fabric_profile(
    client: &reqwest::Client,
    game_version: &str,
    loader_version: &str,
) -> Result<Version, CoreError> {
    fetch_fabric_profile_from(client, FABRIC_META, game_version, loader_version).await
}

/// 同上，但可指定 Meta 根地址（镜像）
pub async fn fetch_fabric_profile_from(
    client: &reqwest::Client,
    meta_base: &str,
    game_version: &str,
    loader_version: &str,
) -> Result<Version, CoreError> {
    let url = format!(
        "{}/versions/loader/{}/{}/profile/json",
        meta_base, game_version, loader_version
    );
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| CoreError::Network(format!("获取 Fabric profile 失败 ({}): {}", url, e)))?;

    if !response.status().is_success() {
        return Err(CoreError::Network(format!(
            "Fabric profile 返回非成功状态 ({}): {}",
            url,
            response.status()
        )));
    }

    let json = response
        .text()
        .await
        .map_err(|e| CoreError::Network(format!("读取 Fabric profile 响应失败: {}", e)))?;

    crate::manifest::parse_version(&json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_loader_list() {
        let json = r#"[
            {"loader": {"separator": ".", "build": 15, "maven": "net.fabricmc:fabric-loader:0.16.9", "version": "0.16.9", "stable": true}, "intermediary": {"version": "1.21.1"}},
            {"loader": {"version": "0.17.0-beta.1", "stable": false}}
        ]"#;
        let entries: Vec<FabricLoaderEntry> = serde_json::from_str(json).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].loader.stable);
        assert_eq!(entries[0].loader.version, "0.16.9");
        assert!(!entries[1].loader.stable);
    }

    #[test]
    fn parses_fabric_profile_with_inheritance() {
        // 真实 profile 结构的关键字段（省略部分 libraries）
        let json = r#"{
            "id": "fabric-loader-0.16.9-1.21.1",
            "inheritsFrom": "1.21.1",
            "releaseTime": "2024-08-08T00:00:00+00:00",
            "time": "2024-08-08T00:00:00+00:00",
            "type": "release",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "arguments": {"game": [], "jvm": ["-DFabricMcEmu=net.minecraft.client.main.Main "]},
            "libraries": [
                {"name": "net.fabricmc:fabric-loader:0.16.9", "url": "https://maven.fabricmc.net/", "downloads": {"artifact": {"path": "net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar", "sha1": "abc", "size": 100, "url": "https://maven.fabricmc.net/fabric-loader.jar"}}}
            ]
        }"#;
        let version = crate::manifest::parse_version(json).unwrap();
        assert_eq!(version.id, "fabric-loader-0.16.9-1.21.1");
        assert_eq!(version.inherits_from.as_deref(), Some("1.21.1"));
        assert_eq!(
            version.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(version.libraries.len(), 1);
        // downloads 字段缺失时应为空而非解析失败
        assert!(version.downloads.is_empty());
    }
}
