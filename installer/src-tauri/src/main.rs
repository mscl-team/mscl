#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! MSCL 月之海启动器 安装程序
//!
//! 自绘安装界面（WebView），替代 NSIS 向导：
//! - 安装：释放主程序 → 写入许可证 / 卸载器 → 创建快捷方式 → 写注册表
//! - 卸载：由安装目录里的 uninstall.exe（本程序的副本）以 `--uninstall` 启动
//! - 自删除：卸载结束后把自身复制到临时目录，以 `--purge <dir>` 清理安装目录

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

const APP_EXE: &[u8] = include_bytes!("../resources/mscl-app.exe");
const LICENSE_TEXT: &str = include_str!("../resources/LICENSE.txt");
const PRODUCT_NAME: &str = "MSCL";
const DISPLAY_NAME: &str = "MSCL 月之海启动器";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const UNINSTALL_KEY: &str = r"HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Uninstall\MSCL";
const INSTALL_KEY: &str = r"HKEY_CURRENT_USER\Software\MSCL";

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// 为每次 PowerShell 调用生成唯一的临时脚本文件名，避免并发写冲突。
static PS_SCRIPT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Serialize, Clone)]
struct InstallInfo {
    version: String,
    default_dir: String,
    license: String,
    /// 以 `--uninstall` 启动时为 true
    uninstall: bool,
    /// 已安装目录（升级/卸载时用于预填）
    installed_dir: Option<String>,
    app_size: u64,
}

#[derive(Serialize, Clone)]
struct ProgressPayload {
    percent: f64,
    stage: String,
    message: String,
}

#[derive(Serialize, Clone)]
struct LogPayload {
    level: String,
    message: String,
}

#[derive(Serialize, Clone)]
struct DonePayload {
    ok: bool,
    message: String,
}

fn emit_progress(app: &AppHandle, percent: f64, stage: &str, message: &str) {
    let _ = app.emit(
        "installer://progress",
        ProgressPayload {
            percent: percent.clamp(0.0, 100.0),
            stage: stage.to_string(),
            message: message.to_string(),
        },
    );
}

fn emit_log(app: &AppHandle, level: &str, message: impl Into<String>) {
    let _ = app.emit(
        "installer://log",
        LogPayload {
            level: level.to_string(),
            message: message.into(),
        },
    );
}

fn emit_done(app: &AppHandle, ok: bool, message: impl Into<String>) {
    let _ = app.emit(
        "installer://done",
        DonePayload {
            ok,
            message: message.into(),
        },
    );
}

#[cfg(windows)]
fn no_window(cmd: &mut Command) -> &mut Command {
    cmd.creation_flags(CREATE_NO_WINDOW)
}

#[cfg(not(windows))]
fn no_window(cmd: &mut Command) -> &mut Command {
    cmd
}

fn ps_escape(s: &str) -> String {
    s.replace('\'', "''")
}

fn run_powershell(script: &str) -> Result<String, String> {
    // 通过临时 .ps1（UTF-8 BOM）执行，而非 -Command 内联：
    // -Command 会把脚本当作命令行参数解析，值中的双引号会被吞掉，导致带引号的
    // 注册表值（如 UninstallString）写入空值；UTF-8 BOM 也避免中文在 PS 5.1 下乱码。
    let mut path = std::env::temp_dir();
    path.push(format!("mscl-ps-{}-{}.ps1", std::process::id(), PS_SCRIPT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let mut content = Vec::with_capacity(script.len() + 3);
    content.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    content.extend_from_slice(script.as_bytes());
    fs::write(&path, &content).map_err(|e| format!("写临时脚本失败: {}", e))?;

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]);
    cmd.arg(&path);
    let mut cmd = cmd;
    let output = no_window(&mut cmd)
        .output()
        .map_err(|e| format!("调用 PowerShell 失败: {}", e))?;
    let _ = fs::remove_file(&path);
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// 生成一条注册表写入语句（不单独执行）。
/// 必须把所有写入放进同一个 PowerShell 脚本一次执行：多次独立进程对同一键
/// 并发执行 New-Item -Force 会相互覆盖，导致除最后一项外的值全部丢失。
fn reg_add_line(key: &str, name: &str, ty: &str, value: &str) -> String {
    let (kind, rendered) = match ty {
        "REG_DWORD" => ("DWord", value.to_string()),
        _ => ("String", format!("'{}'", ps_escape(value))),
    };
    format!(
        "New-ItemProperty -Path 'Registry::{key}' -Name '{name}' -PropertyType {kind} \
         -Value {rendered} -Force | Out-Null",
        key = key,
        name = name,
        kind = kind,
        rendered = rendered
    )
}

fn reg_query(key: &str, name: &str) -> Option<String> {
    let script = format!(
        "try {{ (Get-ItemProperty -Path 'Registry::{}' -Name '{}' -ErrorAction Stop).{} }} catch {{ }}",
        key, name, name
    );
    run_powershell(&script).ok().filter(|s| !s.is_empty())
}

fn local_appdata() -> PathBuf {
    std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("C:\\"))
}

