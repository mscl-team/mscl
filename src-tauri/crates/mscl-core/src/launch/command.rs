//! 启动命令构建（阶段 3 核心）
//!
//! 流程：
//!   1. 解析 jvm / game 参数（按平台过滤）
//!   2. 构建 token 集合并替换所有 ${}
//!   3. 组装命令：java路径 + jvm参数 + 主类 + game参数
//!
//! 注意：这里只构建"参数列表"，实际进程启动交给 platform 层。
//! 保持 core 纯粹——不执行进程，不碰环境变量探测。

use std::path::PathBuf;

use super::args::{game_arguments, jvm_arguments};
use super::classpath::build_classpath;
use super::tokens::{self, TokenResolver, Tokens};
use crate::error::CoreError;
use crate::model::Version;
use crate::rules::TargetPlatform;

/// 离线账号信息（阶段 1 认证的占位，后续会替换为微软认证）
#[derive(Debug, Clone)]
pub struct Account {
    pub username: String,
    pub uuid: String,
    /// 离线模式 access token 固定值
    pub access_token: String,
}

impl Account {
    /// 创建离线账号（uuid 用基于用户名的确定性值，玩家皮肤会缺失但能进游戏）
    pub fn offline(username: &str) -> Self {
        // 离线模式标准做法：uuid = 基于名字的离线 UUID，token 固定
        let uuid = offline_uuid(username);
        Self {
            username: username.to_string(),
            uuid,
            access_token: "00000000000000000000000000000000".to_string(),
        }
    }
}

/// 生成离线UUID（简化版：基于用户名的哈希前32位格式化，非严格UUID v3但足够唯一）
fn offline_uuid(username: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(format!("OfflinePlayer:{}", username).as_bytes());
    let hash = hasher.finalize();
    let hex = hex::encode(hash);
    let h = &hex[..32];
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32]
    )
}

/// 启动配置（调用方提供的所有外部信息）
#[derive(Debug, Clone)]
pub struct LaunchConfig {
    /// Java 可执行文件绝对路径（由 platform 层检测提供）
    pub java_path: PathBuf,
    /// 游戏根目录（.mscl 下的共享根：libraries / assets / versions）
    pub game_root: PathBuf,
    /// 实例数据目录（saves / mods / config / options.txt）。
    /// None 表示与 game_root 相同（不隔离，全局共享存档）。
    pub game_dir: Option<PathBuf>,
    /// 资源根目录（assets）
    pub assets_root: PathBuf,
    /// natives 解压目录
    pub natives_dir: PathBuf,
    /// 分配的最大内存 MB
    pub max_memory_mb: u32,
    /// 窗口宽度
    pub window_width: u32,
    /// 窗口高度
    pub window_height: u32,
    /// 启动器名称（写入 token）
    pub launcher_name: String,
    pub launcher_version: String,
}

/// 构建好的启动命令
#[derive(Debug, Clone)]
pub struct LaunchCommand {
    /// 可执行文件（java 路径）
    pub executable: PathBuf,
    /// 完整参数列表
    pub args: Vec<String>,
    /// 工作目录
    pub working_dir: PathBuf,
}

impl LaunchCommand {
    /// 生成可直接显示/复制的命令行字符串（用于 UI 展示与调试）
    pub fn to_command_line(&self) -> String {
        let mut parts = vec![format!("\"{}\"", self.executable.to_string_lossy())];
        parts.extend(self.args.iter().map(|a| {
            if a.contains(' ') || a.contains(';') {
                format!("\"{}\"", a)
            } else {
                a.clone()
            }
        }));
        parts.join(" ")
    }
}

