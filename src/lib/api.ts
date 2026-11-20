/**
 * MSCL 前端 API 封装
 * 把 Tauri invoke / listen 包装成类型安全的前端接口。
 * 这是前后端契约的 TS 侧，与 Rust 的 models.rs 一一对应。
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { BUILTIN_PLUGINS } from "./plugins";

// ---------- 数据类型（对应 Rust models.rs）----------

export interface VersionInfo {
  id: string;
  version_type: string;
  release_time: string;
  is_latest_release: boolean;
}

export interface VersionListResponse {
  latest_release: string;
  latest_snapshot: string;
  versions: VersionInfo[];
}

export interface JavaInfo {
  path: string;
  major_version: number | null;
  version_output: string;
}

export interface InstallResult {
  version_id: string;
  downloaded: number;
  skipped: number;
  failed: number;
  errors: string[];
  game_root: string;
}

export interface LaunchResult {
  pid: number;
  version_id: string;
  run_id: string;
  game_root: string;
  command_line: string;
}

export interface RunningGameInfo {
  run_id: string;
  version_id: string;
  pid: number;
  started_at: number;
  game_root: string;
}

export interface GameRootInfo {
  path: string;
  is_primary: boolean;
  exists: boolean;
  instance_count: number;
}

export interface InstanceLaunchOptions {
  max_memory_mb: number | null;
  jvm_args: string;
  game_args: string;
}

export interface BackendStatus {
  core: string;
  os: string;
  game_root: string;
  running_pid: number | null;
  running_instances?: RunningGameInfo[];
}

export const ISOLATION_RESOURCE_TYPES = [
  "saves",
  "mods",
  "resourcepacks",
  "shaderpacks",
  "config",
  "datapacks",
] as const;

export interface IsolationEntry {
  kind: string;
  /** isolated | shared | shared_with_files | unknown */
  state: string;
  target: string | null;
}

export interface IsolationOverview {
  version_isolation: boolean;
  types: { kind: string; isolated: boolean }[];
}

export interface AppSettings {
  username: string;
  max_memory_mb: number;
  java_path: string | null;
  /** 下载源：official / bmclapi / 自定义根地址 */
  download_source: string;
  /** 并发下载数 */
  download_concurrency: number;
  /** 版本隔离：每版本独立存档 / Mod / 配置 */
  version_isolation: boolean;
  /** 自定义隔离的资源类型；空数组按旧行为视为全部隔离 */
  isolation_resources: string[];
  /** 资源站 API 根地址（Modrinth 兼容，旧字段） */
  modrinth_api: string;
  /** 自定义资源来源列表 */
  resource_sources: ResourceSource[];
  /** 安装前检查重复与前置依赖 */
  check_dependencies: boolean;
  /** 启动前校验资源完整性（较慢） */
  verify_resources_on_launch: boolean;
  /** 启动前校验游戏文件完整性 */
  verify_game_on_launch: boolean;
  /** 启动器语言 */
  launcher_language: string;
  /** 额外 JVM 参数 */
  extra_jvm_args: string;
  /** 额外游戏参数 */
  extra_game_args: string;
  /** 隐藏安装日志（底部状态坞） */
  hide_install_logs: boolean;
  /** 隐藏更新日志（校验 / 修复 / 更新） */
  hide_update_logs: boolean;
  /** 隐藏启动日志（首页游戏控制台） */
  hide_launch_logs: boolean;
  /** 插件仓库地址（GitHub） */
  plugin_repo_url: string;
  /** 插件仓库分支 */
  plugin_repo_branch: string;
  /** 插件仓库访问 Token（可选） */
  plugin_repo_token: string;
  /** 是否随程序更新默认插件库 */
  plugin_auto_update: boolean;
  /** 是否自动检查插件仓库更新 */
  plugin_check_updates: boolean;
  /** 是否自动检查并更新启动器本体 */
  launcher_auto_update: boolean;
  /** 自定义资源安装目录 */
  resource_targets: ResourceTargetDir[];
}

export interface ResourceTargetDir {
  id: string;
  name: string;
  path: string;
  kind: "local" | "server" | "multiplayer";
}

export interface ResourceTarget {
  id: string;
  label: string;
  path: string;
  kind: "instance" | "local" | "server" | "multiplayer" | "launcher";
}

export interface BackupOptions {
  settings: boolean;
  plugins: boolean;
  instances: boolean;
  accounts: boolean;
  logs: boolean;
  extraPaths: string[];
}

export interface BackupSummary {
  id: string;
  path: string;
  createdAt: number;
  appVersion: string;
  note: string;
  size: number;
  items: string[];
}

export interface StartupUpdateCheck {
  launcher: PluginRelease | null;
  plugins: PluginRelease | null;
  launcher_auto_update: boolean;
  plugins_auto_update: boolean;
}

/** 插件 manifest（与后端 PluginManifest 一致） */
export interface PluginConfigField {
  key: string;
  label: string;
  type: "string" | "number" | "boolean" | "select" | "string[]";
  default: string | number | boolean | string[];
  options?: { value: string; label: string }[];
  hint?: string;
  min?: number;
  max?: number;
}

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  category: "core" | "launcher" | "server";
  description: string;
  author: string;
  entry: string;
  enabled: boolean;
  builtin: boolean;
  required: boolean;
  source: "builtin" | "local" | "github";
  repo?: string | null;
  capabilities: string[];
  icon?: string | null;
  layer?: "harness" | "module" | "feature";
  hooks?: string[];
  extensionPoints?: { point: string; key: string }[];
  dependencies?: string[];
  configSchema?: PluginConfigField[];
  order?: number;
  schema?: number;
}