fn default_install_dir() -> PathBuf {
    local_appdata().join(PRODUCT_NAME)
}

/// `--uninstall` 模式；独立卸载程序（文件名以 uninstall 开头）也自动进入卸载流程
fn is_uninstall_mode() -> bool {
    if std::env::args().any(|a| a == "--uninstall") {
        return true;
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.file_name()
                .map(|name| name.to_string_lossy().to_ascii_lowercase())
        })
        .map(|name| name.starts_with("uninstall") || name.starts_with("mscl_uninstall"))
        .unwrap_or(false)
}

/// `--dir <path>`：启动器触发卸载时显式传入安装目录，优先于注册表
fn arg_install_dir() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == "--dir")
        .and_then(|pos| args.get(pos + 1))
        .cloned()
}

/// 复制自身到临时目录，退出后由它清理安装目录
fn spawn_purge(dir: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let tmp = std::env::temp_dir().join(format!("mscl-purge-{}.exe", std::process::id()));
    fs::copy(&exe, &tmp).map_err(|e| format!("复制清理程序失败: {}", e))?;

    let mut cmd = Command::new(&tmp);
    cmd.arg("--purge").arg(dir);
    no_window(&mut cmd)
        .spawn()
        .map_err(|e| format!("启动清理程序失败: {}", e))?;
    Ok(())
}

/// 自删除只允许清理默认安装目录或临时目录，避免被传入任意路径。
fn is_purge_allowed(dir: &Path) -> bool {
    if dir.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return false;
    }
    dir == default_install_dir() || dir.starts_with(std::env::temp_dir())
}

/// `--purge <dir>`：等待主进程退出后删除安装目录并自删除
fn run_purge(dir: &Path) {
    if !is_purge_allowed(dir) {
        println!("PURGE_REJECTED {}", dir.display());
        return;
    }    thread::sleep(Duration::from_millis(900));
    // 卸载器主窗口可能仍开着，给足重试时间（60 秒）等它退出
    for _ in 0..120 {
        if fs::remove_dir_all(dir).is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(500));
    }
    // 删掉临时目录里的自身
    if let Ok(exe) = std::env::current_exe() {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "ping", "127.0.0.1", "-n", "3", ">NUL", "&", "del", "/f", "/q"]);
        cmd.arg(exe.to_string_lossy().to_string());
        let _ = no_window(&mut cmd).spawn();
    }
}

#[tauri::command]
fn get_info() -> InstallInfo {
    InstallInfo {
        version: VERSION.to_string(),
        default_dir: default_install_dir().to_string_lossy().to_string(),
        license: LICENSE_TEXT.to_string(),
        uninstall: is_uninstall_mode(),
        installed_dir: arg_install_dir().or_else(|| {
            reg_query(INSTALL_KEY, "InstallPath")
                .or_else(|| reg_query(UNINSTALL_KEY, "InstallLocation"))
        }),
        app_size: APP_EXE.len() as u64,
    }
}

#[tauri::command]
fn pick_dir(current: String) -> Option<String> {
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms | Out-Null; \
         $d = New-Object System.Windows.Forms.FolderBrowserDialog; \
         $d.Description = '选择 MSCL 安装目录'; \
         $d.SelectedPath = '{}'; \
         if ($d.ShowDialog() -eq 'OK') {{ Write-Output $d.SelectedPath }}",
        ps_escape(&current)
    );
    run_powershell(&script).ok().filter(|s| !s.is_empty())
}

