//! Java 运行时检测（阶段 3）
//!
//! 策略（方案1：自动检测系统 Java）：
//!   1. 优先 JAVA_HOME 环境变量
//!   2. PATH 中查找 java/javaw
//!   3. 常见安装路径（Windows: Program Files；macOS: /Library/Java）
//!
//! 检测到后运行 `java -version` 解析主版本号，供版本兼容性提示。

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{PlatformError, Result, TargetOs};

/// 检测到的 Java 运行时
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaRuntime {
    /// java 可执行文件绝对路径
    pub path: PathBuf,
    /// 主版本号（如 21、25），None 表示无法解析
    pub major_version: Option<u32>,
    /// 完整版本字符串
    pub version_output: String,
}

/// 检测系统 Java（返回找到的第一个可用运行时）
pub fn detect_java() -> Result<JavaRuntime> {
    let candidates = java_candidates();

    for candidate in candidates {
        if !is_executable(&candidate) {
            continue;
        }
        if let Some(info) = probe_version(&candidate) {
            return Ok(JavaRuntime {
                path: candidate,
                major_version: info.0,
                version_output: info.1,
            });
        }
    }

    Err(PlatformError::UnsupportedOs(
        "未找到可用的 Java 运行时，请安装 JDK 25 或设置 JAVA_HOME".to_string(),
    ))
}

/// 收集候选 java 路径（按优先级）
fn java_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    // 1. JAVA_HOME
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let home = PathBuf::from(java_home);
        candidates.push(home.join("bin").join(java_exe_name()));
    }

    // 2. PATH 中查找
    if let Ok(path_var) = std::env::var("PATH") {
        let separator = if cfg!(windows) { ';' } else { ':' };
        for dir in path_var.split(separator) {
            candidates.push(PathBuf::from(dir).join(java_exe_name()));
        }
    }

    // 3. 常见安装路径
    match crate::current_os() {
        TargetOs::Windows => {
            if let Ok(pf) = std::env::var("ProgramFiles") {
                // 扫描 Program Files 下的 jdk-* 目录
                if let Ok(entries) = std::fs::read_dir(&pf) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.to_lowercase().starts_with("jdk") || name.to_lowercase().starts_with("jre") {
                            candidates.push(entry.path().join("bin").join(java_exe_name()));
                        }
                    }
                }
            }
        }
        TargetOs::MacOS => {
            // macOS 的 JDK 布局是 /Library/Java/JavaVirtualMachines/<jdk>/Contents/Home/bin/java
            for root in ["/Library/Java/JavaVirtualMachines", "/opt/homebrew/opt", "/usr/local/opt"] {
                if let Ok(entries) = std::fs::read_dir(root) {
                    for entry in entries.flatten() {
                        if !entry.path().is_dir() {
                            continue;
                        }
                        // 标准 JDK 布局：<jdk>/Contents/Home/bin/java
                        candidates.push(
                            entry.path().join("Contents/Home/bin").join(java_exe_name()),
                        );
                        // Homebrew openjdk 布局：<opt>/openjdk/bin/java
                        candidates.push(entry.path().join("bin").join(java_exe_name()));
                    }
                }
            }
            candidates.push(PathBuf::from("/usr/bin/java"));
        }
        TargetOs::Linux => {
            // 发行版常见 JDK 安装位置：
            // - /usr/lib/jvm/<distro>/bin/java（Debian/Ubuntu/Fedora 通用）
            // - /opt/java/<distro>/bin/java、/opt/jdk*/bin/java（手动安装 / sdkman 等）
            for root in ["/usr/lib/jvm", "/opt/java", "/opt/jdk"] {
                if let Ok(entries) = std::fs::read_dir(root) {
                    for entry in entries.flatten() {
                        if !entry.path().is_dir() {
                            continue;
                        }
                        // 直接子目录 bin/java
                        candidates.push(entry.path().join("bin").join(java_exe_name()));
                        // 部分布局多一层（如 /usr/lib/jvm/java-21-openjdk-amd64/bin/java 已是直接层，
                        // 这里再覆盖 /opt/java/jdk-21/bin/java 形式，属同一层，无需递归）
                    }
                }
            }
            // 常见直接路径（某些发行版 alternatives 链接）
            candidates.push(PathBuf::from("/usr/bin/java"));
            candidates.push(PathBuf::from("/usr/local/bin/java"));
        }
        _ => {}
    }

    // 去重（保留顺序）
    let mut seen = std::collections::HashSet::new();
    candidates.retain(|p| seen.insert(p.clone()));
    candidates
}

fn java_exe_name() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}

/// 判断路径是否为可执行文件
fn is_executable(path: &Path) -> bool {
    #[cfg(windows)]
    {
        path.exists() && path.is_file()
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        path.exists()
            && std::fs::metadata(path)
                .map(|m| m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
    }
}

/// 运行 `java -version` 解析版本号
/// 返回 (主版本号 Option, 完整输出)
fn probe_version(java_path: &Path) -> Option<(Option<u32>, String)> {
    let mut command = Command::new(java_path);
    command.arg("-version");

    // Windows 上必须隐藏控制台窗口：
    // 否则每次检测 Java（首页挂载时会调用）都会闪一下 cmd 黑框。
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command.output().ok()?;

    // java -version 输出到 stderr
    let text = String::from_utf8_lossy(&output.stderr).to_string();
    if text.is_empty() {
        return None;
    }

    // 解析 "java version \"1.8.0_xxx\"" 或 "java version \"25.0.1\""
    let major = parse_java_major(&text);
    Some((major, text.trim().to_string()))
}

/// 解析 Java 主版本号
/// Java 8 及以前是 "1.8.0"，Java 9+ 是 "17.0.1" / "25"
pub fn parse_java_major(version_text: &str) -> Option<u32> {
    // 找引号内的版本串
    let start = version_text.find('"')?;
    let rest = &version_text[start + 1..];
    let end = rest.find('"')?;
    let ver = &rest[..end];

    let parts: Vec<&str> = ver.split('.').collect();
    if parts.is_empty() {
        return None;
    }

    let first = parts[0].parse::<u32>().ok()?;
    if first == 1 && parts.len() >= 2 {
        // 旧格式 1.8.0 → 主版本 8
        parts[1].parse::<u32>().ok()
    } else {
        Some(first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_java_version() {
        assert_eq!(
            parse_java_major("java version \"25.0.3\" 2026-01-01\n"),
            Some(25)
        );
        assert_eq!(
            parse_java_major("openjdk version \"21.0.1\" 2023-10-17\n"),
            Some(21)
        );
    }

    #[test]
    fn parses_legacy_java_version() {
        assert_eq!(
            parse_java_major("java version \"1.8.0_312\"\n"),
            Some(8)
        );
    }

    #[test]
    fn detects_real_java_if_available() {
        // 本机环境探测（不强求成功，只验证不 panic）
        match detect_java() {
            Ok(java) => {
                println!("[ok] 检测到 Java: {:?} (版本 {:?})", java.path, java.major_version);
                assert!(java.path.exists());
            }
            Err(e) => println!("[skip] 未检测到 Java: {}", e),
        }
    }
}
