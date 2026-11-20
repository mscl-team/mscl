/**
 * MSCL 插件语法与工程生态（Harness Plugin DSL v1）
 *
 * 主程序 exe 是宿主（Harness），只负责：进程生命周期、插件扫描/加载、能力总线、
 * 事件总线与配置持久化。所有功能以插件为单位，通过 plugin.json manifest 挂载。
 *
 * manifest 语法：schema/id/name/version/category/layer/description/author/entry/
 *   enabled/builtin/required/source/repo/capabilities/hooks/extensionPoints/
 *   dependencies/configSchema/icon/order
 * 生命周期：onLoad → onEnable → run → onDisable；游戏：onBeforeLaunch/onLaunch/onAfterLaunch；
 *   资源：onBeforeResourceInstall/onResourceInstalled；更新：onUpdateCheck；设置：onSettingsSave。
 * 扩展点：page / settings / resource.category / launch.step / download.source / theme / command。
 * 配置：每个插件按 configSchema 声明可编辑字段，宿主统一读写/导入/导出/校验。
 */

export type PluginCategory = "core" | "launcher" | "server";
export type PluginLayer = "harness" | "module" | "feature";
export type PluginSource = "builtin" | "local" | "github";

export type PluginCapability =
  | "page" | "settings" | "resourceCategory" | "theme" | "command"
  | "launch" | "download" | "storage" | "event" | "i18n" | "config";

export type PluginHook =
  | "onLoad" | "onEnable" | "onDisable"
  | "onBeforeLaunch" | "onLaunch" | "onAfterLaunch"
  | "onBeforeResourceInstall" | "onResourceInstalled"
  | "onUpdateCheck" | "onSettingsSave";

export type ExtensionPoint =
  | "page" | "settings" | "resource.category" | "launch.step"
  | "download.source" | "theme" | "command";

export interface ExtensionBinding {
  point: ExtensionPoint;
  key: string;
  label?: string;
}

export type ConfigFieldType = "string" | "number" | "boolean" | "select" | "string[]";

export interface ConfigFieldSchema {
  key: string;
  label: string;
  type: ConfigFieldType;
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
  category: PluginCategory;
  layer: PluginLayer;
  description: string;
  author: string;
  entry: string;
  enabled: boolean;
  builtin: boolean;
  required: boolean;
  source: PluginSource;
  repo?: string;
  capabilities: PluginCapability[];
  hooks: PluginHook[];
  extensionPoints: ExtensionBinding[];
  dependencies: string[];
  configSchema: ConfigFieldSchema[];
  icon?: string;
  order?: number;
  updateAvailable?: boolean;
}

type PlugBase = Pick<PluginManifest, "id" | "name" | "description"> & Partial<PluginManifest>;

function plug(base: PlugBase): PluginManifest {
  return {
    version: "1.0.0",
    category: "core",
    layer: "feature",
    author: "MSCL Team",
    entry: base.id,
    enabled: true,
    builtin: false,
    required: false,
    source: "builtin",
    capabilities: [],
    hooks: ["onLoad"],
    extensionPoints: [],
    dependencies: [],
    configSchema: [],
    order: 100,
    ...base,
  };
}

/**
 * 内置插件清单：按单一职责拆细。
 * 宿主内核 6 / 页面模块 7 / 资源子模块 8 / 启动与下载 8 / 功能插件 9。
 */