#[tauri::command]
fn launch_app(app: AppHandle, dir: String) -> Result<(), String> {
    let exe = Path::new(&dir).join("mscl-app.exe");
    if !exe.is_file() {
        return Err(format!("未找到启动器：{}", exe.to_string_lossy()));
    }
    let mut cmd = Command::new(&exe);
    cmd.current_dir(&dir);
    no_window(&mut cmd)
        .spawn()
        .map_err(|e| format!("启动 MSCL 失败：{}", e))?;
    // 启动器已拉起，关闭安装程序，避免两个窗口并存。
    app.exit(0);
    Ok(())
}

#[tauri::command]
fn open_dir(dir: String) {
    let mut cmd = Command::new("explorer");
    cmd.arg(dir);
    let _ = no_window(&mut cmd).spawn();
}

#[tauri::command]
fn exit_app(app: AppHandle) {
    app.exit(0);
}

/// 写快捷方式（桌面 / 开始菜单）
fn create_shortcuts(
    dir: &Path,
    desktop: bool,
    start_menu: bool,
) -> Result<(), String> {
    let target = dir.join("mscl-app.exe");
    let uninstaller = dir.join("uninstall.exe");
    let t = ps_escape(&target.to_string_lossy());
    let u = ps_escape(&uninstaller.to_string_lossy());
    let w = ps_escape(&dir.to_string_lossy());
    let mut script = String::from("$ws = New-Object -ComObject WScript.Shell; ");

    if start_menu {
        script.push_str(&format!(
            "$sm = Join-Path $env:APPDATA 'Microsoft\\Windows\\Start Menu\\Programs\\MSCL'; \
             New-Item -ItemType Directory -Force -Path $sm | Out-Null; \
             $s = $ws.CreateShortcut((Join-Path $sm 'MSCL.lnk')); \
             $s.TargetPath = '{t}'; $s.WorkingDirectory = '{w}'; \
             $s.IconLocation = '{t},0'; $s.Description = '{DISPLAY_NAME}'; $s.Save(); \
             $u = $ws.CreateShortcut((Join-Path $sm '卸载 MSCL.lnk')); \
             $u.TargetPath = '{u}'; $u.Arguments = '--uninstall'; $u.WorkingDirectory = '{w}'; \
             $u.IconLocation = '{t},0'; $u.Description = '卸载 {DISPLAY_NAME}'; $u.Save(); "
        ));
    }
    if desktop {
        script.push_str(&format!(
            "$desk = [Environment]::GetFolderPath('Desktop'); \
             $s2 = $ws.CreateShortcut((Join-Path $desk 'MSCL.lnk')); \
             $s2.TargetPath = '{t}'; $s2.WorkingDirectory = '{w}'; \
             $s2.IconLocation = '{t},0'; $s2.Description = '{DISPLAY_NAME}'; $s2.Save(); "
        ));
    }
    if script.len() > 40 {
        run_powershell(&script)?;
    }
    Ok(())
}

fn write_registry(dir: &Path) {
    let exe = dir.join("mscl-app.exe");
    let uninstaller = dir.join("uninstall.exe");
    let size_kb = ((APP_EXE.len() as u64) / 1024 + 512).to_string();

    // 先建键（一次），再在同一脚本里写入全部值，避免多进程并发覆盖。
    let script = format!(
        "New-Item -Path 'Registry::{ik}' -Force | Out-Null; \
         New-Item -Path 'Registry::{uk}' -Force | Out-Null; \
         {lines}",
        ik = INSTALL_KEY,
        uk = UNINSTALL_KEY,
        lines = [
            reg_add_line(INSTALL_KEY, "InstallPath", "REG_SZ", &dir.to_string_lossy()),
            reg_add_line(UNINSTALL_KEY, "DisplayName", "REG_SZ", DISPLAY_NAME),
            reg_add_line(UNINSTALL_KEY, "DisplayVersion", "REG_SZ", VERSION),
            reg_add_line(UNINSTALL_KEY, "Publisher", "REG_SZ", "MSCL Team"),
            reg_add_line(UNINSTALL_KEY, "DisplayIcon", "REG_SZ", &exe.to_string_lossy()),
            reg_add_line(UNINSTALL_KEY, "InstallLocation", "REG_SZ", &dir.to_string_lossy()),
            reg_add_line(
                UNINSTALL_KEY,
                "UninstallString",
                "REG_SZ",
                &format!("\"{}\" --uninstall", uninstaller.to_string_lossy()),
            ),
            reg_add_line(UNINSTALL_KEY, "NoModify", "REG_DWORD", "1"),
            reg_add_line(UNINSTALL_KEY, "NoRepair", "REG_DWORD", "1"),
            reg_add_line(UNINSTALL_KEY, "EstimatedSize", "REG_DWORD", &size_kb),
        ]
        .join("; ")
    );
    let _ = run_powershell(&script);
}

