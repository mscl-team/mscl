//! 前后端数据传输对象（DTO）
//!
//! 这些类型是 Tauri command 的参数/返回值，是"前端契约"。
//! 与 core 内部模型刻意解耦：前端只看到需要的字段，
//! core 模型变化时不影响前端（除非 DTO 也改）。

use serde::{Deserialize, Serialize};

/// 版本列表中的单个版本（前端展示用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub id: String,
    pub version_type: String,
    pub release_time: String,
    pub is_latest_release: bool,
}

/// 版本清单（前端展示用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionListResponse {
    pub latest_release: String,
    pub latest_snapshot: String,
    pub versions: Vec<VersionInfo>,
}

/// Java 运行时信息（前端展示用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaInfo {
    pub path: String,
    pub major_version: Option<u32>,
    pub version_output: String,
}

/// 安装游戏请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallGameRequest {
    /// 要安装的版本 id
    pub version_id: String,
}

/// 安装结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResult {
    pub version_id: String,
    pub downloaded: usize,
    pub skipped: usize,
    pub failed: usize,
    pub errors: Vec<String>,
    pub game_root: String,
}

/// 启动游戏请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchGameRequest {
    pub version_id: String,
    pub username: String,
    /// 分配内存 MB，0 表示自动
    pub max_memory_mb: u32,
    /// 指定游戏目录；None 使用当前主目录
    #[serde(default)]
    pub game_root: Option<String>,
    /// 本次启动覆盖 JVM 参数
    #[serde(default)]
    pub jvm_args: Option<String>,
    /// 本次启动覆盖游戏参数
    #[serde(default)]
    pub game_args: Option<String>,
}

/// 启动结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchResult {
    pub pid: u32,
    pub version_id: String,
    pub run_id: String,
    pub game_root: String,
    pub command_line: String,
}

/// 运行中的实例
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningGameInfo {
    pub run_id: String,
    pub version_id: String,
    pub pid: u32,
    pub started_at: u64,
    pub game_root: String,
}

/// 游戏目录（多目录实例扫描）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameRootInfo {
    pub path: String,
    pub is_primary: bool,
    pub exists: bool,
    pub instance_count: usize,
}

/// 单个运行实例的资源占用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceMetrics {
    pub run_id: String,
    pub version_id: String,
    pub pid: u32,
    pub game_root: String,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub uptime_secs: u64,
}

/// 一次运行监测采样
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeMetrics {
    pub timestamp: u64,
    pub system_cpu_percent: f32,
    pub system_memory_used: u64,
    pub system_memory_total: u64,
    pub instances: Vec<InstanceMetrics>,
}

/// 实例级自定义启动参数（instances/{id}/launch.json）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstanceLaunchOptions {
    /// 0 / None 表示沿用全局设置
    #[serde(default)]
    pub max_memory_mb: Option<u32>,
    #[serde(default)]
    pub jvm_args: String,
    #[serde(default)]
    pub game_args: String,
}

