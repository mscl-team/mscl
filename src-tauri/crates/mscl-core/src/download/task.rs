//! 下载任务模型与规划逻辑（纯函数，可独立单测）
//!
//! 把 Version + AssetIndex 转化为一组扁平的 DownloadTask。
//! 这一层只决定"要下载什么、存到哪个相对路径、期望什么哈希"，
//! 不碰文件系统、不碰网络，由 engine 层消费。

use crate::model::{Artifact, Library, Version};
use crate::rules::TargetPlatform;
use std::collections::HashMap;

/// 下载资源的类别（便于进度展示与分目录存储）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadKind {
    ClientJar,
    Library,
    Native,
    Asset,
    AssetIndex,
    Logging,
}

/// 单个下载任务
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadTask {
    /// 下载源 URL
    pub url: String,
    /// 相对游戏根目录的存储路径（用 / 分隔，跨平台统一）
    pub relative_path: String,
    /// 期望的 SHA1（40 位十六进制）
    pub sha1: String,
    /// 期望字节数
    pub size: u64,
    pub kind: DownloadKind,
}

/// 资源索引模型 —— 对应 assetIndex JSON
/// 真实结构：{ "objects": { "icons/icon_128x128.png": {"hash":"...","size":9101} } }
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssetIndexFile {
    pub objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

/// 资源对象的存储路径规则：assets/objects/{hash前2位}/{完整hash}
/// 注意：文件名必须是完整 hash，不是去掉前两位后的剩余部分
pub fn asset_object_path(hash: &str) -> String {
    if hash.len() < 2 {
        return format!("assets/objects/{}", hash);
    }
    let prefix = &hash[..2];
    format!("assets/objects/{}/{}", prefix, hash)
}

impl DownloadTask {
    /// 从 Artifact 构造任务，path 为相对路径
    fn from_artifact(artifact: &Artifact, relative_path: String, kind: DownloadKind) -> Self {
        Self {
            url: artifact.url.clone(),
            relative_path,
            sha1: artifact.sha1.clone(),
            size: artifact.size,
            kind,
        }
    }

    /// client.jar 的规划：存到 versions/{id}/{id}.jar
    ///
    /// 继承版本（Fabric / Forge）复用父版本（原版）的 client.jar，
    /// 路径必须指向父版本目录，否则 classpath 会找不到。
    pub fn plan_client(version: &Version) -> Option<Self> {
        let artifact = version.client_download()?;
        let source_id = version.client_jar_from.as_deref().unwrap_or(&version.id);
        Some(Self::from_artifact(
            artifact,
            format!("versions/{0}/{0}.jar", source_id),
            DownloadKind::ClientJar,
        ))
    }

    /// 资源索引 JSON 的规划：存到 assets/indexes/{id}.json
    pub fn plan_asset_index(version: &Version) -> Option<Self> {
        let idx = version.asset_index.as_ref()?;
        Some(Self {
            url: idx.url.clone(),
            relative_path: format!("assets/indexes/{}.json", idx.id),
            sha1: idx.sha1.clone(),
            size: idx.size,
            kind: DownloadKind::AssetIndex,
        })
    }

    /// 日志配置的规划：存到 versions/{id}/logging.xml
    pub fn plan_logging(version: &Version) -> Option<Self> {
        let client = version.logging.as_ref()?.client.as_ref()?;
        let file = client.file.clone();
        Some(Self {
            url: file.url,
            relative_path: format!("versions/{}/logging.xml", version.id),
            sha1: file.sha1,
            size: file.size,
            kind: DownloadKind::Logging,
        })
    }
}

/// 规划某个 library 需要下载的文件（主 artifact + 平台相关的 natives classifier）
///
/// 逻辑：
///   1. 若有主 artifact（artifact 存在），下载并存入 libraries/{artifact.path}
///   2. 若 library 定义了 natives 映射，取当前 os 对应的 classifier 名，
///      从 downloads.classifiers 中取出对应产物下载（作为 Native）
pub fn plan_library(lib: &Library, platform: TargetPlatform) -> Vec<DownloadTask> {
    let mut tasks = Vec::new();

    if let Some(artifact) = &lib.downloads.artifact {
        tasks.push(DownloadTask::from_artifact(
            artifact,
            format!("libraries/{}", artifact.path.as_deref().unwrap_or_else(|| {
                // 极端兜底：无 path 时用 library name 推导，避免空路径
                &lib.name
            })),
            DownloadKind::Library,
        ));
    } else if let Some((url, path)) = maven_artifact(lib) {
        // Fabric / 旧版本只给 name + url：按 Maven 规则自行拼 URL 与路径。
        // 这类库没有官方 sha1/size，校验降级为"存在且非空"。
        tasks.push(DownloadTask {
            url,
            relative_path: format!("libraries/{}", path),
            sha1: String::new(),
            size: 0,
            kind: DownloadKind::Library,
        });
    }

    if let (Some(natives), Some(classifiers)) = (&lib.natives, &lib.downloads.classifiers) {
        if let Some(classifier_name) = natives.get(platform.os_name()) {
            if let Some(native_artifact) = classifiers.get(classifier_name) {
                tasks.push(DownloadTask::from_artifact(
                    native_artifact,
                    format!("libraries/{}", native_artifact.path.as_deref().unwrap_or(&lib.name)),
                    DownloadKind::Native,
                ));
            }
        }
    }

    tasks
}

/// 根据 Maven 坐标 + 仓库地址推导下载 URL 与相对路径
///
/// 用于没有 `downloads.artifact` 的库（Fabric profile、部分旧版本）。
/// 坐标格式：group:artifact:version[:classifier]
fn maven_artifact(lib: &Library) -> Option<(String, String)> {
    let base = lib.url.as_deref()?.trim_end_matches('/');
    if base.is_empty() {
        return None;
    }

    let mut parts = lib.name.split(':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    let version = parts.next()?;
    let classifier = parts.next().filter(|c| !c.is_empty());

    let file_name = match classifier {
        Some(c) => format!("{}-{}-{}.jar", artifact, version, c),
        None => format!("{}-{}.jar", artifact, version),
    };
    let path = format!(
        "{}/{}/{}/{}",
        group.replace('.', "/"),
        artifact,
        version,
        file_name
    );

    Some((format!("{}/{}", base, path), path))
}

/// 规划所有 libraries 的下载任务（先做 rules 过滤）
pub fn plan_all_libraries(version: &Version, platform: TargetPlatform) -> Vec<DownloadTask> {
    version
        .applicable_libraries(platform)
        .into_iter()
        .flat_map(|lib| plan_library(lib, platform))
        .collect()
}

/// 需要解压出 jar 的动态库文件扩展名（按平台）
///
/// 老版本 Minecraft（如 1.16.5）把 LWJGL 原生库放在 `natives-*` classifier jar 里，
/// 启动前必须解压到 java.library.path 指向的目录。
/// 现代版本（26.x）已将原生库作为普通 artifact 提供并走 classpath，
/// 此时 plan_native_jars 返回空列表，解压步骤自然跳过。
pub fn native_library_extensions(platform: TargetPlatform) -> &'static [&'static str] {
    match platform {
        TargetPlatform::Windows => &["dll"],
        TargetPlatform::MacOs => &["dylib", "so"],
        TargetPlatform::Linux | TargetPlatform::Android => &["so"],
    }
}

/// 规划需要解压的 native jar（返回相对 game_root 的路径）
///
/// 纯逻辑：只筛选 kind=Native 的下载任务，不碰文件系统。
/// platform 层拿到这些路径后读取 jar 并解压动态库。
pub fn plan_native_jars(version: &Version, platform: TargetPlatform) -> Vec<String> {
    plan_all_libraries(version, platform)
        .into_iter()
        .filter(|t| t.kind == DownloadKind::Native)
        .map(|t| t.relative_path)
        .collect()
}

/// 从资源索引规划所有资源文件下载任务
/// 资源 URL 规则固定，直接在此拼好，engine 层无需特殊处理
pub fn plan_assets(index: &AssetIndexFile) -> Vec<DownloadTask> {
    index
        .objects
        .values()
        .map(|obj| DownloadTask {
            url: asset_download_url(&obj.hash),
            relative_path: asset_object_path(&obj.hash),
            sha1: obj.hash.clone(),
            size: obj.size,
            kind: DownloadKind::Asset,
        })
        .collect()
}

/// 统一规划某个版本的完整下载任务：
/// client + logging + 资源索引 + libraries + 资源对象。
///
/// 调用方需先下载并解析 asset index（资源列表来源于它）；传 None 时只规划其余部分，
/// 用于"索引还没下载下来"的首次安装前阶段。
pub fn plan_version_tasks(
    version: &Version,
    asset_index: Option<&AssetIndexFile>,
    platform: TargetPlatform,
) -> Vec<DownloadTask> {
    let mut tasks = Vec::new();
    if let Some(t) = DownloadTask::plan_client(version) {
        tasks.push(t);
    }
    if let Some(t) = DownloadTask::plan_logging(version) {
        tasks.push(t);
    }
    if let Some(t) = DownloadTask::plan_asset_index(version) {
        tasks.push(t);
    }
    tasks.extend(plan_all_libraries(version, platform));
    if let Some(index) = asset_index {
        tasks.extend(plan_assets(index));
    }
    tasks
}

/// 资源 CDN URL 拼接（Mojang 官方 resources 下载规则）
pub fn asset_download_url(hash: &str) -> String {
    format!(
        "https://resources.download.minecraft.net/{}/{}",
        &hash[..2.min(hash.len())],
        hash
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_object_path_splits_hash() {
        assert_eq!(
            asset_object_path("b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356"),
            "assets/objects/b6/b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356"
        );
    }

    #[test]
    fn asset_download_url_format() {
        assert_eq!(
            asset_download_url("b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356"),
            "https://resources.download.minecraft.net/b6/b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356"
        );
    }

    #[test]
    fn plans_library_with_native() {
        let json = r#"{
            "name": "org.lwjgl:lwjgl:3.2.1",
            "downloads": {
                "artifact": {"path":"org/lwjgl/lwjgl/3.2.1/lwjgl.jar","sha1":"aaa","size":100,"url":"http://a"},
                "classifiers": {
                    "natives-windows": {"path":"org/lwjgl/lwjgl/3.2.1/lwjgl-natives-windows.jar","sha1":"bbb","size":50,"url":"http://b"},
                    "natives-macos": {"path":"org/lwjgl/lwjgl/3.2.1/lwjgl-natives-macos.jar","sha1":"ccc","size":60,"url":"http://c"}
                }
            },
            "natives": {"windows":"natives-windows","osx":"natives-macos"},
            "rules": []
        }"#;
        let lib: Library = serde_json::from_str(json).unwrap();

        let win = plan_library(&lib, TargetPlatform::Windows);
        assert_eq!(win.len(), 2); // 主 jar + windows native
        assert_eq!(win[0].kind, DownloadKind::Library);
        assert_eq!(win[1].kind, DownloadKind::Native);
        assert!(win[1].relative_path.contains("natives-windows"));

        let mac = plan_library(&lib, TargetPlatform::MacOs);
        assert_eq!(mac.len(), 2);
        assert!(mac[1].relative_path.contains("natives-macos"));
    }

    #[test]
    fn native_extensions_by_platform() {
        assert_eq!(native_library_extensions(TargetPlatform::Windows), &["dll"]);
        assert_eq!(native_library_extensions(TargetPlatform::Linux), &["so"]);
        assert_eq!(native_library_extensions(TargetPlatform::Android), &["so"]);
        assert_eq!(native_library_extensions(TargetPlatform::MacOs), &["dylib", "so"]);
    }

    #[test]
    fn linux_native_classifier_selected() {
        // Linux 的 natives key 是 "linux"，必须能选中对应 classifier
        let json = r#"{
            "name": "org.lwjgl:lwjgl:3.2.1",
            "downloads": {
                "artifact": {"path":"org/lwjgl/lwjgl.jar","sha1":"a","size":1,"url":"u"},
                "classifiers": {
                    "natives-linux": {"path":"org/lwjgl/lwjgl-natives-linux.jar","sha1":"b","size":1,"url":"u"},
                    "natives-windows": {"path":"org/lwjgl/lwjgl-natives-windows.jar","sha1":"c","size":1,"url":"u"}
                }
            },
            "natives": {"linux":"natives-linux","windows":"natives-windows"},
            "rules": []
        }"#;
        let lib: Library = serde_json::from_str(json).unwrap();
        let linux = plan_library(&lib, TargetPlatform::Linux);
        assert_eq!(linux.len(), 2);
        assert!(linux[1].relative_path.contains("natives-linux"), "Linux 应选中 linux classifier");
    }

    #[test]
    fn plans_fabric_library_from_maven_coordinate() {
        let lib: Library = serde_json::from_str(
            r#"{
                "name": "net.fabricmc:fabric-loader:0.20.1",
                "url": "https://maven.fabricmc.net/"
            }"#,
        )
        .unwrap();

        let tasks = plan_library(&lib, TargetPlatform::Windows);
        assert_eq!(tasks.len(), 1);
        assert_eq!(
            tasks[0].url,
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.20.1/fabric-loader-0.20.1.jar"
        );
        assert_eq!(
            tasks[0].relative_path,
            "libraries/net/fabricmc/fabric-loader/0.20.1/fabric-loader-0.20.1.jar"
        );
        // 无官方哈希：留空，由校验层降级处理
        assert!(tasks[0].sha1.is_empty());
        assert_eq!(tasks[0].kind, DownloadKind::Library);
    }

    #[test]
    fn maven_classifier_goes_into_file_name() {
        let lib: Library = serde_json::from_str(
            r#"{
                "name": "org.example:demo:1.0:natives-linux",
                "url": "https://repo.example.com"
            }"#,
        )
        .unwrap();

        let tasks = plan_library(&lib, TargetPlatform::Linux);
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0]
            .relative_path
            .ends_with("demo-1.0-natives-linux.jar"));
    }

    #[test]
    fn library_without_downloads_or_url_is_skipped() {
        let lib: Library =
            serde_json::from_str(r#"{"name": "org.example:demo:1.0"}"#).unwrap();
        assert!(plan_library(&lib, TargetPlatform::Windows).is_empty());
    }

    #[test]
    fn plan_version_tasks_covers_all_parts() {
        let json = r#"{
            "id": "test",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "downloads": {
                "client": {"sha1":"c1","size":10,"url":"https://piston-data.mojang.com/c.jar"}
            },
            "assetIndex": {"id":"5","sha1":"i1","size":5,"url":"https://piston-meta.mojang.com/i.json"},
            "logging": {"client": {"argument":"-Dlog","type":"log4j2-xml","file":{"id":"l","sha1":"l1","size":3,"url":"https://piston-data.mojang.com/l.xml"}}},
            "libraries": [],
            "assets": "5"
        }"#;
        let version: Version = serde_json::from_str(json).unwrap();
        let index: AssetIndexFile =
            serde_json::from_str(r#"{"objects":{"a/b":{"hash":"aabb","size":7}}}"#).unwrap();

        let tasks = plan_version_tasks(&version, Some(&index), TargetPlatform::Windows);
        // client + logging + assetIndex + 1 asset（libraries 为空）
        assert_eq!(tasks.len(), 4);
        assert!(tasks.iter().any(|t| t.kind == DownloadKind::ClientJar));
        assert!(tasks.iter().any(|t| t.kind == DownloadKind::Logging));
        assert!(tasks.iter().any(|t| t.kind == DownloadKind::AssetIndex));
        assert!(tasks.iter().any(|t| t.kind == DownloadKind::Asset));
    }

    #[test]
    fn plans_assets() {
        let json = r#"{
            "objects": {
                "icons/icon.png": {"hash":"aabbccddee00112233445566778899aabbccddee","size":123}
            }
        }"#;
        let index: AssetIndexFile = serde_json::from_str(json).unwrap();
        let tasks = plan_assets(&index);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].kind, DownloadKind::Asset);
        assert_eq!(tasks[0].relative_path, "assets/objects/aa/aabbccddee00112233445566778899aabbccddee");
    }
}