/// 把用户勾选的默认插件写入游戏数据目录的 plugins/config.json。
/// 路径用 %APPDATA%\.mscl，与主程序 default_data_dir 保持一致。
/// 返回实际写入字节数，供安装进度按真实字节统计。
fn write_plugin_config(enabled_ids: &[String]) -> usize {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| "C:\\".to_string());
    let root = Path::new(&appdata).join(".mscl").join("plugins");
    if fs::create_dir_all(&root).is_err() {
        return 0;
    }
    let enabled: Vec<serde_json::Value> = enabled_ids
        .iter()
        .map(|id| serde_json::Value::String(id.clone()))
        .collect();
    let config = serde_json::json!({ "schema": 1, "enabled": enabled });
    let text = serde_json::to_string_pretty(&config).unwrap_or_default();
    match fs::write(root.join("config.json"), &text) {
        Ok(()) => text.len(),
        Err(_) => 0,
    }
}

/// 分块写入文件，返回实际写入字节数；每块通过回调回报真实进度。
fn write_chunked(path: &Path, data: &[u8], on_chunk: &mut dyn FnMut(u64)) -> Result<(), String> {
    let mut file = File::create(path).map_err(|e| format!("创建文件失败: {}", e))?;
    const CHUNK: usize = 1 << 20;
    for chunk in data.chunks(CHUNK) {
        file.write_all(chunk).map_err(|e| format!("写入失败: {}", e))?;
        on_chunk(chunk.len() as u64);
    }
    file.flush().ok();
    drop(file);
    Ok(())
}

