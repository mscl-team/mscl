//! 阶段 3 真实进程验证：启动一个真实的 java 进程，验证输出捕获
//! 不启动游戏，只验证 platform 层进程管理能力

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mscl_platform::{detect_java, GameProcess, ProcessOutput};

#[test]
fn spawns_real_java_and_captures_output() {
    let java = match detect_java() {
        Ok(j) => j,
        Err(e) => {
            eprintln!("[skip] 无 Java 可用: {}", e);
            return;
        }
    };

    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let lines_clone = lines.clone();

    // 用 -version 让它输出版本信息到 stderr 后退出
    let proc = GameProcess::spawn(
        &java.path,
        &["-version".to_string()],
        &std::env::temp_dir(),
        Some(Arc::new(move |out| {
            let text = match out {
                ProcessOutput::Stdout(s) => format!("OUT: {}", s),
                ProcessOutput::Stderr(s) => format!("ERR: {}", s),
            };
            lines_clone.lock().unwrap().push(text);
        })),
    )
    .expect("应能启动 java 进程");

    assert!(proc.id() > 0, "应有有效 PID");

    // 等待退出
    let code = proc.wait().expect("应能等待进程");
    assert_eq!(code, 0, "java -version 应正常退出");

    // 等输出线程收完
    std::thread::sleep(std::time::Duration::from_millis(200));

    let captured = lines.lock().unwrap();
    let all = captured.join("\n");
    assert!(all.contains("version") || all.contains("openjdk"), "应捕获到版本输出: {}", all);
    assert!(all.contains("ERR:"), "java -version 走 stderr");

    println!("[ok] 真实 java 进程启动成功, 捕获 {} 行输出: {}", captured.len(), all.lines().next().unwrap_or(""));
}

#[test]
fn spawn_with_working_directory() {
    let java = match detect_java() { Ok(j) => j, Err(_) => return };
    let work = std::env::temp_dir();
    // 仅验证能带工作目录启动（用 java -version 快速退出）
    let proc = GameProcess::spawn(
        &java.path,
        &["-version".to_string()],
        &work,
        None,
    ).expect("应能启动");
    let code = proc.wait().unwrap();
    assert_eq!(code, 0);
    let _ = PathBuf::from("unused");
    println!("[ok] 工作目录参数正常");
}
