//! 下载源（镜像）支持（阶段 9.1）
//!
//! Mojang 官方 CDN 在部分网络环境下不稳定，社区镜像（BMCLAPI）可显著提升速度。
//! 这一层只做纯字符串替换，不碰网络，便于单测与复用。
//!
//! 约定：任务规划阶段始终保存官方 URL，下载执行时才按当前源重写，
//! 这样切换源不需要重新规划任务，也能在镜像失败时回退官方。

/// Mojang 官方版本清单地址
pub const OFFICIAL_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// BMCLAPI 镜像根地址
const BMCLAPI_BASE: &str = "https://bmclapi2.bangbang93.com";

/// 下载源
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadSource {
    /// Mojang 官方
    Official,
    /// BMCLAPI 社区镜像（国内加速）
    Bmclapi,
    /// 自定义镜像根地址（如自建反代）
    Custom(String),
}

impl DownloadSource {
    /// 从设置中的字符串构造：空 / "official" → 官方，"bmclapi" → 镜像，其余视为自定义根地址
    pub fn from_id(id: &str) -> Self {
        match id.trim() {
            "" | "official" => Self::Official,
            "bmclapi" => Self::Bmclapi,
            other => Self::Custom(other.trim_end_matches('/').to_string()),
        }
    }

    /// 反向序列化为设置字符串
    pub fn id(&self) -> String {
        match self {
            Self::Official => "official".to_string(),
            Self::Bmclapi => "bmclapi".to_string(),
            Self::Custom(base) => base.clone(),
        }
    }

    /// 是否为官方源
    pub fn is_official(&self) -> bool {
        matches!(self, Self::Official)
    }

    /// 按当前源重写单条下载 URL；非 Mojang 域名的 URL 原样返回
    pub fn rewrite(&self, url: &str) -> String {
        match self {
            Self::Official => url.to_string(),
            Self::Bmclapi => rewrite_with_base(url, BMCLAPI_BASE),
            Self::Custom(base) => rewrite_with_base(url, base),
        }
    }

    /// 版本清单地址（同样走当前源）
    pub fn manifest_url(&self) -> String {
        self.rewrite(OFFICIAL_MANIFEST_URL)
    }
}

/// 把 Mojang 官方 URL 的 host 前缀替换为给定镜像根地址
///
/// 映射关系（与 BMCLAPI 文档一致）：
///   - piston-meta / launchermeta / launcher / piston-data → {base}
///   - resources.download.minecraft.net                  → {base}/assets
///   - libraries.minecraft.net                           → {base}/maven
fn rewrite_with_base(url: &str, base: &str) -> String {
    let base = base.trim_end_matches('/');

    // 元数据 / 启动器 / 资源索引等主机：直接换根
    const HOSTS: &[&str] = &[
        "https://piston-meta.mojang.com",
        "https://launchermeta.mojang.com",
        "https://launcher.mojang.com",
        "https://piston-data.mojang.com",
    ];
    for host in HOSTS {
        if let Some(rest) = url.strip_prefix(host) {
            return format!("{}{}", base, rest);
        }
    }

    // 资源对象 CDN
    if let Some(rest) = url.strip_prefix("https://resources.download.minecraft.net") {
        return format!("{}/assets{}", base, rest);
    }

    // Maven 库
    if let Some(rest) = url.strip_prefix("https://libraries.minecraft.net") {
        return format!("{}/maven{}", base, rest);
    }

    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_keeps_url_untouched() {
        let src = DownloadSource::from_id("official");
        assert_eq!(src, DownloadSource::Official);
        let url = "https://piston-meta.mojang.com/v1/packages/abc/26.3.json";
        assert_eq!(src.rewrite(url), url);
    }

    #[test]
    fn empty_id_defaults_to_official() {
        assert_eq!(DownloadSource::from_id(""), DownloadSource::Official);
        assert_eq!(DownloadSource::from_id("  "), DownloadSource::Official);
    }

    #[test]
    fn bmclapi_rewrites_all_known_hosts() {
        let src = DownloadSource::from_id("bmclapi");
        assert_eq!(
            src.rewrite("https://piston-meta.mojang.com/v1/packages/abc/26.3.json"),
            "https://bmclapi2.bangbang93.com/v1/packages/abc/26.3.json"
        );
        assert_eq!(
            src.rewrite("https://launchermeta.mojang.com/mc/game/version_manifest_v2.json"),
            "https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json"
        );
        assert_eq!(
            src.rewrite("https://launcher.mojang.com/v1/objects/abc/client.jar"),
            "https://bmclapi2.bangbang93.com/v1/objects/abc/client.jar"
        );
        assert_eq!(
            src.rewrite("https://piston-data.mojang.com/v1/objects/abc/client.jar"),
            "https://bmclapi2.bangbang93.com/v1/objects/abc/client.jar"
        );
        assert_eq!(
            src.rewrite("https://resources.download.minecraft.net/b6/b62ca8ec"),
            "https://bmclapi2.bangbang93.com/assets/b6/b62ca8ec"
        );
        assert_eq!(
            src.rewrite("https://libraries.minecraft.net/org/lwjgl/lwjgl/3.2.1/lwjgl.jar"),
            "https://bmclapi2.bangbang93.com/maven/org/lwjgl/lwjgl/3.2.1/lwjgl.jar"
        );
    }

    #[test]
    fn custom_base_rewrites_with_own_root() {
        let src = DownloadSource::from_id("https://mirror.example.com/");
        assert_eq!(
            src,
            DownloadSource::Custom("https://mirror.example.com".to_string())
        );
        assert_eq!(
            src.rewrite("https://libraries.minecraft.net/a/b.jar"),
            "https://mirror.example.com/maven/a/b.jar"
        );
        assert_eq!(
            src.rewrite("https://resources.download.minecraft.net/aa/bb"),
            "https://mirror.example.com/assets/aa/bb"
        );
    }

    #[test]
    fn non_mojang_urls_are_untouched() {
        let src = DownloadSource::from_id("bmclapi");
        let custom_launcher = "https://example.com/some/file.jar";
        assert_eq!(src.rewrite(custom_launcher), custom_launcher);
    }

    #[test]
    fn manifest_url_follows_source() {
        assert_eq!(
            DownloadSource::Official.manifest_url(),
            OFFICIAL_MANIFEST_URL
        );
        assert_eq!(
            DownloadSource::Bmclapi.manifest_url(),
            "https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json"
        );
    }

    #[test]
    fn id_roundtrip() {
        for id in ["official", "bmclapi", "https://mirror.example.com"] {
            assert_eq!(DownloadSource::from_id(id).id(), id);
        }
    }
}
