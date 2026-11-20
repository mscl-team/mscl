//! MSCL 平台适配层
//!
//! 封装所有与操作系统交互的操作，向核心层与桥接层屏蔽平台差异：
//!   - 进程启动与管理（阶段 3）
//!   - 文件系统路径解析（游戏目录、版本目录等）
//!   - Java 运行时检测
//!   - 平台信息（Windows / macOS / Android）识别
//!
//! 阶段 0 建立框架；阶段 3 加入 Java 检测与进程启动。

pub mod java;
pub mod natives;
pub mod process;

pub use java::{detect_java, parse_java_major, JavaRuntime};
pub use natives::extract_native_jars;
pub use process::{
    kill_process_by_pid, open_in_file_manager, GameProcess, OutputCallback, ProcessOutput,
};

use std::path::PathBuf;

/// 平台错误类型
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("不支持的操作系统: {0}")]
    UnsupportedOs(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("ZIP 解压错误: {0}")]
    Zip(#[from] zip::result::ZipError),
}

pub type Result<T> = std::result::Result<T, PlatformError>;

/// 当前运行的目标操作系统
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOs {
    Windows,
    MacOS,
    Linux,
    Android,
}

/// 检测当前操作系统
pub fn current_os() -> TargetOs {
    #[cfg(target_os = "windows")]
    return TargetOs::Windows;
    #[cfg(target_os = "macos")]
    return TargetOs::MacOS;
    #[cfg(target_os = "linux")]
    return TargetOs::Linux;
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    return TargetOs::Android;
}

impl TargetOs {
    /// 转换为 core 层的平台抽象，供 rules 过滤等纯逻辑使用。
    /// 这是 platform → core 的单向桥接，core 自身不感知具体 OS。
    pub fn to_core_platform(self) -> mscl_core::TargetPlatform {
        match self {
            TargetOs::Windows => mscl_core::TargetPlatform::Windows,
            TargetOs::MacOS => mscl_core::TargetPlatform::MacOs,
            TargetOs::Linux => mscl_core::TargetPlatform::Linux,
            TargetOs::Android => mscl_core::TargetPlatform::Android,
        }
    }
}

/// 便捷函数：直接获取当前平台对应的 core 平台抽象
pub fn current_core_platform() -> mscl_core::TargetPlatform {
    current_os().to_core_platform()
}

/// 获取默认游戏数据目录（遵循各系统约定）
/// - Windows: %APPDATA%/.mscl
/// - macOS: ~/Library/Application Support/MSCL
/// - Linux: $XDG_DATA_HOME/mscl，未设置时回退 ~/.local/share/mscl
/// - Android: ~/.mscl
pub fn default_data_dir() -> Result<PathBuf> {
    match current_os() {
        TargetOs::Windows => Ok(PathBuf::from(
            std::env::var("APPDATA").map_err(|_| PlatformError::UnsupportedOs(
                "无法读取 APPDATA 环境变量".to_string(),
            ))?,
        )
        .join(".mscl")),
        TargetOs::MacOS => Ok(PathBuf::from(std::env::var("HOME").map_err(|_| {
            PlatformError::UnsupportedOs("无法读取 HOME 环境变量".to_string())
        })?)
        .join("Library/Application Support/MSCL")),
        TargetOs::Linux => linux_data_dir(),
        TargetOs::Android => Ok(PathBuf::from(
            std::env::var("HOME")
                .map_err(|_| PlatformError::UnsupportedOs("无法读取 HOME 环境变量".to_string()))?,
        )
        .join(".mscl")),
    }
}

/// XDG Base Directory 规范的数据目录
/// 优先级：$XDG_DATA_HOME/mscl > ~/.local/share/mscl > $HOME/.mscl（极端兜底）
fn linux_data_dir() -> Result<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.trim().is_empty() {
            return Ok(PathBuf::from(xdg).join("mscl"));
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return Ok(PathBuf::from(home).join(".local/share/mscl"));
        }
    }
    Err(PlatformError::UnsupportedOs(
        "无法读取 XDG_DATA_HOME 或 HOME 环境变量".to_string(),
    ))
}

/// 供测试使用的纯函数：根据传入的 XDG_HOME / HOME 计算 Linux 数据目录
#[cfg(test)]
fn linux_data_dir_from(xdg_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    match xdg_home {
        Some(x) if !x.trim().is_empty() => Some(PathBuf::from(x).join("mscl")),
        _ => home
            .filter(|h| !h.trim().is_empty())
            .map(|h| PathBuf::from(h).join(".local/share/mscl")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_an_os() {
        let os = current_os();
        assert!(matches!(
            os,
            TargetOs::Windows | TargetOs::MacOS | TargetOs::Linux | TargetOs::Android
        ));
    }

    #[test]
    fn linux_data_dir_follows_xdg() {
        // XDG_DATA_HOME 优先
        assert_eq!(
            linux_data_dir_from(Some("/home/u/.local/share"), Some("/home/u")),
            Some(PathBuf::from("/home/u/.local/share/mscl"))
        );
        // XDG_DATA_HOME 为空时回退 HOME/.local/share
        assert_eq!(
            linux_data_dir_from(Some(""), Some("/home/u")),
            Some(PathBuf::from("/home/u/.local/share/mscl"))
        );
        // 自定义 XDG 路径
        assert_eq!(
            linux_data_dir_from(Some("/mnt/data/xdg"), None),
            Some(PathBuf::from("/mnt/data/xdg/mscl"))
        );
        // 两者都没有 → 错误路径
        assert_eq!(linux_data_dir_from(None, None), None);
    }
}