/// 应用设置（持久化到 game_root/settings.json）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// 离线模式游戏昵称
    pub username: String,
    /// 分配内存 MB，0 表示自动
    pub max_memory_mb: u32,
    /// 自定义 Java 路径，None 表示自动检测
    pub java_path: Option<String>,
    /// 下载源：official / bmclapi / 自定义根地址
    #[serde(default = "default_download_source")]
    pub download_source: String,
    /// 并发下载数
    #[serde(default = "default_download_concurrency")]
    pub download_concurrency: u32,
    /// 版本隔离：每个版本独立的存档 / Mod / 配置目录
    #[serde(default)]
    pub version_isolation: bool,
    /// 自定义隔离的资源类型；空数组按旧行为视为全部隔离
    #[serde(default = "default_isolation_resources")]
    pub isolation_resources: Vec<String>,
    /// 资源站 API 根地址（Modrinth 兼容），可自定义镜像
    #[serde(default = "default_modrinth_api")]
    pub modrinth_api: String,
    /// 自定义资源来源列表（首个为默认来源）
    #[serde(default = "default_resource_sources")]
    pub resource_sources: Vec<ResourceSource>,
    /// 安装资源前检查重复与前置依赖
    #[serde(default = "default_true")]
    pub check_dependencies: bool,
    /// 启动前校验资源完整性（较慢，默认关闭）
    #[serde(default)]
    pub verify_resources_on_launch: bool,
    /// 启动前校验游戏文件完整性（默认开启）
    #[serde(default = "default_true")]
    pub verify_game_on_launch: bool,
    /// 启动器语言（zh-CN / en-US）
    #[serde(default = "default_language")]
    pub launcher_language: String,
    /// 额外 JVM 参数（空格分隔）
    #[serde(default)]
    pub extra_jvm_args: String,
    /// 额外游戏参数（空格分隔）
    #[serde(default)]
    pub extra_game_args: String,
    /// 隐藏安装日志（底部状态坞的安装/下载过程日志）
    #[serde(default)]
    pub hide_install_logs: bool,
    /// 隐藏更新日志（版本校验 / 修复 / 更新类日志）
    #[serde(default)]
    pub hide_update_logs: bool,
    /// 隐藏启动日志（首页游戏控制台输出）
    #[serde(default)]
    pub hide_launch_logs: bool,
    /// 插件仓库地址（GitHub）
    #[serde(default = "default_plugin_repo")]
    pub plugin_repo_url: String,
    /// 插件仓库分支
    #[serde(default = "default_plugin_branch")]
    pub plugin_repo_branch: String,
    /// 插件仓库访问 Token（可选，私有仓库）
    #[serde(default)]
    pub plugin_repo_token: String,
    /// 是否随程序更新默认插件库
    #[serde(default = "default_true")]
    pub plugin_auto_update: bool,
    /// 是否自动检查插件仓库更新
    #[serde(default = "default_true")]
    pub plugin_check_updates: bool,
    /// 是否自动检查并更新启动器本体（总开关）
    #[serde(default = "default_true")]
    pub launcher_auto_update: bool,
    /// 自定义资源安装目录（实例之外的本地目录、服务器/多人游戏目录）
    #[serde(default)]
    pub resource_targets: Vec<ResourceTargetDir>,
}

/// 自定义资源安装目标目录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTargetDir {
    pub id: String,
    pub name: String,
    pub path: String,
    /// local | server | multiplayer
    pub kind: String,
}

fn default_plugin_repo() -> String {
    "https://github.com/mscl-team/mscl-plugins".to_string()
}

fn default_plugin_branch() -> String {
    "main".to_string()
}

fn default_language() -> String {
    "zh-CN".to_string()
}

fn default_true() -> bool {
    true
}

fn default_modrinth_api() -> String {
    "https://api.modrinth.com/v2".to_string()
}

/// 创建实例请求（自定义名称 + 加载器组合 + 附属 API）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInstanceRequest {
    /// 实例显示名称
    pub name: String,
    /// 原版游戏版本
    pub minecraft_version: String,
    /// vanilla | fabric | forge | neoforge
    pub loader: String,
    /// 指定加载器版本；None 取推荐/最新
    #[serde(default)]
    pub loader_version: Option<String>,
    /// 是否安装 Fabric API（仅 fabric）
    #[serde(default)]
    pub install_fabric_api: bool,
    /// 是否下载 OptiFine（Fabric 下需自行搭配 OptiFabric）
    #[serde(default)]
    pub install_optifine: bool,
    /// 版本描述（安装时可填）
    #[serde(default)]
    pub description: String,
    /// 版本图标（emoji 或图片路径）
    #[serde(default)]
    pub icon: String,
    /// 版本标签
    #[serde(default)]
    pub tags: Vec<String>,
}

/// 实例信息（前端展示）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub version_id: String,
    pub name: String,
    pub minecraft_version: String,
    pub loader: String,
    pub loader_version: Option<String>,
    /// 版本描述
    #[serde(default)]
    pub description: String,
    /// 版本图标（emoji 或图片路径）
    #[serde(default)]
    pub icon: String,
    /// 版本标签
    #[serde(default)]
    pub tags: Vec<String>,
    /// 实例所属游戏目录
    #[serde(default)]
    pub game_root: String,
}