export interface PluginEntry extends PluginManifest {
  path: string;
  loaded: boolean;
}

export interface PluginRelease {
  tag: string;
  body: string;
  published_at: string;
  html_url: string;
}

export interface ModInfo {
  file_name: string;
  enabled: boolean;
  size: number;
}

export interface InstanceInfo {
  version_id: string;
  name: string;
  minecraft_version: string;
  loader: string;
  loader_version: string | null;
  description: string;
  icon: string;
  tags: string[];
  verify_game_on_launch: boolean | null;
  verify_resources_on_launch: boolean | null;
  check_dependencies: boolean | null;
  /** 实例所属游戏目录 */
  game_root?: string;
}

export interface CreateInstanceRequest {
  name: string;
  minecraft_version: string;
  loader: "vanilla" | "fabric" | "forge" | "neoforge";
  loader_version?: string | null;
  install_fabric_api?: boolean;
  install_optifine?: boolean;
  description?: string;
  icon?: string;
  tags?: string[];
}

export interface ResourceHit {
  project_id: string;
  slug: string;
  title: string;
  description: string;
  icon_url: string | null;
  downloads: number;
  project_type: string;
}

export interface ResourceSearchResult {
  hits: ResourceHit[];
  total: number;
}

export interface ResourceDetail {
  project_id: string;
  title: string;
  description: string;
  icon_url: string | null;
  downloads: number;
  dependencies: string[];
  source_kind: string;
}

export interface DetectedResource {
  path: string;
  file_name: string;
  project_type: string;
  kind: string;
  loader: string | null;
}

export interface UpdateInstanceMetaRequest {
  version_id: string;
  name: string;
  description: string;
  icon: string;
  tags: string[];
  verify_game_on_launch?: boolean | null;
  verify_resources_on_launch?: boolean | null;
  check_dependencies?: boolean | null;
  minecraft_version?: string | null;
  loader?: string | null;
  loader_version?: string | null;
}

export interface InstanceResource {
  kind: string;
  file_name: string;
  size: number;
  enabled: boolean;
}

export interface ResourceSource {
  id: string;
  name: string;
  kind: string;
  api_base: string;
  api_key: string;
  /** 该来源支持的资源分类 */
  categories?: string[];
}

export const RESOURCE_CATEGORIES = [
  "mod",
  "plugin",
  "skin",
  "resourcepack",
  "shader",
  "datapack",
  "modpack",
] as const;

export function defaultCategoriesForKind(kind: string): string[] {
  switch (kind) {
    case "spigot":
      return ["plugin"];
    case "minebbs":
      return ["mod", "plugin", "resourcepack", "modpack"];
    case "mcmod":
      return ["mod", "resourcepack", "datapack", "modpack"];
    case "curseforge":
      return ["mod", "plugin", "skin", "resourcepack", "shader", "datapack", "modpack"];
    default:
      return ["mod", "plugin", "resourcepack", "shader", "datapack", "modpack"];
  }
}

export function sourceSupports(source: ResourceSource, category: string): boolean {
  const categories = source.categories?.length
    ? source.categories
    : defaultCategoriesForKind(source.kind);
  return categories.includes(category);
}

export interface ResourceVersionInfo {
  id: string;
  name: string;
  version_number: string;
  game_versions: string[];
  loaders: string[];
  downloads: number;
  file_name: string;
  published: string;
}

export interface AccountInfo {
  id: string;
  kind: "offline" | "external" | "premium";
  username: string;
  uuid: string;
  avatar_url: string | null;
  api_base: string | null;
  is_active: boolean;
}

export interface SkinProfile {
  name: string;
  uuid: string;
  skin_url: string;
  skin_data_url?: string;
  cape_url: string | null;
  cape_data_url?: string;
  model?: string;
  source?: string;
}

export interface SkinAsset {
  name: string;
  path: string;
  data_url: string;
  source: string;
  kind?: string;
}

export interface InstanceMetrics {
  run_id: string;
  version_id: string;
  pid: number;
  game_root: string;
  cpu_percent: number;
  memory_bytes: number;
  uptime_secs: number;
}

export interface RuntimeMetrics {
  timestamp: number;
  system_cpu_percent: number;
  system_memory_used: number;
  system_memory_total: number;
  instances: InstanceMetrics[];
}

export interface GameDirInfo {
  path: string;
  exists: boolean;
  installed_versions: number;
  is_custom: boolean;
}

export interface LogDirInfo {
  path: string;
  exists: boolean;
  is_custom: boolean;
  file_count: number;
  total_bytes: number;
}

export interface CleanupResult {
  deleted_files: number;
  deleted_dirs: number;
  freed_bytes: number;
  paths: string[];
}

export interface UninstallInfo {
  supported: boolean;
  uninstaller: string | null;
  install_dir: string | null;
}

export interface ResourceCheckResult {
  already_installed: boolean;
  existing_file: string | null;
  missing_dependencies: string[];
}

export interface InstallLogEvent {
  level: "info" | "warn" | "error" | "success" | string;
  message: string;
  /** install（安装/下载，默认）| update（校验/修复/更新） */
  category?: "install" | "update" | string;
}

export interface ImportMrpackResult {
  pack_name: string;
  target_version: string;
  mc_version: string;
  downloaded: number;
  skipped: number;
  overrides: number;
  errors: string[];
}

export interface VerifyReport {
  total: number;
  valid: number;
  missing: number;
  corrupted: number;
  missing_paths: string[];
  corrupted_paths: string[];
  bytes_to_download: number;
}

// ---------- 事件类型 ----------

