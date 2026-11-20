//! 应用全局状态
//!
//! 通过 Tauri 的 manage() 注入，所有 command 可访问。
//! 用 Mutex 保护可变部分（运行中的游戏进程）。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::models::RunningGameInfo;

/// 启动器自身配置（独立于游戏目录，避免"配置放在被配置目录里"的鸡生蛋问题）
pub fn launcher_config_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(|appdata| PathBuf::from(appdata).join("mscl-launcher").join("config.json"))
}

/// 读取启动器自身配置（缺失或损坏时返回空对象）
fn read_config() -> serde_json::Value {
    let text = launcher_config_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .filter(|value| value.is_object())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// 写回启动器自身配置（保留已有键）
fn write_config(value: &serde_json::Value) -> Result<(), String> {
    let config = launcher_config_path().ok_or("无法定位配置目录（缺少 APPDATA）")?;
    if let Some(parent) = config.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败: {}", e))?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(&config, text).map_err(|e| format!("写入配置失败: {}", e))
}

/// 读取自定义游戏目录（未设置时返回 None）
pub fn load_custom_game_root() -> Option<PathBuf> {
    read_config()
        .get("gameRoot")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

/// 保存自定义游戏目录
pub fn save_custom_game_root(path: &std::path::Path) -> Result<(), String> {
    let mut config = read_config();
    config["gameRoot"] = serde_json::json!(path.to_string_lossy());
    write_config(&config)
}

/// 默认日志目录：启动器配置目录下的 logs
pub fn default_log_dir() -> PathBuf {
    launcher_config_path()
        .and_then(|path| path.parent().map(|parent| parent.join("logs")))
        .unwrap_or_else(|| PathBuf::from("logs"))
}

/// 读取自定义日志目录（未设置时返回 None）
pub fn load_custom_log_dir() -> Option<PathBuf> {
    read_config()
        .get("logDir")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

/// 当前生效的日志目录
pub fn current_log_dir() -> PathBuf {
    load_custom_log_dir().unwrap_or_else(default_log_dir)
}

/// 设置自定义日志目录
pub fn save_custom_log_dir(path: &std::path::Path) -> Result<(), String> {
    let mut config = read_config();
    config["logDir"] = serde_json::json!(path.to_string_lossy());
    write_config(&config)
}

/// 恢复默认日志目录
pub fn reset_custom_log_dir() -> Result<(), String> {
    let mut config = read_config();
    if let Some(object) = config.as_object_mut() {
        object.remove("logDir");
    }
    write_config(&config)
}

/// 启动器临时缓存目录（配置目录下 cache）
pub fn launcher_cache_dir() -> Option<PathBuf> {
    launcher_config_path().and_then(|path| path.parent().map(|parent| parent.join("cache")))
}

/// 解析主游戏目录（自定义优先，否则平台默认）
pub fn primary_game_root() -> Result<PathBuf, String> {
    if let Some(path) = load_custom_game_root() {
        std::fs::create_dir_all(&path)
            .map_err(|e| format!("创建自定义游戏目录失败: {}", e))?;
        return Ok(path);
    }
    mscl_platform::default_data_dir().map_err(|e| format!("无法确定游戏数据目录: {}", e))
}

/// 额外游戏目录（用于跨目录实例管理）
pub fn load_extra_game_roots() -> Vec<PathBuf> {
    read_config()
        .get("extraGameRoots")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

pub fn save_extra_game_roots(paths: &[PathBuf]) -> Result<(), String> {
    let mut config = read_config();
    config["extraGameRoots"] = serde_json::json!(
        paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
    );
    write_config(&config)
}

fn same_path(left: &Path, right: &Path) -> bool {
    if let (Ok(a), Ok(b)) = (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        return a == b;
    }
    left.to_string_lossy().to_lowercase() == right.to_string_lossy().to_lowercase()
}

/// 主目录 + 额外目录（去重）
pub fn all_game_roots() -> Result<Vec<PathBuf>, String> {
    let primary = primary_game_root()?;
    let mut roots = vec![primary.clone()];
    for extra in load_extra_game_roots() {
        if extra.is_dir() && !roots.iter().any(|root| same_path(root, &extra)) {
            roots.push(extra);
        }
    }
    Ok(roots)
}

/// 该路径是否为已注册的游戏目录（防止任意路径启动）
pub fn is_known_game_root(path: &Path) -> bool {
    all_game_roots()
        .map(|roots| roots.iter().any(|root| same_path(root, path)))
        .unwrap_or(false)
}

/// 读取账户列表与当前账户 ID
pub fn load_accounts_config() -> (Vec<serde_json::Value>, Option<String>) {
    let config = read_config();
    let accounts = config
        .get("accounts")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let active = config
        .get("activeAccountId")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    (accounts, active)
}

/// 保存账户列表与当前账户 ID
pub fn save_accounts_config(
    accounts: &[serde_json::Value],
    active_id: Option<&str>,
) -> Result<(), String> {
    let mut config = read_config();
    config["accounts"] = serde_json::json!(accounts);
    if let Some(id) = active_id {
        config["activeAccountId"] = serde_json::json!(id);
    } else if let Some(object) = config.as_object_mut() {
        object.remove("activeAccountId");
    }
    write_config(&config)
}

/// 应用状态
pub struct AppState {
    /// HTTP client（复用连接池）
    pub http_client: reqwest::Client,
    /// 游戏数据根目录（platform 层默认目录）
    pub game_root: std::path::PathBuf,
    /// 运行中的游戏实例（同一实例允许多开，因此用 run_id 区分）
    pub running: Mutex<Vec<RunningGameInfo>>,
    /// 进程资源采样器（保留两次刷新之间的 CPU 差值）
    pub system: Mutex<sysinfo::System>,
    /// 引擎停止信号：暂停或取消都会置位，下载循环看到后立刻停下
    pub install_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// 是否处于“暂停”状态（停止后等待用户继续）
    pub install_paused: std::sync::atomic::AtomicBool,
    /// 是否已被用户取消（清理半成品并结束流程）
    pub install_cancelled: std::sync::atomic::AtomicBool,
    /// 是否有实例安装流程正在运行（用于区分“新安装”与“流程内的子步骤”）
    pub install_active: std::sync::atomic::AtomicBool,
}

impl AppState {
    pub fn new() -> Result<Self, String> {
        let game_root = primary_game_root()?;

        // 注意：不设总超时——单个大文件（库/资源）在慢网络下很容易超过 120 秒，
        // 总超时会让下载在后半段被掐断，表现为 "error decoding response body"。
        // 改为连接超时 + 单次读取超时：只要还在持续收数据就不会被判失败。
        let http_client = reqwest::Client::builder()
            .user_agent(format!("MSCL/{}", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(20))
            .read_timeout(std::time::Duration::from_secs(60))
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .build()
            .map_err(|e| format!("创建 HTTP client 失败: {}", e))?;

        Ok(Self {
            http_client,
            game_root,
            running: Mutex::new(Vec::new()),
            system: Mutex::new(sysinfo::System::new()),
            install_cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            install_paused: std::sync::atomic::AtomicBool::new(false),
            install_cancelled: std::sync::atomic::AtomicBool::new(false),
            install_active: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// 标记实例安装流程是否在运行
    pub fn set_install_active(&self, active: bool) {
        self.install_active
            .store(active, std::sync::atomic::Ordering::SeqCst);
    }

    /// 是否有实例安装流程正在运行
    pub fn is_install_active(&self) -> bool {
        self.install_active.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 开始一次实例安装：清掉上一次的暂停 / 取消标记
    pub fn begin_install(&self) {
        self.install_cancel
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.install_paused
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.install_cancelled
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// 暂停当前安装：只停止下载，保留已完成文件，等待继续
    pub fn pause_install(&self) {
        self.install_paused
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.install_cancel
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// 恢复当前安装：清掉停止信号，流程会重跑当前阶段并跳过已校验文件
    pub fn resume_install(&self) {
        self.install_paused
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.install_cancel
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// 取消当前安装：结束流程并清理半成品
    pub fn request_cancel_install(&self) {
        self.install_paused
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.install_cancelled
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.install_cancel
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// 是否已被用户取消
    pub fn is_install_cancelled(&self) -> bool {
        self.install_cancelled
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 是否处于暂停状态
    pub fn is_install_paused(&self) -> bool {
        self.install_paused.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn add_running(&self, instance: RunningGameInfo) {
        if let Ok(mut running) = self.running.lock() {
            running.push(instance);
        }
    }

    pub fn remove_running(&self, run_id: &str) -> Option<RunningGameInfo> {
        let mut guard = self.running.lock().ok()?;
        let index = guard.iter().position(|item| item.run_id == run_id)?;
        Some(guard.remove(index))
    }

    pub fn remove_running_by_version(&self, version_id: &str) -> Vec<RunningGameInfo> {
        let Ok(mut guard) = self.running.lock() else {
            return Vec::new();
        };
        let mut removed = Vec::new();
        let mut index = 0;
        while index < guard.len() {
            if guard[index].version_id == version_id {
                removed.push(guard.remove(index));
            } else {
                index += 1;
            }
        }
        removed
    }

    pub fn clear_running(&self) -> Vec<RunningGameInfo> {
        self.running
            .lock()
            .map(|mut guard| std::mem::take(&mut *guard))
            .unwrap_or_default()
    }

    pub fn list_running(&self) -> Vec<RunningGameInfo> {
        self.running
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }
}