/// 编辑实例信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInstanceMetaRequest {
    pub version_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// 若实例尚不存在，用这些字段创建元数据（原版版本首次编辑时）
    #[serde(default)]
    pub minecraft_version: Option<String>,
    #[serde(default)]
    pub loader: Option<String>,
    #[serde(default)]
    pub loader_version: Option<String>,
    /// 实例级"启动前校验完整性"覆盖；None 表示继承全局
    #[serde(default)]
    pub verify_game_on_launch: Option<bool>,
    /// 实例级"启动前校验资源"覆盖
    #[serde(default)]
    pub verify_resources_on_launch: Option<bool>,
    /// 实例级"安装前检查依赖"覆盖
    #[serde(default)]
    pub check_dependencies: Option<bool>,
}

/// 实例内的一个资源文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceResource {
    /// mods | shaderpacks | resourcepacks | datapacks
    pub kind: String,
    pub file_name: String,
    pub size: u64,
    pub enabled: bool,
}

/// 本地导入资源请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportLocalResourceRequest {
    pub version_id: String,
    /// mods | shaderpacks | resourcepacks | datapacks
    pub kind: String,
    pub path: String,
    /// 指定本地目录；非空时优先于实例目录
    #[serde(default)]
    pub target_dir: Option<String>,
}

/// 资源移动请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveResourceRequest {
    pub from_version: String,
    pub to_version: String,
    pub kind: String,
    pub file_name: String,
}

/// 安装前检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceCheckResult {
    /// 是否已安装同名/同项目资源
    pub already_installed: bool,
    /// 冲突文件名
    pub existing_file: Option<String>,
    /// 缺失的前置依赖（展示名）
    pub missing_dependencies: Vec<String>,
}

/// 安装日志事件（前端日志面板）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallLogEvent {
    /// info | warn | error | success
    pub level: String,
    pub message: String,
    /// 日志分类：install（安装/下载）| update（校验/修复/更新）
    #[serde(default = "default_log_category")]
    pub category: String,
}

fn default_log_category() -> String {
    "install".to_string()
}

/// 自定义资源来源
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSource {
    pub id: String,
    pub name: String,
    /// modrinth | curseforge | minebbs | spigot
    pub kind: String,
    /// API 根地址（可填镜像）
    #[serde(default)]
    pub api_base: String,
    /// 需要鉴权的来源填 Key
    #[serde(default)]
    pub api_key: String,
    /// 该来源可用于哪些资源分类（mod / plugin / skin / resourcepack / shader / datapack / modpack）
    #[serde(default)]
    pub categories: Vec<String>,
}

pub fn default_categories_for_kind(kind: &str) -> Vec<String> {
    let values: &[&str] = match kind {
        "spigot" => &["plugin"],
        "minebbs" => &["mod", "plugin", "resourcepack", "modpack"],
        "mcmod" => &["mod", "resourcepack", "datapack", "modpack"],
        "curseforge" => &[
            "mod",
            "plugin",
            "skin",
            "resourcepack",
            "shader",
            "datapack",
            "modpack",
        ],
        // Modrinth 及兼容源
        _ => &[
            "mod",
            "plugin",
            "resourcepack",
            "shader",
            "datapack",
            "modpack",
        ],
    };
    values.iter().map(|value| value.to_string()).collect()
}

impl ResourceSource {
    pub fn modrinth_official() -> Self {
        Self {
            id: "modrinth".to_string(),
            name: "Modrinth（官方）".to_string(),
            kind: "modrinth".to_string(),
            api_base: "https://api.modrinth.com/v2".to_string(),
            api_key: String::new(),
            categories: default_categories_for_kind("modrinth"),
        }
    }

