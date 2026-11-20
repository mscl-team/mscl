//! 进程启动与管理（阶段 3）
//!
//! 用 std::process 启动游戏，支持：
//!   - 工作目录设置
//!   - 输出捕获（stdout/stderr 通过线程实时读取，支持回调）
//!   - 进程状态查询、等待、终止

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use crate::{PlatformError, Result};

/// 运行中的游戏进程
pub struct GameProcess {
    child: Child,
    /// 仍在运行的输出读取线程数（归零表示所有输出已读完）
    readers_remaining: Arc<AtomicU8>,
}

/// 进程输出行（供 UI 显示日志）
#[derive(Debug, Clone)]
pub enum ProcessOutput {
    Stdout(String),
    Stderr(String),
}

/// 输出回调类型（需 Sync 才能被 stdout/stderr 两个读取线程共享）
pub type OutputCallback = std::sync::Arc<dyn Fn(ProcessOutput) + Send + Sync + 'static>;

impl GameProcess {
    /// 启动游戏进程
    pub fn spawn(
        executable: &PathBuf,
        args: &[String],
        working_dir: &PathBuf,
        on_output: Option<OutputCallback>,
    ) -> Result<Self> {
        let mut command = Command::new(executable);
        command
            .args(args)
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Windows 上隐藏子进程的控制台窗口
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command.spawn().map_err(|e| {
            PlatformError::Io(std::io::Error::new(
                e.kind(),
                format!("启动游戏进程失败 ({}): {}", executable.display(), e),
            ))
        })?;

        // 抽取 stdout/stderr 句柄，开线程实时读取
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // 统计需要启动的读取线程数
        let reader_count = stdout.is_some() as u8 + stderr.is_some() as u8;
        let readers_remaining = Arc::new(AtomicU8::new(reader_count));

        // 回调已是 Arc，可直接在两个读取线程间共享
        let callback = on_output;

        if let Some(out) = stdout {
            let cb = callback.clone();
            let remaining = readers_remaining.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(out);
                for line in reader.lines().map_while(|line| line.ok()) {
                    if let Some(cb) = &cb {
                        cb(ProcessOutput::Stdout(line));
                    }
                }
                remaining.fetch_sub(1, Ordering::SeqCst);
            });
        }

        if let Some(err) = stderr {
            let cb = callback.clone();
            let remaining = readers_remaining.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(err);
                for line in reader.lines().map_while(|line| line.ok()) {
                    if let Some(cb) = &cb {
                        cb(ProcessOutput::Stderr(line));
                    }
                }
                remaining.fetch_sub(1, Ordering::SeqCst);
            });
        }

        Ok(Self { child, readers_remaining })
    }

    /// 获取进程 ID
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// 查询进程是否仍在运行（不阻塞）
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// 等待进程退出，返回退出码
    pub fn wait(mut self) -> Result<i32> {
        let status = self.child.wait()?;
        Ok(status.code().unwrap_or(-1))
    }

    /// 尝试终止进程
    pub fn kill(&mut self) -> Result<()> {
        self.child.kill()?;
        Ok(())
    }

    /// 输出读取是否已全部结束
    pub fn output_finished(&self) -> bool {
        self.readers_remaining.load(Ordering::SeqCst) == 0
    }
}

/// 按 PID 终止任意进程（前端"结束游戏"用）。
///
/// 之所以不复用 GameProcess::kill：等待线程持有 GameProcess 本体，
/// command 层只有 PID，无法再拿到那个实例。
/// - Windows: taskkill /PID <pid> /T /F（含子进程树），CREATE_NO_WINDOW 隐藏控制台
/// - Unix: kill -TERM <pid>
pub fn kill_process_by_pid(pid: u32) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| {
                PlatformError::Io(std::io::Error::new(
                    e.kind(),
                    format!("调用 taskkill 失败: {}", e),
                ))
            })?;
        if status.success() {
            Ok(())
        } else {
            Err(PlatformError::Io(std::io::Error::other(format!(
                "taskkill 终止进程 {} 失败，退出码: {:?}",
                pid,
                status.code()
            ))))
        }
    }

    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status()
            .map_err(|e| {
                PlatformError::Io(std::io::Error::new(
                    e.kind(),
                    format!("调用 kill 失败: {}", e),
                ))
            })?;
        if status.success() {
            Ok(())
        } else {
            Err(PlatformError::Io(std::io::Error::other(format!(
                "kill 终止进程 {} 失败，退出码: {:?}",
                pid,
                status.code()
            ))))
        }
    }

    #[cfg(not(any(windows, unix)))]
    {
        Err(PlatformError::UnsupportedOs(format!(
            "当前平台不支持按 PID 终止进程: {}",
            std::env::consts::OS
        )))
    }
}

/// 用系统文件管理器打开目录（Windows explorer / macOS open / Linux xdg-open）
///
/// 用 spawn 而不是 status：explorer 打开目录后往往立即返回非零退出码，
/// 等待退出会把成功误判为失败。
pub fn open_in_file_manager(path: &std::path::Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        Command::new("explorer")
            .arg(path)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| {
                PlatformError::Io(std::io::Error::new(
                    e.kind(),
                    format!("打开目录失败 ({}): {}", path.display(), e),
                ))
            })?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn().map_err(|e| {
            PlatformError::Io(std::io::Error::new(
                e.kind(),
                format!("打开目录失败 ({}): {}", path.display(), e),
            ))
        })?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(path).spawn().map_err(|e| {
            PlatformError::Io(std::io::Error::new(
                e.kind(),
                format!("打开目录失败 ({}): {}", path.display(), e),
            ))
        })?;
    }
    Ok(())
}
