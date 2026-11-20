//! SHA1 与大小双重校验（阶段 2）
//!
//! 设计要点：
//!   1. 流式读取文件，避免把 client.jar（40MB+）整个加载进内存
//!   2. SHA1 + size 双重校验，任一不符即判定损坏
//!   3. 校验通过的文件在下载时可跳过（简单可靠的"缓存命中"，不做 Range 续传）

use sha1::{Digest, Sha1};
use std::io::Read;
use std::path::Path;

use super::task::DownloadTask;
use crate::error::CoreError;

/// 校验结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// 文件完好，可直接使用
    Valid,
    /// 文件不存在
    Missing,
    /// 文件损坏（哈希或大小不符）
    Corrupted { reason: String },
}

/// 流式计算文件的 SHA1（十六进制小写）
pub fn sha1_of_file(path: &Path) -> Result<String, CoreError> {
    let mut file = std::fs::File::open(path).map_err(|e| {
        CoreError::Checksum(format!("无法打开文件 {}: {}", path.display(), e))
    })?;

    let mut hasher = Sha1::new();
    let mut buf = [0u8; 64 * 1024]; // 64KB 缓冲
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| CoreError::Checksum(format!("读取文件失败 {}: {}", path.display(), e)))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// 校验单个文件：SHA1 + size 双重校验
pub fn verify_file(
    path: &Path,
    expected_sha1: &str,
    expected_size: u64,
) -> Result<VerifyOutcome, CoreError> {
    if !path.exists() {
        return Ok(VerifyOutcome::Missing);
    }

    // 先查大小，不符直接判定损坏（快速失败，省去哈希计算）
    let actual_size = std::fs::metadata(path)
        .map_err(|e| CoreError::Checksum(format!("读取元数据失败 {}: {}", path.display(), e)))?
        .len();

    // 无哈希来源（Fabric Maven 库等）：降级为"存在且非空"
    if expected_sha1.is_empty() {
        if actual_size == 0 {
            return Ok(VerifyOutcome::Corrupted {
                reason: "文件为空".to_string(),
            });
        }
        if expected_size > 0 && actual_size != expected_size {
            return Ok(VerifyOutcome::Corrupted {
                reason: format!(
                    "大小不符: 期望 {} 字节, 实际 {} 字节",
                    expected_size, actual_size
                ),
            });
        }
        return Ok(VerifyOutcome::Valid);
    }
    if actual_size != expected_size {
        return Ok(VerifyOutcome::Corrupted {
            reason: format!(
                "大小不符: 期望 {} 字节, 实际 {} 字节",
                expected_size, actual_size
            ),
        });
    }

    let actual_sha1 = sha1_of_file(path)?;
    if !actual_sha1.eq_ignore_ascii_case(expected_sha1) {
        return Ok(VerifyOutcome::Corrupted {
            reason: format!(
                "SHA1 不符: 期望 {}, 实际 {}",
                expected_sha1, actual_sha1
            ),
        });
    }

    Ok(VerifyOutcome::Valid)
}

/// 批量校验报告（供"校验 / 修复"功能使用）
#[derive(Debug, Clone, Default)]
pub struct VerifyReport {
    pub total: usize,
    pub valid: usize,
    pub missing: usize,
    pub corrupted: usize,
    /// 缺失文件相对路径（最多保留前 50 条，避免 IPC 数据过大）
    pub missing_paths: Vec<String>,
    /// 损坏文件相对路径（最多保留前 50 条）
    pub corrupted_paths: Vec<String>,
    /// 修复需要下载的总字节数
    pub bytes_to_download: u64,
}

impl VerifyReport {
    /// 是否全部完好
    pub fn is_healthy(&self) -> bool {
        self.missing == 0 && self.corrupted == 0
    }
}