    pub fn spigot_official() -> Self {
        Self {
            id: "spigot".to_string(),
            name: "Spigot（Spiget）".to_string(),
            kind: "spigot".to_string(),
            api_base: "https://api.spiget.org/v2".to_string(),
            api_key: String::new(),
            categories: default_categories_for_kind("spigot"),
        }
    }

    pub fn mcmod_official() -> Self {
        Self {
            id: "mcmod".to_string(),
            name: "MC百科（网页检索）".to_string(),
            kind: "mcmod".to_string(),
            api_base: "https://www.mcmod.cn".to_string(),
            api_key: String::new(),
            categories: default_categories_for_kind("mcmod"),
        }
    }

    pub fn effective_categories(&self) -> Vec<String> {
        if self.categories.is_empty() {
            default_categories_for_kind(&self.kind)
        } else {
            self.categories.clone()
        }
    }

    pub fn supports(&self, project_type: &str) -> bool {
        self.effective_categories()
            .iter()
            .any(|category| category == project_type)
    }
}

fn default_resource_sources() -> Vec<ResourceSource> {
    vec![
        ResourceSource::modrinth_official(),
        ResourceSource::spigot_official(),
        ResourceSource::mcmod_official(),
    ]
}

/// 资源版本列表请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResourceVersionsRequest {
    pub project_id: String,
    pub project_type: String,
    #[serde(default)]
    pub source_id: Option<String>,
}

/// 资源版本条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceVersionInfo {
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub downloads: u64,
    pub file_name: String,
    pub published: String,
}

/// 皮肤 / 披风档案
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkinProfileDto {
    pub name: String,
    pub uuid: String,
    pub skin_url: String,
    #[serde(default)]
    pub skin_data_url: String,
    pub cape_url: Option<String>,
    #[serde(default)]
    pub cape_data_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub source: String,
}

/// 本地皮肤文件（下载 / 导入 / 资源中心安装）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkinAssetDto {
    pub name: String,
    pub path: String,
    pub data_url: String,
    pub source: String,
    #[serde(default = "default_asset_kind")]
    pub kind: String,
}

fn default_asset_kind() -> String {
    "skin".to_string()
}

/// 游戏目录信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameDirInfo {
    pub path: String,
    pub exists: bool,
    pub installed_versions: usize,
    pub is_custom: bool,
}

/// 日志目录信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogDirInfo {
    pub path: String,
    pub exists: bool,
    pub is_custom: bool,
    pub file_count: usize,
    pub total_bytes: u64,
}

/// 清理结果（缓存 / 日志文件）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupResult {
    pub deleted_files: usize,
    pub deleted_dirs: usize,
    pub freed_bytes: u64,
    pub paths: Vec<String>,
}

/// 账户信息（不向前端暴露 access token）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub id: String,
    /// offline / external / premium
    pub kind: String,
    pub username: String,
    pub uuid: String,
    pub avatar_url: Option<String>,
    pub api_base: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateOfflineAccountRequest {
    pub id: Option<String>,
    pub username: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExternalLoginRequest {
    pub api_base: String,
    pub username: String,
    pub password: String,
}

/// 资源搜索请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSearchRequest {
    pub query: String,
    /// mod | shader | resourcepack | datapack | modpack
    pub project_type: String,
    #[serde(default)]
    pub game_version: Option<String>,
    #[serde(default)]
    pub loader: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
    /// 指定资源来源 id；None 用第一个来源
    #[serde(default)]
    pub source_id: Option<String>,
    /// 分页偏移
    #[serde(default)]
    pub offset: Option<u32>,
    /// 分类筛选（如优化/装饰/光影效果）
    #[serde(default)]
    pub category: Option<String>,
}

/// 资源搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub project_type: String,
}

/// 资源搜索分页结果（含命中总数，供前端计算总页数）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSearchResult {
    pub hits: Vec<ResourceHit>,
    pub total: u64,
}

/// 资源详情（点击资源后展示：描述 / 前置依赖 / 分类等）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDetail {
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    /// 前置依赖（项目标题）
    pub dependencies: Vec<String>,
    /// 来源类型：modrinth | curseforge | minebbs
    pub source_kind: String,
}