/// 构建完整启动命令
pub fn build_launch_command(
    version: &Version,
    config: &LaunchConfig,
    account: &Account,
    platform: TargetPlatform,
) -> Result<LaunchCommand, CoreError> {
    // 1. 构建 classpath
    let classpath = build_classpath(version, &config.game_root, platform);

    // 实例目录：开启版本隔离时是 instances/{id}，否则与共享根相同
    let game_dir = config
        .game_dir
        .clone()
        .unwrap_or_else(|| config.game_root.clone());

    // 2. 构建 token 集合
    let mut tokens: Tokens = Tokens::new();
    tokens.insert(tokens::TOKEN_NATIVES_DIRECTORY.into(), config.natives_dir.to_string_lossy().into_owned());
    tokens.insert(tokens::TOKEN_LAUNCHER_NAME.into(), config.launcher_name.clone());
    tokens.insert(tokens::TOKEN_LAUNCHER_VERSION.into(), config.launcher_version.clone());
    tokens.insert(tokens::TOKEN_CLASSPATH.into(), classpath);
    tokens.insert(tokens::TOKEN_AUTH_PLAYER_NAME.into(), account.username.clone());
    tokens.insert(tokens::TOKEN_VERSION_NAME.into(), version.id.clone());
    tokens.insert(tokens::TOKEN_GAME_DIRECTORY.into(), game_dir.to_string_lossy().into_owned());
    // Forge 1.13+ 的 JVM 参数会用到这两个占位符
    tokens.insert(
        "library_directory".into(),
        config
            .game_root
            .join("libraries")
            .to_string_lossy()
            .into_owned(),
    );
    tokens.insert(
        "classpath_separator".into(),
        super::classpath::classpath_separator(platform).to_string(),
    );
    tokens.insert(tokens::TOKEN_ASSETS_ROOT.into(), config.assets_root.to_string_lossy().into_owned());
    let asset_id = version.asset_index.as_ref().map(|a| a.id.clone()).unwrap_or_else(|| version.assets.clone().unwrap_or_default());
    tokens.insert(tokens::TOKEN_ASSETS_INDEX_NAME.into(), asset_id);
    tokens.insert(tokens::TOKEN_AUTH_UUID.into(), account.uuid.clone());
    tokens.insert(tokens::TOKEN_AUTH_ACCESS_TOKEN.into(), account.access_token.clone());
    tokens.insert(tokens::TOKEN_CLIENT_ID.into(), String::new()); // 微软认证才有 clientid，离线留空
    tokens.insert(tokens::TOKEN_AUTH_XUID.into(), String::new()); // 离线无 xuid
    // 老版本（1.16.5 及以前）用 ${user_type}，标准离线值为 "mscl"/"legacy"
    tokens.insert(tokens::TOKEN_USER_TYPE.into(), "mscl".to_string());
    tokens.insert(tokens::TOKEN_VERSION_TYPE.into(), version.version_type.as_str().into());
    // 分辨率（虽然 we don't enable has_custom_resolution feature，仍提供以防万一）
    tokens.insert("resolution_width".into(), config.window_width.to_string());
    tokens.insert("resolution_height".into(), config.window_height.to_string());

    let resolver = TokenResolver::new(tokens);

    // 3. 解析并替换 jvm 参数
    let jvm_raw = jvm_arguments(version, platform);
    let mut args = resolver.resolve_all(&jvm_raw)?;

    // 4. 插入内存参数 -Xmx（放在 jvm 参数里，若已有则不重复）
    if !args.iter().any(|a| a.starts_with("-Xmx")) {
        args.insert(0, format!("-Xmx{}M", config.max_memory_mb));
    }

    // 5. 主类
    args.push(version.main_class.clone());

    // 6. game 参数
    let game_raw = game_arguments(version, platform);
    args.extend(resolver.resolve_all(&game_raw)?);

    Ok(LaunchCommand {
        executable: config.java_path.clone(),
        args,
        // 工作目录必须与 --gameDir 一致，否则相对路径（如 logs/）会落在别处
        working_dir: game_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::VersionType;

    fn sample_version() -> Version {
        serde_json::from_str(r#"{
            "id": "26.3",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "arguments": {
                "jvm": ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"],
                "game": ["--username", "${auth_player_name}", "--version", "${version_name}"]
            },
            "downloads": {"client": {"path":"c","sha1":"s","size":1,"url":"u"}},
            "assetIndex": {"id":"34","sha1":"s","size":1,"url":"u"},
            "libraries": []
        }"#).unwrap()
    }

    fn sample_config() -> LaunchConfig {
        LaunchConfig {
            java_path: PathBuf::from("C:/java/bin/java.exe"),
            game_root: PathBuf::from("C:/mscl/game"),
            game_dir: None,
            assets_root: PathBuf::from("C:/mscl/assets"),
            natives_dir: PathBuf::from("C:/mscl/natives"),
            max_memory_mb: 2048,
            window_width: 1280,
            window_height: 720,
            launcher_name: "MSCL".into(),
            launcher_version: "0.1.0".into(),
        }
    }

    #[test]
    fn builds_command_with_tokens_resolved() {
        let version = sample_version();
        let cmd = build_launch_command(
            &version, &sample_config(), &Account::offline("Steve"), TargetPlatform::Windows,
        ).unwrap();

        assert_eq!(cmd.executable, PathBuf::from("C:/java/bin/java.exe"));
        assert!(cmd.args.contains(&"-Xmx2048M".to_string()));
        assert!(cmd.args.contains(&"net.minecraft.client.main.Main".to_string()));
        assert!(cmd.args.contains(&"Steve".to_string()));
        // 所有占位符都应被替换
        for arg in &cmd.args {
            assert!(!arg.contains("${"), "残留占位符: {}", arg);
        }
    }

    #[test]
    fn offline_uuid_is_deterministic() {
        let a = offline_uuid("Steve");
        let b = offline_uuid("Steve");
        let c = offline_uuid("Alex");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 36, "UUID 应为36字符");
    }

    #[test]
    fn version_type_as_str() {
        assert_eq!(VersionType::Release.as_str(), "release");
        assert_eq!(VersionType::Snapshot.as_str(), "snapshot");
    }

    fn version_with_game_dir_arg() -> Version {
        serde_json::from_str(r#"{
            "id": "26.3",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "arguments": {
                "jvm": ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"],
                "game": ["--gameDir", "${game_directory}", "--username", "${auth_player_name}"]
            },
            "downloads": {"client": {"path":"c","sha1":"s","size":1,"url":"u"}},
            "assetIndex": {"id":"34","sha1":"s","size":1,"url":"u"},
            "libraries": []
        }"#).unwrap()
    }

    #[test]
    fn isolated_game_dir_drives_working_dir_and_token() {
        let version = version_with_game_dir_arg();
        let mut config = sample_config();
        config.game_dir = Some(PathBuf::from("C:/mscl/instances/26.3"));

        let cmd = build_launch_command(
            &version, &config, &Account::offline("Steve"), TargetPlatform::Windows,
        ).unwrap();

        assert_eq!(cmd.working_dir, PathBuf::from("C:/mscl/instances/26.3"));
        let idx = cmd
            .args
            .iter()
            .position(|a| a == "--gameDir")
            .expect("应包含 --gameDir");
        assert_eq!(cmd.args[idx + 1], "C:/mscl/instances/26.3");
    }

    #[test]
    fn shared_mode_falls_back_to_game_root() {
        let version = version_with_game_dir_arg();
        let config = sample_config(); // game_dir = None

        let cmd = build_launch_command(
            &version, &config, &Account::offline("Steve"), TargetPlatform::Windows,
        ).unwrap();

        assert_eq!(cmd.working_dir, config.game_root);
        let idx = cmd.args.iter().position(|a| a == "--gameDir").unwrap();
        assert_eq!(cmd.args[idx + 1], config.game_root.to_string_lossy());
    }
}