/// 批量校验任务列表对应的本地文件
pub fn verify_tasks(root: &Path, tasks: &[DownloadTask]) -> Result<VerifyReport, CoreError> {
    const MAX_LISTED: usize = 50;
    let mut report = VerifyReport {
        total: tasks.len(),
        ..Default::default()
    };

    for task in tasks {
        let path = root.join(&task.relative_path);
        match verify_file(&path, &task.sha1, task.size)? {
            VerifyOutcome::Valid => report.valid += 1,
            VerifyOutcome::Missing => {
                report.missing += 1;
                report.bytes_to_download += task.size;
                if report.missing_paths.len() < MAX_LISTED {
                    report.missing_paths.push(task.relative_path.clone());
                }
            }
            VerifyOutcome::Corrupted { .. } => {
                report.corrupted += 1;
                report.bytes_to_download += task.size;
                if report.corrupted_paths.len() < MAX_LISTED {
                    report.corrupted_paths.push(task.relative_path.clone());
                }
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::DownloadKind;
    use std::io::Write;

    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn temp_file(content: &[u8]) -> std::path::PathBuf {
        // 用原子计数器生成唯一文件名，避免并发测试互相冲突
        let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "mscl_verify_test_{}_{}_{}.bin",
            std::process::id(),
            seq,
            content.len()
        ));
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content).unwrap();
        path
    }

    #[test]
    fn sha1_known_content() {
        // "abc" 的 SHA1 是已知值
        let path = temp_file(b"abc");
        assert_eq!(
            sha1_of_file(&path).unwrap(),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn verify_valid_file() {
        let path = temp_file(b"abc");
        let outcome = verify_file(
            &path,
            "a9993e364706816aba3e25717850c26c9cd0d89d",
            3,
        )
        .unwrap();
        assert_eq!(outcome, VerifyOutcome::Valid);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn verify_detects_wrong_size() {
        let path = temp_file(b"abc");
        let outcome = verify_file(
            &path,
            "a9993e364706816aba3e25717850c26c9cd0d89d",
            999, // 错误的大小
        )
        .unwrap();
        assert!(matches!(outcome, VerifyOutcome::Corrupted { .. }));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn verify_detects_wrong_hash() {
        let path = temp_file(b"abc");
        let outcome = verify_file(&path, "0000000000000000000000000000000000000000", 3).unwrap();
        assert!(matches!(outcome, VerifyOutcome::Corrupted { .. }));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn verify_missing_file() {
        let path = std::env::temp_dir().join("mscl_definitely_missing_file_xyz.bin");
        let outcome = verify_file(&path, "abc", 3).unwrap();
        assert_eq!(outcome, VerifyOutcome::Missing);
    }

    #[test]
    fn verify_without_hash_only_requires_non_empty() {
        let good = temp_file(b"content");
        assert_eq!(verify_file(&good, "", 0).unwrap(), VerifyOutcome::Valid);

        let empty = temp_file(b"");
        assert!(matches!(
            verify_file(&empty, "", 0).unwrap(),
            VerifyOutcome::Corrupted { .. }
        ));

        std::fs::remove_file(&good).ok();
        std::fs::remove_file(&empty).ok();
    }

    #[test]
    fn verify_tasks_reports_all_states() {
        let dir = std::env::temp_dir().join(format!("mscl_verify_tasks_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("good.bin"), b"abc").unwrap();
        std::fs::write(dir.join("bad.bin"), b"xyz").unwrap();

        let good_sha1 = "a9993e364706816aba3e25717850c26c9cd0d89d"; // "abc"
        let tasks = vec![
            DownloadTask {
                url: "u".into(),
                relative_path: "good.bin".into(),
                sha1: good_sha1.into(),
                size: 3,
                kind: DownloadKind::Library,
            },
            DownloadTask {
                url: "u".into(),
                relative_path: "bad.bin".into(),
                sha1: good_sha1.into(),
                size: 3,
                kind: DownloadKind::Library,
            },
            DownloadTask {
                url: "u".into(),
                relative_path: "missing.bin".into(),
                sha1: "x".into(),
                size: 9,
                kind: DownloadKind::Asset,
            },
        ];

        let report = verify_tasks(&dir, &tasks).unwrap();
        assert_eq!(report.total, 3);
        assert_eq!(report.valid, 1);
        assert_eq!(report.corrupted, 1);
        assert_eq!(report.missing, 1);
        assert_eq!(report.bytes_to_download, 12);
        assert!(!report.is_healthy());
        assert_eq!(report.corrupted_paths, vec!["bad.bin".to_string()]);
        assert_eq!(report.missing_paths, vec!["missing.bin".to_string()]);

        std::fs::remove_dir_all(&dir).ok();
    }
}