/// 资源详情请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDetailRequest {
    pub project_id: String,
    #[serde(default)]
    pub source_id: Option<String>,
}

/// 拖拽本地文件识别结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedResource {
    pub path: String,
    pub file_name: String,
    /// 识别出的资源类型：mod | shader | resourcepack | datapack | modpack | plugin | unknown
    pub project_type: String,
    /// 目标子目录：mods | shaderpacks | resourcepacks | datapacks
    pub kind: String,
    /// 推荐加载器（仅 mod 时可能给出：fabric / forge / neoforge）
    pub loader: Option<String>,
}

/// 安装资源请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResourceRequest {
    /// 目标实例对应的版本 id
    pub instance_version_id: String,
    pub project_id: String,
    #[serde(default)]
    pub version_id: Option<String>,
    /// mod | shader | resourcepack | datapack | modpack | plugin
    pub project_type: String,
    /// 指定资源来源 id
    #[serde(default)]
    pub source_id: Option<String>,
    /// 安装到外部目录（优先于实例目录）
    #[serde(default)]
    pub target_dir: Option<String>,
    /// 目标类型：instance | local | server | multiplayer
    #[serde(default)]
    pub target_kind: Option<String>,
}

/// 外链整合包导入请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportMrpackUrlRequest {
    pub url: String,
    /// 可选：导入到指定游戏目录
    #[serde(default)]
    pub target_dir: Option<String>,
}

/// 安装 Fabric 加载器请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallFabricRequest {
    /// 原版游戏版本（必须已安装）
    pub minecraft_version: String,
    /// 指定 loader 版本；None 表示取最新稳定版
    #[serde(default)]
    pub loader_version: Option<String>,
}

/// 安装 Forge 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallForgeRequest {
    /// 原版游戏版本（必须已安装）
    pub minecraft_version: String,
    /// 指定 Forge 版本；None 表示取推荐版
    #[serde(default)]
    pub forge_version: Option<String>,
}

/// 整合包导入结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportMrpackResult {
    pub pack_name: String,
    /// 导入到的目标版本（实例目录名）
    pub target_version: String,
    pub mc_version: String,
    pub downloaded: usize,
    pub skipped: usize,
    /// 解压的 overrides 文件数
    pub overrides: usize,
    pub errors: Vec<String>,
}

/// Mod 信息（前端展示用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModInfo {
    /// 展示用文件名（已去掉 .disabled 后缀）
    pub file_name: String,
    pub enabled: bool,
    pub size: u64,
}

pub fn default_download_source() -> String {
    "official".to_string()
}

pub fn default_download_concurrency() -> u32 {
    8
}

/// 可隔离的资源类型（目录名）
pub fn default_isolation_resources() -> Vec<String> {
    ["saves", "mods", "resourcepacks", "shaderpacks", "config", "datapacks"]
        .iter()
        .map(|value| value.to_string())
        .collect()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            username: "Player".to_string(),
            max_memory_mb: 2048,
            java_path: None,
            download_source: default_download_source(),
            download_concurrency: default_download_concurrency(),
            version_isolation: false,
            isolation_resources: default_isolation_resources(),
            modrinth_api: default_modrinth_api(),
            resource_sources: default_resource_sources(),
            check_dependencies: true,
            verify_resources_on_launch: false,
            verify_game_on_launch: true,
            launcher_language: default_language(),
            extra_jvm_args: String::new(),
            extra_game_args: String::new(),
            hide_install_logs: false,
            hide_update_logs: false,
            hide_launch_logs: false,
            plugin_repo_url: default_plugin_repo(),
            plugin_repo_branch: default_plugin_branch(),
            plugin_repo_token: String::new(),
            plugin_auto_update: true,
            plugin_check_updates: true,
            launcher_auto_update: true,
            resource_targets: Vec::new(),
        }
    }
}

impl AppSettings {
    /// 从 settings.json 读取；文件不存在或格式异常时回退默认值
    /// （旧版本设置文件缺少新字段时，由 serde default 补齐）
    pub fn load_or_default(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }
}