export type InstallProgressEvent =
  | { stage: "started"; total: number; to_download: number; version_id: string }
  | { stage: "task_done"; path: string; skipped: boolean; completed: number; total: number }
  | { stage: "finished"; downloaded: number; skipped: number; failed: number }
  | { stage: "error"; message: string }
  /** 多步骤任务（创建实例）：step 从 1 开始，total = 0 表示只有阶段进度 */
  | {
      stage: "step";
      label: string;
      step: number;
      steps: number;
      completed: number;
      total: number;
    }
  /** 安装控制状态：running | paused | cancelled */
  | { stage: "control"; state: "running" | "paused" | "cancelled" | string };

export type LaunchStatusEvent =
  | { status: "preparing"; message: string }
  | { status: "generated"; command_line: string }
  | { status: "started"; pid: number }
  | { status: "exited"; code: number }
  | { status: "failed"; message: string };

export interface GameOutputEvent {
  stream: "stdout" | "stderr";
  line: string;
  /** 离线模式已知噪音行，详细日志关闭时可隐藏 */
  noise: boolean;
}

export type WindowAction = "minimize" | "maximize" | "close";

// ---------- 运行环境检测 ----------
// Tauri 2 在 WebView 启动时注入 window.__TAURI_INTERNALS__。
// 普通浏览器（设计预览 / 单元测试）没有该对象，直接调用 invoke/listen
// 会在 @tauri-apps/api 内部读取 transformCallback 时抛 TypeError。
// 这里统一拦截：非 Tauri 环境返回预览数据，保证 UI 可独立开发验证。

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const PREVIEW_VERSION = "1.21.11";

// 预览模式下模拟设置持久化：保存后再次读取能拿到最新值
let previewSettings: AppSettings = {
  username: "Player",
  max_memory_mb: 2048,
  java_path: null,
  download_source: "official",
  download_concurrency: 8,
  version_isolation: false,
  isolation_resources: [...ISOLATION_RESOURCE_TYPES],
  modrinth_api: "https://api.modrinth.com/v2",
  resource_sources: [
    {
      id: "modrinth",
      name: "Modrinth（官方）",
      kind: "modrinth",
      api_base: "https://api.modrinth.com/v2",
      api_key: "",
      categories: ["mod", "plugin", "resourcepack", "shader", "datapack", "modpack"],
    },
    {
      id: "spigot",
      name: "Spigot（Spiget）",
      kind: "spigot",
      api_base: "https://api.spiget.org/v2",
      api_key: "",
      categories: ["plugin"],
    },
    {
      id: "mcmod",
      name: "MC百科（网页检索）",
      kind: "mcmod",
      api_base: "https://www.mcmod.cn",
      api_key: "",
      categories: ["mod", "resourcepack", "datapack", "modpack"],
    },
  ],
  check_dependencies: true,
  verify_resources_on_launch: false,
  verify_game_on_launch: true,
  launcher_language: "zh-CN",
  extra_jvm_args: "",
  extra_game_args: "",
  hide_install_logs: false,
  hide_update_logs: false,
  hide_launch_logs: false,
  plugin_repo_url: "https://github.com/mscl-team/mscl-plugins",
  plugin_repo_branch: "main",
  plugin_repo_token: "",
  plugin_auto_update: true,
  plugin_check_updates: true,
  launcher_auto_update: true,
  resource_targets: [],
};

// 预览模式的 Mod 列表（模拟启停与删除）
let previewMods: ModInfo[] = [
  { file_name: "fabric-api-0.92.2.jar", enabled: true, size: 2_180_000 },
  { file_name: "sodium-0.5.11.jar", enabled: false, size: 1_240_000 },
];