export const BUILTIN_PLUGINS: PluginManifest[] = [
  // ── 宿主内核 ──
  plug({ id: "core-harness", name: "插件宿主内核", description: "进程生命周期、插件扫描加载、能力注册与调度总控。", category: "core", layer: "harness", entry: "app", required: true, capabilities: ["command", "config"], hooks: ["onLoad", "onEnable", "onDisable"], extensionPoints: [{ point: "command", key: "harness" }], order: 0, icon: "◈" }),
  plug({ id: "core-ipc", name: "IPC 通信总线", description: "前端与 Tauri 后端类型安全通信通道。", layer: "harness", required: true, capabilities: ["command"], dependencies: ["core-harness"], order: 1, icon: "⇄" }),
  plug({ id: "core-eventbus", name: "事件总线", description: "安装进度、启动日志等跨插件事件发布订阅。", layer: "harness", required: true, capabilities: ["event"], dependencies: ["core-harness"], order: 2, icon: "⌁" }),
  plug({ id: "core-state", name: "全局状态管理", description: "账号、实例、下载队列等共享状态。", layer: "harness", required: true, capabilities: ["storage", "event"], dependencies: ["core-ipc"], order: 3, icon: "▣" }),
  plug({ id: "core-config", name: "配置中心", description: "settings.json 与插件 config.json 的统一读写、导入导出。", layer: "harness", required: true, capabilities: ["config", "storage"], hooks: ["onLoad", "onSettingsSave"], dependencies: ["core-ipc"], order: 4, icon: "⚙" }),
  plug({ id: "core-i18n", name: "国际化模块", description: "中英文词典与占位符插值，供所有插件复用。", layer: "harness", required: true, capabilities: ["i18n"], dependencies: ["core-harness"], order: 5, icon: "文" }),

  // ── 页面模块 ──
  plug({ id: "page-home", name: "首页", description: "欢迎区、快捷启动与控制台入口。", required: true, capabilities: ["page"], extensionPoints: [{ point: "page", key: "home" }], dependencies: ["core-state"], order: 10, icon: "⌂" }),
  plug({ id: "page-users", name: "账号页", description: "离线与正版账号管理、切换。", required: true, capabilities: ["page"], extensionPoints: [{ point: "page", key: "users" }], dependencies: ["core-state"], order: 11, icon: "☻" }),
  plug({ id: "page-versions", name: "版本页", description: "版本列表与实例管理入口。", required: true, capabilities: ["page"], extensionPoints: [{ point: "page", key: "versions" }], dependencies: ["core-state"], order: 12, icon: "▤" }),
  plug({ id: "page-resources", name: "资源页", description: "资源中心容器与分类管理。", required: true, capabilities: ["page", "resourceCategory"], extensionPoints: [{ point: "page", key: "resources" }], dependencies: ["core-state"], order: 13, icon: "◇" }),
  plug({ id: "page-features", name: "功能页", description: "功能介绍与引导。", capabilities: ["page"], extensionPoints: [{ point: "page", key: "features" }], order: 14, icon: "✧" }),
  plug({ id: "page-settings", name: "设置页", description: "设置分区容器。", required: true, capabilities: ["page", "settings"], extensionPoints: [{ point: "page", key: "settings" }], dependencies: ["core-config"], order: 15, icon: "⚙" }),
  plug({ id: "page-skin", name: "皮肤页", description: "皮肤站与 3D 预览入口。", capabilities: ["page"], extensionPoints: [{ point: "page", key: "skin" }], dependencies: ["plugin-skin"], order: 16, icon: "♟" }),

  // ── 资源子模块 ──
  plug({ id: "res-mod", name: "Mod 资源", description: "Fabric/Forge Mod 检索与安装。", capabilities: ["resourceCategory"], extensionPoints: [{ point: "resource.category", key: "mod" }], dependencies: ["page-resources"], order: 20, icon: "▦" }),
  plug({ id: "res-shader", name: "光影资源", description: "光影包检索与安装。", capabilities: ["resourceCategory"], extensionPoints: [{ point: "resource.category", key: "shader" }], dependencies: ["page-resources"], order: 21, icon: "✺" }),
  plug({ id: "res-resourcepack", name: "材质包资源", description: "材质包检索与安装。", capabilities: ["resourceCategory"], extensionPoints: [{ point: "resource.category", key: "resourcepack" }], dependencies: ["page-resources"], order: 22, icon: "▧" }),
  plug({ id: "res-datapack", name: "数据包资源", description: "数据包检索与安装。", capabilities: ["resourceCategory"], extensionPoints: [{ point: "resource.category", key: "datapack" }], dependencies: ["page-resources"], order: 23, icon: "▩" }),
  plug({ id: "res-modpack", name: "整合包资源", description: "整合包导入与安装。", capabilities: ["resourceCategory"], extensionPoints: [{ point: "resource.category", key: "modpack" }], dependencies: ["page-resources"], hooks: ["onBeforeResourceInstall", "onResourceInstalled"], order: 24, icon: "▥" }),
  plug({ id: "res-server-plugin", name: "服务端插件资源", description: "Spigot/Paper 服务端插件检索与安装。", category: "server", capabilities: ["resourceCategory"], extensionPoints: [{ point: "resource.category", key: "plugin" }], dependencies: ["page-resources"], order: 25, icon: "⬡" }),
  plug({ id: "res-launcher-plugin", name: "启动器插件资源", description: "启动器插件市场（本地/仓库安装与管理）。", category: "launcher", capabilities: ["resourceCategory", "command"], extensionPoints: [{ point: "resource.category", key: "launcherPlugin" }], dependencies: ["plugin-plugin-hub"], order: 26, icon: "⊞" }),
  plug({ id: "res-skin", name: "皮肤资源", description: "皮肤与披风检索。", category: "server", capabilities: ["resourceCategory"], extensionPoints: [{ point: "resource.category", key: "skin" }], dependencies: ["page-resources", "plugin-skin"], order: 27, icon: "♟" }),

  // ── 启动与下载 ──
  plug({ id: "launch-core", name: "启动编排器", description: "按钩子顺序编排启动流程。", required: true, capabilities: ["launch", "command"], hooks: ["onBeforeLaunch", "onLaunch", "onAfterLaunch"], extensionPoints: [{ point: "launch.step", key: "core" }], dependencies: ["core-state"], order: 30, icon: "▶", configSchema: [{ key: "closeOnLaunch", label: "启动后关闭启动器", type: "boolean", default: false }, { key: "timeoutSec", label: "启动超时（秒）", type: "number", default: 120, min: 30, max: 600 }] }),
  plug({ id: "launch-java", name: "Java 检测", description: "自动检测系统 Java 与版本匹配。", capabilities: ["launch"], hooks: ["onBeforeLaunch"], extensionPoints: [{ point: "launch.step", key: "java" }], dependencies: ["launch-core"], order: 31, icon: "☕", configSchema: [{ key: "autoDetect", label: "自动检测 Java", type: "boolean", default: true }, { key: "minMajor", label: "最低 Java 主版本", type: "number", default: 17, min: 8, max: 30 }] }),
  plug({ id: "launch-memory", name: "内存分配", description: "自动/手动堆内存分配。", capabilities: ["launch", "settings"], hooks: ["onBeforeLaunch"], extensionPoints: [{ point: "launch.step", key: "memory" }, { point: "settings", key: "game" }], dependencies: ["launch-core"], order: 32, icon: "▤", configSchema: [{ key: "autoMemory", label: "自动分配", type: "boolean", default: true }, { key: "maxMemoryMb", label: "最大内存（MB）", type: "number", default: 2048, min: 512, max: 65536 }] }),
  plug({ id: "launch-args", name: "JVM/游戏参数", description: "额外 JVM 与游戏参数注入。", capabilities: ["launch", "settings"], hooks: ["onBeforeLaunch"], extensionPoints: [{ point: "launch.step", key: "args" }, { point: "settings", key: "advanced" }], dependencies: ["launch-core"], order: 33, icon: "⌘", configSchema: [{ key: "jvmArgs", label: "额外 JVM 参数", type: "string", default: "" }, { key: "gameArgs", label: "额外游戏参数", type: "string", default: "" }] }),
  plug({ id: "launch-verify", name: "启动前校验", description: "游戏文件与资源完整性校验。", capabilities: ["launch", "settings"], hooks: ["onBeforeLaunch"], extensionPoints: [{ point: "launch.step", key: "verify" }, { point: "settings", key: "verify" }], dependencies: ["launch-core"], order: 34, icon: "✓", configSchema: [{ key: "verifyGame", label: "校验游戏文件", type: "boolean", default: true }, { key: "verifyResources", label: "校验资源", type: "boolean", default: false }] }),
  plug({ id: "launch-isolation", name: "版本隔离", description: "按版本隔离存档/Mod/配置。", capabilities: ["launch", "settings"], hooks: ["onBeforeLaunch", "onSettingsSave"], extensionPoints: [{ point: "settings", key: "isolation" }], dependencies: ["launch-core"], order: 35, icon: "▦", configSchema: [{ key: "enabled", label: "启用版本隔离", type: "boolean", default: false }, { key: "types", label: "隔离资源类型", type: "string[]", default: ["saves", "mods", "resourcepacks"] }] }),
  plug({ id: "download-engine", name: "下载引擎", description: "并发下载、断点续传与镜像回退。", required: true, capabilities: ["download"], hooks: ["onLoad"], extensionPoints: [{ point: "download.source", key: "engine" }], dependencies: ["core-ipc"], order: 40, icon: "⬇", configSchema: [{ key: "concurrency", label: "并发数", type: "number", default: 8, min: 1, max: 32 }, { key: "retry", label: "失败重试次数", type: "number", default: 3, min: 0, max: 10 }] }),
  plug({ id: "download-mirror", name: "镜像源管理", description: "官方/BMCLAPI/自定义镜像源切换。", capabilities: ["download", "settings"], hooks: ["onLoad"], extensionPoints: [{ point: "download.source", key: "mirror" }, { point: "settings", key: "download" }], dependencies: ["download-engine"], order: 41, icon: "⇆", configSchema: [{ key: "source", label: "下载源", type: "select", default: "official", options: [{ value: "official", label: "官方" }, { value: "bmclapi", label: "BMCLAPI" }, { value: "custom", label: "自定义" }] }, { key: "customRoot", label: "自定义镜像根地址", type: "string", default: "" }] }),

  // ── 功能插件 ──
  plug({ id: "plugin-skin", name: "皮肤与披风", description: "皮肤站接入、3D 预览与披风。", category: "core", capabilities: ["page", "resourceCategory"], dependencies: ["core-state"], order: 50, icon: "♟", configSchema: [{ key: "show3d", label: "启用 3D 预览", type: "boolean", default: true }] }),
  plug({ id: "plugin-plugin-hub", name: "插件市场", description: "启动器插件安装/更新/导入/导出/禁用与 config 编辑。", category: "launcher", capabilities: ["page", "resourceCategory", "settings", "command"], hooks: ["onLoad", "onEnable", "onDisable"], dependencies: ["core-config"], order: 51, icon: "⊞", configSchema: [{ key: "autoInstallDefaults", label: "安装时勾选全部默认插件", type: "boolean", default: true }, { key: "checkSignature", label: "安装前校验 manifest", type: "boolean", default: true }] }),
  plug({ id: "plugin-auto-update", name: "启动器更新检测", description: "检测程序与默认插件库更新，展示日志并安全更新回滚。", category: "launcher", capabilities: ["settings", "command"], hooks: ["onLoad", "onUpdateCheck"], dependencies: ["core-config"], order: 52, icon: "↻", configSchema: [{ key: "autoCheck", label: "启动时自动检查", type: "boolean", default: true }, { key: "autoUpdate", label: "自动更新程序", type: "boolean", default: false }, { key: "updatePlugins", label: "同时更新默认插件库", type: "boolean", default: true }] }),
  plug({ id: "plugin-logger", name: "日志中心", description: "分类日志（安装/更新/启动）与清理。", category: "core", capabilities: ["event", "settings"], extensionPoints: [{ point: "settings", key: "logs" }], dependencies: ["core-eventbus"], order: 53, icon: "≡", configSchema: [{ key: "retainDays", label: "日志保留天数", type: "number", default: 30, min: 1, max: 365 }] }),
  plug({ id: "plugin-window", name: "窗口管理", description: "自定义标题栏、最小化/最大化/关闭。", category: "core", capabilities: ["command"], dependencies: ["core-ipc"], order: 54, icon: "▢" }),
  plug({ id: "plugin-theme", name: "主题系统", description: "深色/浅色主题切换与 CSS 变量注入。", category: "core", capabilities: ["theme", "settings"], hooks: ["onLoad", "onSettingsSave"], extensionPoints: [{ point: "theme", key: "theme" }, { point: "settings", key: "appearance" }], dependencies: ["core-state"], order: 55, icon: "◐", configSchema: [{ key: "mode", label: "主题模式", type: "select", default: "dark", options: [{ value: "dark", label: "深色" }, { value: "light", label: "浅色" }] }] }),
  plug({ id: "plugin-backup", name: "备份与还原", description: "按内容勾选备份启动器设置、插件、实例元数据与账号；还原前自动保护现有数据。", category: "core", capabilities: ["config", "settings", "storage"], hooks: ["onLoad", "onSettingsSave"], extensionPoints: [{ point: "settings", key: "backup" }], dependencies: ["core-config"], order: 56, icon: "⇩", configSchema: [{ key: "keepCount", label: "最多保留备份数", type: "number", default: 10, min: 1, max: 100 }, { key: "includeLogs", label: "默认包含日志", type: "boolean", default: false }] }),
  plug({ id: "plugin-security", name: "安全中心", description: "插件 manifest 校验、路径穿越防护、来源可信标记。", category: "core", layer: "harness", required: true, capabilities: ["command", "config"], dependencies: ["core-harness"], order: 57, icon: "⛨", configSchema: [{ key: "strictManifest", label: "严格校验 manifest", type: "boolean", default: true }, { key: "allowUnsigned", label: "允许未签名插件", type: "boolean", default: true }] }),
];

export const PLUGIN_CATEGORY_LABELS: Record<PluginCategory, string> = {
  core: "核心模块", launcher: "启动器插件", server: "服务端插件",
};
export const PLUGIN_CATEGORY_LABELS_EN: Record<PluginCategory, string> = {
  core: "Core Modules", launcher: "Launcher Plugins", server: "Server Plugins",
};
export const PLUGIN_LAYER_LABELS: Record<PluginLayer, string> = {
  harness: "宿主内核", module: "功能模块", feature: "功能插件",
};

/** 安装器可勾选的默认插件（非必需项，默认全选） */
export const OPTIONAL_DEFAULT_PLUGINS = BUILTIN_PLUGINS.filter(
  (p) => !p.required && p.layer !== "harness"
).map((p) => ({ id: p.id, name: p.name, icon: p.icon ?? "⊞", category: p.category }));

export const DEFAULT_PLUGIN_REPO = "https://github.com/mscl-team/mscl-plugins";
export const PLUGIN_DSL_VERSION = 1;