/// 校验报告（前端展示用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyReportDto {
    pub total: usize,
    pub valid: usize,
    pub missing: usize,
    pub corrupted: usize,
    pub missing_paths: Vec<String>,
    pub corrupted_paths: Vec<String>,
    pub bytes_to_download: u64,
}

impl From<mscl_core::VerifyReport> for VerifyReportDto {
    fn from(r: mscl_core::VerifyReport) -> Self {
        Self {
            total: r.total,
            valid: r.valid,
            missing: r.missing,
            corrupted: r.corrupted,
            missing_paths: r.missing_paths,
            corrupted_paths: r.corrupted_paths,
            bytes_to_download: r.bytes_to_download,
        }
    }
}

/// 安装进度事件（后端推送给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum InstallProgressEvent {
    Started {
        total: usize,
        to_download: usize,
        version_id: String,
    },
    TaskDone {
        path: String,
        skipped: bool,
        completed: usize,
        total: usize,
    },
    Finished {
        downloaded: usize,
        skipped: usize,
        failed: usize,
    },
    Error {
        message: String,
    },
    /// 多步骤任务（如创建实例）的阶段进度：step 从 1 开始；
    /// total = 0 表示该步骤没有更细的进度，只有阶段推进。
    Step {
        label: String,
        step: usize,
        steps: usize,
        completed: usize,
        total: usize,
    },
    /// 安装控制状态：running | paused | cancelled
    Control { state: String },
}

/// 游戏输出事件（后端推送给前端显示日志）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameOutputEvent {
    pub stream: String, // "stdout" | "stderr"
    pub line: String,
    /// 离线模式下已知的噪音行；前端"详细日志"关闭时可隐藏
    pub noise: bool,
}

/// 启动状态事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LaunchStatusEvent {
    Preparing { message: String },
    Generated { command_line: String },
    Started { pid: u32 },
    Exited { code: i32 },
    Failed { message: String },
}

/// Windows 窗口控制（前端标题栏按钮用）
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowAction {
    Minimize,
    Maximize,
    Close,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_progress_serializes_with_stage_tag() {
        // 前端用 stage 字段做判别联合，必须确保 tag 正确
        let event = InstallProgressEvent::TaskDone {
            path: "libraries/x.jar".into(),
            skipped: false,
            completed: 5,
            total: 10,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["stage"], "task_done");
        assert_eq!(json["completed"], 5);
        assert_eq!(json["skipped"], false);

        let started = InstallProgressEvent::Started {
            total: 100,
            to_download: 80,
            version_id: "26.3".into(),
        };
        let json = serde_json::to_value(&started).unwrap();
        assert_eq!(json["stage"], "started");
        assert_eq!(json["version_id"], "26.3");

        // 多步骤任务：前端按 step/steps 计算整体百分比
        let step = InstallProgressEvent::Step {
            label: "安装原版 1.21.1".into(),
            step: 1,
            steps: 4,
            completed: 3,
            total: 10,
        };
        let json = serde_json::to_value(&step).unwrap();
        assert_eq!(json["stage"], "step");
        assert_eq!(json["label"], "安装原版 1.21.1");
        assert_eq!(json["step"], 1);
        assert_eq!(json["steps"], 4);
        assert_eq!(json["total"], 10);
    }

    #[test]
    fn launch_status_serializes_with_status_tag() {
        let event = LaunchStatusEvent::Started { pid: 1234 };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["status"], "started");
        assert_eq!(json["pid"], 1234);

        let exited = LaunchStatusEvent::Exited { code: 0 };
        let json = serde_json::to_value(&exited).unwrap();
        assert_eq!(json["status"], "exited");
        assert_eq!(json["code"], 0);
    }

    #[test]
    fn window_action_deserializes_from_frontend() {
        // 前端传 snake_case 字符串
        let action: WindowAction = serde_json::from_str("\"minimize\"").unwrap();
        assert!(matches!(action, WindowAction::Minimize));
    }
}
