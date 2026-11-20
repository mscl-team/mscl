//! Native 库解压（阶段 6 · Linux 适配前置）
//!
//! 老版本 Minecraft（如 1.16.5）把 LWJGL 原生库放在 `natives-*` classifier jar 里，
//! 启动前必须把其中的动态库（.dll / .so / .dylib）解压到 java.library.path 指向的目录。
//! 现代版本已将原生库作为普通 artifact 走 classpath，此时调用方传入空列表即可跳过。
//!
//! 安全约束：
//!   - 只取 jar 内条目最后一段文件名写入，拒绝 `../` 之类的路径穿越
//!   - 只解压调用方指定扩展名的文件，跳过 META-INF 元数据与非库文件

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use crate::Result;

/// 记录已解压 native 集合的签名文件
const STAMP_FILE: &str = ".mscl-natives.stamp";

/// FNV-1a：稳定、无需额外依赖，仅用于判断"jar 集合是否变化"
fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

/// jar 集合签名：路径 + 大小 + 修改时间
fn signature_of(jars: &[PathBuf]) -> String {
    let mut buf = String::new();
    for jar in jars {
        match std::fs::metadata(jar) {
            Ok(meta) => {
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                buf.push_str(&format!("{}:{}:{}|", jar.display(), meta.len(), mtime));
            }
            Err(_) => buf.push_str(&format!("{}:missing|", jar.display())),
        }
    }
    format!("{:016x}", fnv1a(&buf))
}

/// 把多个 native jar 中的动态库解压到目标目录
///
/// 返回实际解压出的文件数量。同名文件后写入的会覆盖先写入的（正常版本不会冲突）。
///
/// 优化：jar 集合未变化时直接复用上次解压结果（老版本每次启动都解压会拖慢启动）。
pub fn extract_native_jars(
    jars: &[PathBuf],
    dest: &Path,
    extensions: &[&str],
) -> Result<usize> {
    if jars.is_empty() {
        return Ok(0);
    }

    std::fs::create_dir_all(dest)?;

    let signature = signature_of(jars);
    let stamp_path = dest.join(STAMP_FILE);
    if let Ok(previous) = std::fs::read_to_string(&stamp_path) {
        if previous.trim() == signature {
            tracing::debug!(dest = %dest.display(), "原生库签名未变化，跳过解压");
            return Ok(0);
        }
    }

    let mut extracted = 0usize;

    for jar_path in jars {
        if !jar_path.exists() {
            // 单个 jar 缺失不致命，跳过（下载阶段已校验，可能是现代版本无 native）
            tracing::warn!(path = %jar_path.display(), "native jar 不存在，跳过解压");
            continue;
        }

        let file = File::open(jar_path)?;
        let mut archive = ZipArchive::new(file)?;

        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            if entry.is_dir() {
                continue;
            }

            // 只取最后一段文件名，天然阻断路径穿越。
            // 提前转成 owned String，避免 entry 的不可变借用与后面的可变借用冲突。
            let file_name = {
                let raw_name = entry.name();
                match Path::new(raw_name).file_name().and_then(|n| n.to_str()) {
                    Some(name) => name.to_string(),
                    None => continue,
                }
            };
            if file_name.is_empty() || file_name.contains("..") {
                continue;
            }

            // 扩展名过滤（大小写不敏感）
            let ext = Path::new(&file_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !extensions.iter().any(|allowed| ext == *allowed) {
                continue;
            }

            let out_path = dest.join(&file_name);
            let mut out = File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out)?;
            out.flush()?;
            extracted += 1;
            tracing::debug!(file = %file_name, from = %jar_path.display(), "解压 native 库");
        }
    }

    // 解压成功后才写签名，避免中途失败被误判为"已完成"
    let _ = std::fs::write(&stamp_path, &signature);

    Ok(extracted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_temp_dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("mscl-natives-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn skips_extraction_when_jar_set_unchanged() {
        let dir = make_temp_dir("skip");
        let jar = dir.join("natives.jar");
        let out = dir.join("out");

        {
            let f = File::create(&jar).unwrap();
            let mut zip = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("libfoo.so", opts).unwrap();
            zip.write_all(b"SO").unwrap();
            zip.finish().unwrap();
        }

        let first = extract_native_jars(std::slice::from_ref(&jar), &out, &["so"]).unwrap();
        assert_eq!(first, 1);
        assert!(out.join("libfoo.so").exists());

        // 同一组 jar 再解压一次：签名一致，应跳过
        let second = extract_native_jars(std::slice::from_ref(&jar), &out, &["so"]).unwrap();
        assert_eq!(second, 0, "jar 未变化时应跳过解压");
        assert!(out.join("libfoo.so").exists(), "已解压文件应保留");
    }

    #[test]
    fn extracts_only_matching_extensions() {
        let dir = make_temp_dir("extract");
        let jar = dir.join("natives.jar");
        let out = dir.join("out");

        // 构造一个测试用 native jar
        {
            let f = File::create(&jar).unwrap();
            let mut zip = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("META-INF/MANIFEST.MF", opts).unwrap();
            zip.write_all(b"Manifest-Version: 1.0").unwrap();
            zip.start_file("org/lwjgl/liblwjgl.so", opts).unwrap();
            zip.write_all(b"SO-DATA").unwrap();
            zip.start_file("org/lwjgl/LWJGL.dll", opts).unwrap();
            zip.write_all(b"DLL-DATA").unwrap();
            zip.finish().unwrap();
        }

        let count = extract_native_jars(std::slice::from_ref(&jar), &out, &["so"]).unwrap();
        assert_eq!(count, 1, "只应解压 .so 文件");
        assert!(out.join("liblwjgl.so").exists());
        assert!(!out.join("LWJGL.dll").exists(), "不匹配扩展名的文件不应被解压");
        assert!(!out.join("MANIFEST.MF").exists(), "元数据不应被解压");
        assert_eq!(std::fs::read(out.join("liblwjgl.so")).unwrap(), b"SO-DATA");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn empty_jar_list_is_noop() {
        let dir = make_temp_dir("noop");
        let out = dir.join("out");
        assert_eq!(extract_native_jars(&[], &out, &["so"]).unwrap(), 0);
        assert!(!out.exists(), "空列表不应创建目录");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn skips_missing_jar() {
        let dir = make_temp_dir("missing");
        let out = dir.join("out");
        let fake = dir.join("not-exists.jar");
        assert_eq!(extract_native_jars(&[fake], &out, &["so"]).unwrap(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