fn do_install(
    app: AppHandle,
    dir: String,
    desktop: bool,
    start_menu: bool,
    plugins: Vec<String>,
) {
    let app2 = app.clone();
    thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            let target = PathBuf::from(&dir);
            if target.as_os_str().is_empty() {
                return Err("安装目录不能为空".to_string());
            }

            emit_progress(&app2, 0.0, "准备安装", "创建安装目录…");
            fs::create_dir_all(&target).map_err(|e| format!("创建目录失败: {}", e))?;
            emit_log(&app2, "info", format!("安装目录：{}", dir));

            // ---- 真实字节进度 ----
            // 数据写入占 0-94%，按各文件真实字节数加权；元信息（快捷方式/注册表）占 94-100%。
            let self_exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let self_size = fs::metadata(&self_exe).map(|m| m.len()).unwrap_or(0);
            // 插件配置按每项约 96 字节预留，避免 20+ 插件时进度提前钳到 94%
            let config_reserve = (plugins.len() as u64) * 96 + 512;
            let total_bytes = APP_EXE.len() as u64 + self_size + LICENSE_TEXT.len() as u64 + config_reserve;
            let mut written: u64 = 0;
            let emit_bytes = |w: u64, stage: &str, msg: &str| {
                // 严格单调：用已写字节 / 总字节 × 94
                let pct = (w as f64 / total_bytes.max(1) as f64) * 94.0;
                emit_progress(&app2, pct.min(94.0), stage, msg);
            };

            // 1. 主程序（按真实字节）
            emit_bytes(0, "写入程序文件", "正在释放主程序…");
            let exe_path = target.join("mscl-app.exe");
            write_chunked(&exe_path, APP_EXE, &mut |n| {
                written += n;
                emit_bytes(written, "写入程序文件", "正在释放主程序…");
                thread::sleep(Duration::from_millis(4));
            })?;
            emit_log(&app2, "success", "主程序已释放");

            // 2. 许可证（真实字节）
            fs::write(target.join("LICENSE.txt"), LICENSE_TEXT)
                .map_err(|e| format!("写入协议失败: {}", e))?;
            written += LICENSE_TEXT.len() as u64;
            emit_bytes(written, "写入许可证", "写入用户使用协议…");

            // 3. 默认插件配置（真实字节）
            let cfg_bytes = write_plugin_config(&plugins);
            written += cfg_bytes as u64;
            emit_bytes(written, "配置插件", "正在写入默认插件配置…");
            emit_log(&app2, "success", format!("默认插件配置已写入（{} 项）", plugins.len()));

            // 4. 卸载器（按真实字节分块复制自身，不再用 fs::copy 造成进度卡死）
            let uninstaller_data = fs::read(&self_exe).map_err(|e| e.to_string())?;
            let uninst_path = target.join("uninstall.exe");
            write_chunked(&uninst_path, &uninstaller_data, &mut |n| {
                written += n;
                emit_bytes(written, "安装卸载器", "正在写入卸载程序…");
                thread::sleep(Duration::from_millis(4));
            })?;
            emit_log(&app2, "success", "卸载器已就绪");

            // 5. 快捷方式（94% - 97%，固定小权重）
            emit_progress(&app2, 95.0, "创建快捷方式", "正在创建快捷方式…");
            if let Err(e) = create_shortcuts(&target, desktop, start_menu) {
                emit_log(&app2, "warn", format!("快捷方式创建失败：{}", e));
            } else {
                emit_log(&app2, "success", "快捷方式已创建");
            }

            // 6. 注册表（97% - 100%）
            emit_progress(&app2, 98.0, "写入注册表", "正在注册卸载信息…");
            write_registry(&target);
            emit_log(&app2, "success", "卸载信息已写入注册表");

            emit_progress(&app2, 100.0, "安装完成", "安装完成");
            emit_done(&app2, true, "安装完成，可以开始使用了");
            Ok(())
        })();

        if let Err(e) = result {
            emit_log(&app2, "error", e.clone());
            emit_done(&app2, false, e);
        }
    });
}

#[tauri::command]
fn start_install(app: AppHandle, dir: String, desktop: bool, start_menu: bool, plugins: Vec<String>) {
    do_install(app, dir, desktop, start_menu, plugins);
}

/// 启动器进程是否仍在运行
#[cfg(windows)]
fn launcher_running() -> bool {
    let mut cmd = Command::new("tasklist");
    cmd.args(["/FI", "IMAGENAME eq mscl-app.exe"]);
    no_window(&mut cmd)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .to_ascii_lowercase()
                .contains("mscl-app.exe")
        })
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn launcher_running() -> bool {
    false
}

/// 安全关闭启动器：先请求优雅退出，超时后再强制结束。
/// 只要最终仍检测到启动器进程，就返回错误并中止卸载。
fn close_launcher(app: &AppHandle) -> Result<(), String> {
    if !launcher_running() {
        return Ok(());
    }

    emit_progress(app, 6.0, "关闭启动器", "正在请求启动器退出…");
    emit_log(app, "info", "检测到启动器仍在运行，先请求其优雅退出");
    let mut graceful = Command::new("taskkill");
    graceful.args(["/IM", "mscl-app.exe"]);
    let _ = no_window(&mut graceful).output();
    for _ in 0..16 {
        if !launcher_running() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }

    emit_progress(app, 10.0, "关闭启动器", "启动器未响应，正在强制结束…");
    emit_log(app, "warn", "启动器未响应，改用强制结束");
    let mut force = Command::new("taskkill");
    force.args(["/IM", "mscl-app.exe", "/F"]);
    let _ = no_window(&mut force).output();
    for _ in 0..20 {
        if !launcher_running() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }

    Err("启动器仍在运行，已停止卸载以保证安全。请先手动关闭 MSCL 后重试。".to_string())
}