function previewResponse<T>(command: string, args?: Record<string, unknown>): T {
  switch (command) {
    case "backend_status":
      return {
        core: "mscl-core ready (浏览器预览模式)",
        os: "preview",
        game_root: "（预览模式，不访问磁盘）",
        running_pid: null,
      } as T;
    case "get_version_list":
      return {
        latest_release: PREVIEW_VERSION,
        latest_snapshot: "26.1-snapshot",
        versions: [
          { id: PREVIEW_VERSION, version_type: "release", release_time: "2026-06-10", is_latest_release: true },
          { id: "1.21.10", version_type: "release", release_time: "2026-03-18", is_latest_release: false },
        ],
      } as T;
    case "get_installed_versions":
      return [PREVIEW_VERSION] as T;
    case "detect_java":
      return {
        path: "（预览模式）",
        major_version: 21,
        version_output: "预览数据",
      } as T;
    case "install_game":
      return {
        version_id: PREVIEW_VERSION,
        downloaded: 0,
        skipped: 0,
        failed: 0,
        errors: [],
        game_root: "（预览模式）",
      } as T;
    case "launch_game":
      return {
        pid: 0,
        version_id: "preview",
        run_id: "preview-run",
        game_root: "（预览模式）",
        command_line: "（预览模式，未启动进程）",
      } as T;
    case "get_running_instances":
      return [] as T;
    case "stop_run":
      return undefined as T;
    case "stop_version":
      return 0 as T;
    case "get_instance_launch_options":
      return { max_memory_mb: null, jvm_args: "", game_args: "" } as T;
    case "save_instance_launch_options":
      return undefined as T;
    case "get_settings":
      return { ...previewSettings } as T;
    case "save_settings": {
      const settings = args?.settings as AppSettings | undefined;
      if (settings) previewSettings = { ...settings };
      return undefined as T;
    }
    case "export_settings": {
      const keys = (args?.keys as string[] | undefined) ?? [];
      const selected: Record<string, unknown> = {};
      for (const key of keys) {
        if (key in previewSettings) selected[key] = previewSettings[key as keyof AppSettings];
      }
      return {
        type: "mscl-settings",
        version: 1,
        exported_at: 0,
        settings: selected,
      } as T;
    }
    case "import_settings":
      return { ...previewSettings } as T;
    case "save_settings_file":
      return undefined as T;
    case "read_settings_file":
      return JSON.stringify({
        type: "mscl-settings",
        version: 1,
        settings: {},
      }) as T;
    case "get_instance_isolation":
      return [] as T;
    case "get_isolation_overview":
      return {
        version_isolation: false,
        types: [],
      } as T;
    case "verify_version":
      return {
        total: 1234,
        valid: 1234,
        missing: 0,
        corrupted: 0,
        missing_paths: [],
        corrupted_paths: [],
        bytes_to_download: 0,
      } as T;
    case "repair_version":
      return {
        version_id: PREVIEW_VERSION,
        downloaded: 0,
        skipped: 1234,
        failed: 0,
        errors: [],
        game_root: "（预览模式）",
      } as T;
    case "install_fabric":
      return {
        version_id: "fabric-loader-0.16.9-1.21.1",
        downloaded: 12,
        skipped: 0,
        failed: 0,
        errors: [],
        game_root: "（预览模式）",
      } as T;
    case "install_forge":
      return {
        version_id: "1.21.1-forge-47.2.0",
        downloaded: 0,
        skipped: 0,
        failed: 0,
        errors: [],
        game_root: "（预览模式）",
      } as T;
    case "create_instance":
      return {
        version_id: "fabric-loader-0.20.1-1.21.1",
        downloaded: 0,
        skipped: 0,
        failed: 0,
        errors: [],
        game_root: "（预览模式）",
      } as T;
    case "cancel_instance_install":
    case "pause_instance_install":
    case "resume_instance_install":
      return undefined as T;
    case "list_instances":
      return [
        {
          version_id: "fabric-loader-0.20.1-1.21.1",
          name: "我的整合体验",
          minecraft_version: "1.21.1",
          loader: "fabric",
          loader_version: "0.20.1",
        },
      ] as T;
    case "list_all_instances":
      return [
        {
          version_id: "fabric-loader-0.20.1-1.21.1",
          name: "我的整合体验",
          minecraft_version: "1.21.1",
          loader: "fabric",
          loader_version: "0.20.1",
          game_root: "（预览模式）",
        },
      ] as T;
    case "list_game_roots":
      return [
        {
          path: "（预览模式）",
          is_primary: true,
          exists: true,
          instance_count: 1,
        },
      ] as T;
    case "add_game_root":
    case "remove_game_root":
      return [
        {
          path: "（预览模式）",
          is_primary: true,
          exists: true,
          instance_count: 1,
        },
      ] as T;
    case "search_resources":
      return {
        hits: [
          {
            project_id: "sodium",
            slug: "sodium",
            title: "Sodium",
            description: "现代渲染优化模组，大幅提升帧率",
            icon_url: null,
            downloads: 12000000,
            project_type: "mod",
          },
          {
            project_id: "iris",
            slug: "iris",
            title: "Iris Shaders",
            description: "兼容 OptiFine 光影的着色器加载器",
            icon_url: null,
            downloads: 8000000,
            project_type: "mod",
          },
        ],
        total: 2,
      } as T;
    case "get_resource_detail":
      return {
        project_id: "sodium",
        title: "Sodium",
        description: "现代渲染优化模组，大幅提升帧率。",
        icon_url: null,
        downloads: 12000000,
        dependencies: [],
        source_kind: "modrinth",
      } as T;
    case "detect_resource_file":
      return {
        path: (args?.path as string) ?? "",
        file_name: "local-mod.jar",
        project_type: "mod",
        kind: "mods",
        loader: "fabric",
      } as T;
    case "install_resource":
      return {
        version_id: "fabric-loader-0.20.1-1.21.1",
        downloaded: 1,
        skipped: 0,
        failed: 0,
        errors: [],
        game_root: "（预览模式）",
      } as T;
    case "check_resource_install":
      return {
        already_installed: false,
        existing_file: null,
        missing_dependencies: [],
      } as T;
    case "list_resource_versions":
      return [
        {
          id: "v1",
          name: "1.21.1-0.5.11",
          version_number: "0.5.11",
          game_versions: ["1.21.1"],
          loaders: ["fabric"],
          downloads: 120000,
          file_name: "sodium-0.5.11.jar",
          published: "2026-01-01",
        },
      ] as T;
    case "list_accounts":
      return [
        {
          id: "preview-offline",
          kind: "offline",
          username: "Player",
          uuid: "offline-player",
          avatar_url: null,
          api_base: null,
          is_active: true,
        },
      ] as T;
    case "get_active_account":
      return {
        id: "preview-offline",
        kind: "offline",
        username: "Player",
        uuid: "offline-player",
        avatar_url: null,
        api_base: null,
        is_active: true,
      } as T;
    case "upsert_offline_account":
    case "authenticate_external":
      return {
        id: "preview-account",
        kind: "offline",
        username: "Player",
        uuid: "offline-player",
        avatar_url: null,
        api_base: null,
        is_active: true,
      } as T;
    case "set_active_account":
    case "delete_account":
      return [] as T;
    case "fetch_skin_profile":
      return {
        name: "Player",
        uuid: "00000000000000000000000000000000",
        skin_url: "",
        skin_data_url: "",
        cape_url: null,
        model: "classic",
        source: "mojang",
      } as T;
    case "download_skin":
    case "download_cape":
    case "save_skin_data":
    case "save_cape_data":
    case "import_skin_file":
    case "import_cape_file":
      return {
        name: "preview",
        path: "（预览模式）",
        data_url: "",
        source: "download",
        kind: "skin",
      } as T;
    case "list_local_skins":
      return [] as T;
    case "get_runtime_metrics":
      return {
        timestamp: Date.now(),
        system_cpu_percent: 0,
        system_memory_used: 0,
        system_memory_total: 0,
        instances: [],
      } as T;
    case "get_game_dir_info":
      return {
        path: "（预览模式）",
        exists: true,
        installed_versions: 2,
        is_custom: false,
      } as T;
    case "set_game_dir":
      return {
        path: "（预览模式）",
        exists: true,
        installed_versions: 0,
        is_custom: true,
      } as T;
    case "get_log_dir_info":
    case "set_log_dir":
    case "reset_log_dir":
      return {
        path: "（预览模式）/logs",
        exists: true,
        is_custom: false,
        file_count: 3,
        total_bytes: 24576,
      } as T;
    case "open_log_dir":
      return undefined as T;
    case "clear_log_files":
    case "clear_cache":
      return {
        deleted_files: 3,
        deleted_dirs: 0,
        freed_bytes: 24576,
        paths: [],
      } as T;
    case "reset_settings":
      return {
        username: "Player",
        max_memory_mb: 0,
        java_path: null,
        download_source: "official",
        download_concurrency: 8,
        version_isolation: false,
        isolation_resources: [...ISOLATION_RESOURCE_TYPES],
        modrinth_api: "https://api.modrinth.com/v2",
        resource_sources: [],
        check_dependencies: true,
        verify_resources_on_launch: false,
        verify_game_on_launch: true,
        launcher_language: "zh-CN",
        extra_jvm_args: "",
        extra_game_args: "",
        hide_install_logs: false,
        hide_update_logs: false,
        hide_launch_logs: false,
        plugin_repo_url: "https://github.com/mscl-team/mscl-plugins",
        plugin_repo_branch: "main",
        plugin_repo_token: "",
        plugin_auto_update: true,
        plugin_check_updates: true,
        launcher_auto_update: true,
        resource_targets: [],
      } as T;
    case "delete_game_data":
      return undefined as T;
    case "list_instance_resources":
      return [
        { kind: "mods", file_name: "sodium-0.5.11.jar", size: 1240000, enabled: true },
        { kind: "shaderpacks", file_name: "complementary.zip", size: 3400000, enabled: false },
      ] as T;
    case "import_local_resource":
      return {
        kind: "mods",
        file_name: "local-mod.jar",
        size: 1024,
        enabled: true,
      } as T;
    case "update_instance_meta":
    case "delete_instance":
    case "delete_instance_resource":
    case "move_instance_resource":
      return undefined as T;
    case "import_mrpack_url":
      return {
        pack_name: "外链整合包",
        target_version: "fabric-loader-0.20.1-1.21.1",
        mc_version: "1.21.1",
        downloaded: 20,
        skipped: 0,
        overrides: 5,
        errors: [],
      } as T;
    case "import_mrpack":
      return {
        pack_name: "示例整合包",
        target_version: "fabric-loader-0.20.1-26.3",
        mc_version: "26.3",
        downloaded: 24,
        skipped: 0,
        overrides: 8,
        errors: [],
      } as T;
    case "list_mods":
      return [...previewMods] as T;
    case "toggle_mod": {
      const name = args?.fileName as string | undefined;
      const enabled = Boolean(args?.enabled);
      previewMods = previewMods.map((m) =>
        m.file_name === name ? { ...m, enabled } : m
      );
      return undefined as T;
    }
    case "delete_mod": {
      const name = args?.fileName as string | undefined;
      previewMods = previewMods.filter((m) => m.file_name !== name);
      return undefined as T;
    }
    case "stop_game":
    case "uninstall_version":
    case "clear_game_pid":
    case "open_instance_dir":
    case "window_control":
      return undefined as T;
    case "list_installed_plugins":
      return [] as T;
    case "get_default_plugin_config":
      return BUILTIN_PLUGINS.filter((p) => !p.required).map((p) => p.id) as T;
    case "get_plugin_manifest": {
      const found = BUILTIN_PLUGINS.find((p) => p.id === args?.id);
      if (!found) throw new Error("插件不存在（预览模式）");
      return { ...found, schema: 1 } as T;
    }
    case "get_plugin_config":
      return {} as T;
    case "set_plugin_config":
    case "set_plugin_enabled":
    case "write_default_plugin_config":
    case "uninstall_plugin":
    case "write_plugin_export_file":
    case "import_plugin_configs":
    case "import_plugin_configs_from_file":
    case "install_local_plugin":
    case "install_github_plugin":
    case "add_resource_target_dir":
    case "remove_resource_target_dir":
    case "delete_launcher_backup":
      return undefined as T;
    case "install_builtin_plugin": {
      const found = BUILTIN_PLUGINS.find(
        (p) => p.id === (args?.manifest as { id?: string } | undefined)?.id
      );
      return { ...(found ?? BUILTIN_PLUGINS[0]), path: "（预览模式）", loaded: true } as T;
    }
    case "validate_plugin_manifest":
      return [] as T;
    case "read_plugin_manifest_from_dir":
      return { id: "preview-plugin", name: "预览插件" } as T;
    case "export_plugin_configs":
      return { schema: 1, plugins: [] } as T;
    case "check_plugin_update":
    case "check_plugin_repo_updates":
    case "check_launcher_updates":
      return null as T;
    case "check_startup_updates":
      return {
        launcher: null,
        plugins: null,
        launcher_auto_update: true,
        plugins_auto_update: true,
      } as T;
    case "update_plugin_repo_safe":
    case "prepare_launcher_update":
      return "预览模式不会执行更新" as T;
    case "list_resource_targets":
      return [] as T;
    case "create_launcher_backup":
      return {
        id: "backup-preview",
        path: "（预览模式）",
        createdAt: 0,
        appVersion: "0.1.0",
        note: "",
        size: 0,
        items: [],
      } as T;
    case "list_launcher_backups":
      return [] as T;
    case "restore_launcher_backup":
      return "预览模式不会还原数据" as T;
    default:
      return undefined as T;
  }
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriRuntime()) {
    return previewResponse<T>(command, args);
  }
  return invoke<T>(command, args);
}