#[tauri::command]
fn start_uninstall(app: AppHandle, dir: String) {
    let app2 = app.clone();
    thread::spawn(move || {
        let target = PathBuf::from(&dir);
        let expected = arg_install_dir()
            .or_else(|| reg_query(INSTALL_KEY, "InstallPath"))
            .or_else(|| reg_query(UNINSTALL_KEY, "InstallLocation"))
            .unwrap_or_else(|| default_install_dir().to_string_lossy().into_owned());
        if target.to_string_lossy().to_lowercase() != expected.to_lowercase() {
            let msg = "卸载目录与已注册的安装目录不一致，已停止卸载。".to_string();
            emit_log(&app2, "error", msg.clone());
            emit_done(&app2, false, msg);
            return;
        }
        emit_progress(&app2, 5.0, "准备卸载", "正在关闭 MSCL…");
        if let Err(e) = close_launcher(&app2) {
            emit_log(&app2, "error", e.clone());
            emit_done(&app2, false, e);
            return;
        }

        emit_progress(&app2, 25.0, "删除快捷方式", "正在删除快捷方式…");
        let script = "$ws = New-Object -ComObject WScript.Shell; \
             $sm = Join-Path $env:APPDATA 'Microsoft\\Windows\\Start Menu\\Programs\\MSCL'; \
             if (Test-Path $sm) { Remove-Item -Recurse -Force $sm }; \
             $desk = [Environment]::GetFolderPath('Desktop'); \
             $lnk = Join-Path $desk 'MSCL.lnk'; \
             if (Test-Path $lnk) { Remove-Item -Force $lnk }";
        let _ = run_powershell(script);

        emit_progress(&app2, 45.0, "清理注册表", "正在清理注册信息…");
        let script = format!(
            "Remove-Item -Path 'Registry::{}' -Recurse -Force -ErrorAction SilentlyContinue; \
             Remove-Item -Path 'Registry::{}' -Recurse -Force -ErrorAction SilentlyContinue",
            UNINSTALL_KEY, INSTALL_KEY
        );
        let _ = run_powershell(&script);

        // 删除文件前再确认一次：启动器若被重新拉起，立即中止避免误删占用文件
        if launcher_running() {
            let msg = "检测到启动器重新运行，已停止卸载以保证安全。".to_string();
            emit_log(&app2, "error", msg.clone());
            emit_done(&app2, false, msg);
            return;
        }
        emit_progress(&app2, 70.0, "删除文件", "正在删除安装文件…");
        // 删除除卸载器之外的文件
        if let Ok(entries) = fs::read_dir(&target) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.file_name().map(|n| n == "uninstall.exe").unwrap_or(false) {
                    continue;
                }
                if path.is_dir() {
                    let _ = fs::remove_dir_all(&path);
                } else {
                    let _ = fs::remove_file(&path);
                }
            }
        }

        emit_progress(&app2, 95.0, "收尾", "正在清理残留…");
        let _ = spawn_purge(&target);

        emit_progress(&app2, 100.0, "卸载完成", "MSCL 已卸载");
        emit_done(&app2, true, "MSCL 已卸载");
    });
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--purge") {
        if let Some(dir) = args.get(pos + 1) {
            run_purge(Path::new(dir));
            return;
        }
    }

    // 自检：完整跑一遍 释放文件 → 开始菜单快捷方式 → 注册表，然后回滚
    if let Some(pos) = args.iter().position(|a| a == "--selftest") {
        let dir = args
            .get(pos + 1)
            .cloned()
            .unwrap_or_else(|| std::env::temp_dir().join("mscl-selftest").to_string_lossy().to_string());
        let target = PathBuf::from(&dir);
        let _ = fs::remove_dir_all(&target);
        let result = (|| -> Result<u64, String> {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            let path = target.join("mscl-app.exe");
            fs::write(&path, APP_EXE).map_err(|e| e.to_string())?;
            let size = fs::metadata(&path).map_err(|e| e.to_string())?.len();
            if size != APP_EXE.len() as u64 {
                return Err("写入文件大小不一致".to_string());
            }
            fs::write(target.join("LICENSE.txt"), LICENSE_TEXT).map_err(|e| e.to_string())?;

            // 卸载器：验证安装目录必定携带且非空
            let uninstaller = target.join("uninstall.exe");
            let self_exe = std::env::current_exe().map_err(|e| e.to_string())?;
            fs::copy(&self_exe, &uninstaller).map_err(|e| e.to_string())?;
            if fs::metadata(&uninstaller).map_err(|e| e.to_string())?.len() == 0 {
                return Err("安装目录中的卸载器为空".to_string());
            }
            println!("SELFTEST_UNINSTALLER_OK {}", uninstaller.display());

            // 默认插件配置写入（自检写个最小样例后清理）
            write_plugin_config(&["plugin-plugin-hub".to_string()]);
            let appdata = std::env::var("APPDATA").unwrap_or_else(|_| "C:\\".to_string());
            let cfg = Path::new(&appdata).join(".mscl").join("plugins").join("config.json");
            if !cfg.exists() {
                return Err(format!("默认插件配置未写入: {}", cfg.display()));
            }
            let _ = fs::remove_file(&cfg);
            println!("SELFTEST_PLUGIN_CONFIG_OK");

            // 开始菜单快捷方式（不建桌面，避免污染）
            create_shortcuts(&target, false, true)?;
            let sm = std::env::var("APPDATA")
                .map(|a| PathBuf::from(a).join("Microsoft/Windows/Start Menu/Programs/MSCL/MSCL.lnk"))
                .unwrap_or_default();
            if !sm.exists() {
                return Err(format!("开始菜单快捷方式未创建: {}", sm.display()));
            }
            let sm_uninstall = sm.with_file_name("卸载 MSCL.lnk");
            if !sm_uninstall.exists() {
                return Err(format!("开始菜单卸载快捷方式未创建: {}", sm_uninstall.display()));
            }

            // 注册表写入 + 回读
            write_registry(&target);
            let back = reg_query(INSTALL_KEY, "InstallPath").unwrap_or_default();
            if !back.eq_ignore_ascii_case(&target.to_string_lossy()) {
                return Err(format!("注册表回读不一致: '{}'", back));
            }

            // 控制面板“卸载”按钮执行 UninstallString：必须指向安装目录内的自定义卸载器。
            // 回读该命令并校验其指向的 uninstall.exe 真实存在，确保走的是自绘卸载流程。
            let uninst_str = reg_query(UNINSTALL_KEY, "UninstallString").unwrap_or_default();
            let uninst_expected = format!("\"{}\" --uninstall", uninstaller.to_string_lossy());
            if !uninst_str.eq_ignore_ascii_case(&uninst_expected) {
                return Err(format!(
                    "卸载命令回读不一致: '{}' (期望 '{}')",
                    uninst_str, uninst_expected
                ));
            }
            if !uninstaller.exists() {
                return Err(format!("卸载命令指向的卸载器不存在: {}", uninstaller.display()));
            }
            println!("SELFTEST_UNINSTALL_STRING_OK {}", uninst_str);

            // 回滚
            let _ = run_powershell(&format!(
                "Remove-Item -Recurse -Force '{}' -ErrorAction SilentlyContinue; \
                 Remove-Item -Recurse -Force 'Registry::{}' -ErrorAction SilentlyContinue; \
                 Remove-Item -Recurse -Force 'Registry::{}' -ErrorAction SilentlyContinue",
                ps_escape(&sm.parent().unwrap_or(Path::new("")).to_string_lossy()),
                UNINSTALL_KEY,
                INSTALL_KEY
            ));
            Ok(size)
        })();
        match result {
            Ok(size) => {
                println!("SELFTEST_OK {} bytes -> {}", size, dir);
                let _ = fs::remove_dir_all(&target);
                std::process::exit(0);
            }
            Err(e) => {
                println!("SELFTEST_FAIL {}", e);
                std::process::exit(1);
            }
        }
    }

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_info,
            pick_dir,
            start_install,
            start_uninstall,
            launch_app,
            open_dir,
            exit_app
        ])
        .setup(|app| {
            if is_uninstall_mode() {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.set_title("MSCL 卸载程序");
                    // 卸载确认页内容少，独立缩小窗口避免大量留白
                    let _ = win.set_size(tauri::LogicalSize::new(640u32, 470u32));
                    let _ = win.center();
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running installer");
}