// ---------- Commands ----------

// Java 检测要启动子进程，切换页面时不应重复执行。
// 缓存 60 秒；传 force=true 可绕过（如用户手动"重新检测"）。
let javaCacheInfo: JavaInfo | null = null;
let javaCacheAt = 0;
const JAVA_CACHE_TTL = 60_000;

export const api = {
  backendStatus: () => call<BackendStatus>("backend_status"),

  getVersionList: () => call<VersionListResponse>("get_version_list"),

  getInstalledVersions: () => call<string[]>("get_installed_versions"),

  detectJava: (force = false): Promise<JavaInfo> => {
    if (!force && javaCacheInfo && Date.now() - javaCacheAt < JAVA_CACHE_TTL) {
      return Promise.resolve(javaCacheInfo);
    }
    return call<JavaInfo>("detect_java").then((info) => {
      javaCacheInfo = info;
      javaCacheAt = Date.now();
      return info;
    });
  },

  installGame: (versionId: string) =>
    call<InstallResult>("install_game", { req: { version_id: versionId } }),

  launchGame: (versionId: string, username: string, maxMemoryMb: number = 0) =>
    call<LaunchResult>("launch_game", {
      req: { version_id: versionId, username, max_memory_mb: maxMemoryMb },
    }),

  launchInstance: (req: {
    version_id: string;
    username: string;
    max_memory_mb?: number;
    game_root?: string | null;
    jvm_args?: string | null;
    game_args?: string | null;
  }) =>
    call<LaunchResult>("launch_game", {
      req: {
        version_id: req.version_id,
        username: req.username,
        max_memory_mb: req.max_memory_mb ?? 0,
        game_root: req.game_root ?? null,
        jvm_args: req.jvm_args ?? null,
        game_args: req.game_args ?? null,
      },
    }),

  getRunningInstances: () => call<RunningGameInfo[]>("get_running_instances"),

  stopRun: (runId: string) => call<void>("stop_run", { runId }),

  stopVersion: (versionId: string) =>
    call<number>("stop_version", { versionId }),

  getInstanceLaunchOptions: (versionId: string, gameRoot?: string | null) =>
    call<InstanceLaunchOptions>("get_instance_launch_options", {
      versionId,
      gameRoot: gameRoot ?? null,
    }),

  saveInstanceLaunchOptions: (
    versionId: string,
    options: InstanceLaunchOptions,
    gameRoot?: string | null
  ) =>
    call<void>("save_instance_launch_options", {
      versionId,
      gameRoot: gameRoot ?? null,
      options,
    }),

  getSettings: () => call<AppSettings>("get_settings"),

  saveSettings: (settings: AppSettings) =>
    call<void>("save_settings", { settings }),

  exportSettings: (keys: string[]) =>
    call<Record<string, unknown>>("export_settings", { keys }),

  importSettings: (payload: Record<string, unknown>) =>
    call<AppSettings>("import_settings", { payload }),

  saveSettingsFile: (path: string, payload: Record<string, unknown>) =>
    call<void>("save_settings_file", { path, payload }),

  readSettingsFile: (path: string) =>
    call<string>("read_settings_file", { path }),

  getInstanceIsolation: (versionId: string) =>
    call<IsolationEntry[]>("get_instance_isolation", { versionId }),

  getIsolationOverview: () => call<IsolationOverview>("get_isolation_overview"),

  verifyVersion: (versionId: string) =>
    call<VerifyReport>("verify_version", { versionId }),

  repairVersion: (versionId: string) =>
    call<InstallResult>("repair_version", { versionId }),

  installFabric: (minecraftVersion: string, loaderVersion?: string) =>
    call<InstallResult>("install_fabric", {
      req: {
        minecraft_version: minecraftVersion,
        loader_version: loaderVersion ?? null,
      },
    }),

  installForge: (minecraftVersion: string, forgeVersion?: string) =>
    call<InstallResult>("install_forge", {
      req: {
        minecraft_version: minecraftVersion,
        forge_version: forgeVersion ?? null,
      },
    }),

  importMrpack: (path: string, targetDir?: string | null) =>
    call<ImportMrpackResult>("import_mrpack", { path, targetDir: targetDir ?? null }),

  importMrpackUrl: (url: string, targetDir?: string | null) =>
    call<ImportMrpackResult>("import_mrpack_url", { req: { url, targetDir: targetDir ?? null } }),

  createInstance: (req: CreateInstanceRequest) =>
    call<InstallResult>("create_instance", { req }),

  /** 安全中断当前实例安装（已完成的缓存文件保留） */
  cancelInstanceInstall: () => call<void>("cancel_instance_install"),

  /** 暂停当前实例安装（保留已完成文件，可继续） */
  pauseInstanceInstall: () => call<void>("pause_instance_install"),

  /** 继续当前实例安装 */
  resumeInstanceInstall: () => call<void>("resume_instance_install"),

  listInstances: () => call<InstanceInfo[]>("list_instances"),

  listAllInstances: () => call<InstanceInfo[]>("list_all_instances"),

  listGameRoots: () => call<GameRootInfo[]>("list_game_roots"),

  addGameRoot: (path: string) => call<GameRootInfo[]>("add_game_root", { path }),

  removeGameRoot: (path: string) => call<GameRootInfo[]>("remove_game_root", { path }),

  updateInstanceMeta: (req: UpdateInstanceMetaRequest) =>
    call<void>("update_instance_meta", { req }),

  deleteInstance: (versionId: string) =>
    call<void>("delete_instance", { versionId }),

  listInstanceResources: (versionId: string) =>
    call<InstanceResource[]>("list_instance_resources", { versionId }),

  deleteInstanceResource: (versionId: string, kind: string, fileName: string) =>
    call<void>("delete_instance_resource", { versionId, kind, fileName }),

  moveInstanceResource: (req: {
    from_version: string;
    to_version: string;
    kind: string;
    file_name: string;
  }) => call<void>("move_instance_resource", { req }),

  importLocalResource: (req: {
    version_id: string;
    kind: string;
    path: string;
    target_dir?: string | null;
  }) => call<InstanceResource>("import_local_resource", {
    req: {
      version_id: req.version_id,
      kind: req.kind,
      path: req.path,
      target_dir: req.target_dir ?? null,
    },
  }),

  checkResourceInstall: (req: {
    instance_version_id: string;
    project_id: string;
    version_id?: string | null;
    project_type: string;
    source_id?: string | null;
    target_dir?: string | null;
    target_kind?: string | null;
  }) => call<ResourceCheckResult>("check_resource_install", { req }),

  searchResources: (req: {
    query: string;
    project_type: string;
    game_version?: string | null;
    loader?: string | null;
    limit?: number;
    source_id?: string | null;
    offset?: number;
    category?: string | null;
  }) =>
    call<ResourceSearchResult>("search_resources", {
      req: {
        query: req.query,
        project_type: req.project_type,
        game_version: req.game_version ?? null,
        loader: req.loader ?? null,
        limit: req.limit ?? 20,
        source_id: req.source_id ?? null,
        offset: req.offset ?? 0,
        category: req.category ?? null,
      },
    }),

  installResource: (req: {
    instance_version_id: string;
    project_id: string;
    version_id?: string | null;
    project_type: string;
    source_id?: string | null;
    target_dir?: string | null;
    target_kind?: string | null;
  }) => call<InstallResult>("install_resource", { req }),

  listResourceVersions: (projectId: string, projectType: string, sourceId?: string) =>
    call<ResourceVersionInfo[]>("list_resource_versions", {
      req: {
        project_id: projectId,
        project_type: projectType,
        source_id: sourceId ?? null,
      },
    }),

  getResourceDetail: (projectId: string, sourceId?: string) =>
    call<ResourceDetail>("get_resource_detail", {
      req: { project_id: projectId, source_id: sourceId ?? null },
    }),

  detectResourceFile: (path: string) =>
    call<DetectedResource>("detect_resource_file", { path }),

  fetchSkinProfile: (username: string, source?: "mojang" | "namemc") =>
    call<SkinProfile>("fetch_skin_profile", {
      username,
      source: source ?? "mojang",
    }),

  downloadSkin: (url: string, name: string) =>
    call<SkinAsset>("download_skin", { url, name }),

  downloadCape: (url: string, name: string) =>
    call<SkinAsset>("download_cape", { url, name }),

  saveSkinData: (name: string, data: string) =>
    call<SkinAsset>("save_skin_data", { name, data }),

  saveCapeData: (name: string, data: string) =>
    call<SkinAsset>("save_cape_data", { name, data }),

  importSkinFile: (path: string) =>
    call<SkinAsset>("import_skin_file", { path }),

  importCapeFile: (path: string) =>
    call<SkinAsset>("import_cape_file", { path }),

  listLocalSkins: () => call<SkinAsset[]>("list_local_skins"),

  getRuntimeMetrics: () => call<RuntimeMetrics>("get_runtime_metrics"),

  listAccounts: () => call<AccountInfo[]>("list_accounts"),

  getActiveAccount: () => call<AccountInfo | null>("get_active_account"),

  upsertOfflineAccount: (req: {
    id?: string | null;
    username: string;
    avatar_url?: string | null;
  }) => call<AccountInfo>("upsert_offline_account", { req }),

  authenticateExternal: (req: {
    api_base: string;
    username: string;
    password: string;
  }) => call<AccountInfo>("authenticate_external", { req }),

  setActiveAccount: (id: string) =>
    call<AccountInfo[]>("set_active_account", { id }),

  deleteAccount: (id: string) => call<AccountInfo[]>("delete_account", { id }),

  getGameDirInfo: () => call<GameDirInfo>("get_game_dir_info"),

  setGameDir: (path: string) => call<GameDirInfo>("set_game_dir", { path }),

  getLogDirInfo: () => call<LogDirInfo>("get_log_dir_info"),

  setLogDir: (path: string) => call<LogDirInfo>("set_log_dir", { path }),

  resetLogDir: () => call<LogDirInfo>("reset_log_dir"),

  openLogDir: () => call<void>("open_log_dir"),

  clearLogFiles: () => call<CleanupResult>("clear_log_files"),

  clearCache: () => call<CleanupResult>("clear_cache"),

  getUninstallInfo: () => call<UninstallInfo>("get_uninstall_info"),

  uninstallLauncher: () => call<void>("uninstall_launcher"),

  resetSettings: () => call<AppSettings>("reset_settings"),

  deleteGameData: (confirm: string) => call<void>("delete_game_data", { confirm }),

  openExternalUrl: (url: string) => call<void>("open_external_url", { url }),

  listMods: (versionId: string) =>
    call<ModInfo[]>("list_mods", { versionId }),

  toggleMod: (versionId: string, fileName: string, enabled: boolean) =>
    call<void>("toggle_mod", { versionId, fileName, enabled }),

  deleteMod: (versionId: string, fileName: string) =>
    call<void>("delete_mod", { versionId, fileName }),

  openInstanceDir: (versionId: string) =>
    call<void>("open_instance_dir", { versionId }),

  stopGame: () => call<void>("stop_game"),

  uninstallVersion: (versionId: string) =>
    call<void>("uninstall_version", { versionId }),

  clearGamePid: () => call<void>("clear_game_pid"),

  windowControl: (action: WindowAction) =>
    call<void>("window_control", { action }),

  // ---------- 插件系统 ----------
  listInstalledPlugins: () => call<PluginEntry[]>("list_installed_plugins"),

  getPluginManifest: (id: string) =>
    call<PluginManifest>("get_plugin_manifest", { id }),

  getDefaultPluginConfig: () => call<string[]>("get_default_plugin_config"),

  getPluginConfig: (id: string) => call<Record<string, unknown>>("get_plugin_config", { id }),

  setPluginConfig: (id: string, config: Record<string, unknown>) =>
    call<void>("set_plugin_config", { id, config }),

  exportPluginConfigs: () => call<unknown>("export_plugin_configs"),

  importPluginConfigs: (payload: unknown) =>
    call<number>("import_plugin_configs", { payload }),

  importPluginConfigsFromFile: (path: string) =>
    call<number>("import_plugin_configs_from_file", { path }),

  writePluginExportFile: (path: string, payload: unknown) =>
    call<void>("write_plugin_export_file", { path, payload }),

  validatePluginManifest: (manifest: Record<string, unknown>) =>
    call<string[]>("validate_plugin_manifest", { manifest }),

  checkPluginUpdate: (id: string) =>
    call<PluginRelease | null>("check_plugin_update", { id }),

  getPluginManifestFromDir: (dir: string) =>
    call<Record<string, unknown>>("read_plugin_manifest_from_dir", { dir }),

  setPluginEnabled: (id: string, enabled: boolean) =>
    call<PluginManifest>("set_plugin_enabled", { id, enabled }),

  installLocalPlugin: (sourceDir: string, targetDir?: string | null) =>
    call<PluginEntry>("install_local_plugin", { sourceDir, targetDir: targetDir ?? null }),

  installGithubPlugin: (repo: string, targetDir?: string | null) =>
    call<PluginEntry>("install_github_plugin", { repo, targetDir: targetDir ?? null }),

  uninstallPlugin: (id: string) => call<void>("uninstall_plugin", { id }),

  checkPluginRepoUpdates: (repo: string) =>
    call<PluginRelease | null>("check_plugin_repo_updates", { repo }),

  updatePluginRepoSafe: (repo: string, version: string) =>
    call<string>("update_plugin_repo_safe", { repo, version }),

  writeDefaultPluginConfig: (enabledIds: string[]) =>
    call<void>("write_default_plugin_config", { enabledIds }),

  installBuiltinPlugin: (manifest: Record<string, unknown>, targetDir?: string | null) =>
    call<PluginEntry>("install_builtin_plugin", { manifest, targetDir: targetDir ?? null }),

  // ---------- 更新与备份 ----------
  checkStartupUpdates: () => call<StartupUpdateCheck>("check_startup_updates"),

  checkLauncherUpdates: () => call<PluginRelease | null>("check_launcher_updates"),

  prepareLauncherUpdate: (version: string) =>
    call<string>("prepare_launcher_update", { version }),

  createLauncherBackup: (options: BackupOptions, note: string) =>
    call<BackupSummary>("create_launcher_backup", { options, note }),

  listLauncherBackups: () => call<BackupSummary[]>("list_launcher_backups"),

  restoreLauncherBackup: (path: string, options: BackupOptions) =>
    call<string>("restore_launcher_backup", { path, options }),

  deleteLauncherBackup: (path: string) =>
    call<void>("delete_launcher_backup", { path }),

  // ---------- 资源安装目标 ----------
  listResourceTargets: () => call<ResourceTarget[]>("list_resource_targets"),

  addResourceTargetDir: (input: ResourceTargetDir) =>
    call<ResourceTargetDir[]>("add_resource_target_dir", { input }),

  removeResourceTargetDir: (id: string) =>
    call<ResourceTargetDir[]>("remove_resource_target_dir", { id }),
};

// ---------- Events ----------

function listenSafe<T>(
  event: string,
  handler: (event: T) => void
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) {
    return Promise.resolve(() => {});
  }
  return listen<T>(event, (e) => handler(e.payload));
}

export function onInstallProgress(
  handler: (event: InstallProgressEvent) => void
): Promise<UnlistenFn> {
  return listenSafe<InstallProgressEvent>("install-progress", handler);
}

export function onLaunchStatus(
  handler: (event: LaunchStatusEvent) => void
): Promise<UnlistenFn> {
  return listenSafe<LaunchStatusEvent>("launch-status", handler);
}

/** 安装过程文字日志（各模块通用） */
export function onInstallLog(handler: (event: InstallLogEvent) => void) {
  return listenSafe<InstallLogEvent>("install-log", handler);
}

export function onGameOutput(
  handler: (event: GameOutputEvent) => void
): Promise<UnlistenFn> {
  return listenSafe<GameOutputEvent>("game-output", handler);
}
