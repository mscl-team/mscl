/**
 * 设置页：玩家名、内存分配、下载源、主题、后端信息
 * 所有设置保存到后端 settings.json（跨启动持久化）
 */

import { useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Icon } from "../components/Icon";
import { useConfirm } from "../components/ConfirmDialog";
import { UpdateDialog, type ReleaseUpdate } from "../components/UpdateDialog";
import { BackupPanel } from "../components/BackupPanel";
import { useT } from "../lib/i18n";
import {
  api,
  defaultCategoriesForKind,
  ISOLATION_RESOURCE_TYPES,
  RESOURCE_CATEGORIES,
  type AppSettings,
  type BackendStatus,
  type CleanupResult,
  type GameDirInfo,
  type IsolationEntry,
  type LogDirInfo,
  type PluginEntry,
  type ResourceTargetDir,
  type UninstallInfo,
} from "../lib/api";

interface SettingsPageProps {
  settings: AppSettings;
  onSaveSettings: (settings: AppSettings) => Promise<void>;
  theme: "dark" | "light";
  onThemeChange: (value: "dark" | "light") => void;
}

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / Math.pow(1024, i)).toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

function cleanupSummary(result: CleanupResult, t: (key: string, params?: Record<string, string | number>) => string): string {
  return t("settings.cleanupSummary", {
    n: result.deleted_files,
    size: formatBytes(result.freed_bytes),
  });
}

const RESOURCE_CATEGORY_LABELS: Record<string, string> = {
  mod: "resources.tab.mod",
  plugin: "resources.tab.plugin",
  skin: "resources.tab.skin",
  resourcepack: "resources.tab.resourcepack",
  shader: "resources.tab.shader",
  datapack: "resources.tab.datapack",
  modpack: "resources.tab.modpack",
};

const ISOLATION_META: Record<string, { label: string; desc: string }> = {
  saves: { label: "settings.isolationSaves", desc: "settings.isolationSavesDesc" },
  mods: { label: "settings.isolationMods", desc: "settings.isolationModsDesc" },
  resourcepacks: {
    label: "settings.isolationResourcepacks",
    desc: "settings.isolationResourcepacksDesc",
  },
  shaderpacks: {
    label: "settings.isolationShaderpacks",
    desc: "settings.isolationShaderpacksDesc",
  },
  config: { label: "settings.isolationConfig", desc: "settings.isolationConfigDesc" },
  datapacks: { label: "settings.isolationDatapacks", desc: "settings.isolationDatapacksDesc" },
};

type SettingsTab =
  | "game"
  | "verify"
  | "download"
  | "sources"
  | "dir"
  | "logs"
  | "advanced"
  | "plugins"
  | "appearance"
  | "backup"
  | "about";

const SETTINGS_TABS: { key: SettingsTab; labelKey: string }[] = [
  { key: "game", labelKey: "settings.game" },
  { key: "verify", labelKey: "settings.verify" },
  { key: "download", labelKey: "settings.download" },
  { key: "sources", labelKey: "settings.sources" },
  { key: "dir", labelKey: "settings.gameDir" },
  { key: "logs", labelKey: "settings.logCache" },
  { key: "advanced", labelKey: "settings.advanced" },
  { key: "plugins", labelKey: "settings.plugins" },
  { key: "appearance", labelKey: "settings.appearance" },
  { key: "backup", labelKey: "settings.backup" },
  { key: "about", labelKey: "settings.about" },
];

interface ExportFieldMeta {
  key: string;
  labelKey: string;
}

const EXPORT_FIELDS: ExportFieldMeta[] = [
  { key: "username", labelKey: "settings.exportUsername" },
  { key: "max_memory_mb", labelKey: "settings.exportMemory" },
  { key: "java_path", labelKey: "settings.exportJava" },
  { key: "download_source", labelKey: "settings.exportDownloadSource" },
  { key: "download_concurrency", labelKey: "settings.exportConcurrency" },
  { key: "version_isolation", labelKey: "settings.exportIsolation" },
  { key: "isolation_resources", labelKey: "settings.exportIsolationResources" },
  { key: "resource_sources", labelKey: "settings.exportSources" },
  { key: "check_dependencies", labelKey: "settings.exportCheckDeps" },
  { key: "verify_resources_on_launch", labelKey: "settings.exportVerifyRes" },
  { key: "verify_game_on_launch", labelKey: "settings.exportVerifyGame" },
  { key: "launcher_language", labelKey: "settings.exportLanguage" },
  { key: "extra_jvm_args", labelKey: "settings.exportJvmArgs" },
  { key: "extra_game_args", labelKey: "settings.exportGameArgs" },
  { key: "hide_install_logs", labelKey: "settings.exportHideInstallLogs" },
  { key: "hide_update_logs", labelKey: "settings.exportHideUpdateLogs" },
  { key: "hide_launch_logs", labelKey: "settings.exportHideLaunchLogs" },
  { key: "plugin_repo_url", labelKey: "settings.exportPluginRepo" },
  { key: "plugin_repo_branch", labelKey: "settings.exportPluginBranch" },
  { key: "plugin_auto_update", labelKey: "settings.exportPluginAutoUpdate" },
  { key: "plugin_check_updates", labelKey: "settings.exportPluginCheck" },
  { key: "launcher_auto_update", labelKey: "settings.exportLauncherAutoUpdate" },
  { key: "resource_targets", labelKey: "settings.exportResourceTargets" },
];

export function SettingsPage({
  settings,
  onSaveSettings,
  theme,
  onThemeChange,
}: SettingsPageProps) {
  const t = useT();
  const dialogs = useConfirm();
  const [info, setInfo] = useState<BackendStatus | null>(null);
  const [draft, setDraft] = useState<AppSettings>(settings);
  const [saving, setSaving] = useState(false);
  const [saveMessage, setSaveMessage] = useState("");
  const [checking, setChecking] = useState(false);
  const [updateMessage, setUpdateMessage] = useState("");
  const [checkingPluginUpdates, setCheckingPluginUpdates] = useState(false);
  const [pluginUpdateMessage, setPluginUpdateMessage] = useState("");
  const [releaseUpdate, setReleaseUpdate] = useState<ReleaseUpdate | null>(null);
  const [updatingRelease, setUpdatingRelease] = useState(false);
  const [updateProgress, setUpdateProgress] = useState("");
  const [newSource, setNewSource] = useState({
    name: "",
    kind: "curseforge",
    api_base: "",
    api_key: "",
    categories: defaultCategoriesForKind("curseforge"),
  });
  const [gameDir, setGameDir] = useState<GameDirInfo | null>(null);
  const [dirInput, setDirInput] = useState("");
  const [dirMessage, setDirMessage] = useState("");
  const [logDir, setLogDir] = useState<LogDirInfo | null>(null);
  const [uninstallInfo, setUninstallInfo] = useState<UninstallInfo | null>(null);
  const [maintenanceMessage, setMaintenanceMessage] = useState("");
  const [activeTab, setActiveTab] = useState<SettingsTab>("game");
  const [installedPlugins, setInstalledPlugins] = useState<PluginEntry[]>([]);
  const [pluginListMessage, setPluginListMessage] = useState("");
  const [newTarget, setNewTarget] = useState<ResourceTargetDir>({ id: "", name: "", path: "", kind: "local" });
  const [targetMessage, setTargetMessage] = useState("");

  const refreshPlugins = () => {
    api.listInstalledPlugins()
      .then(setInstalledPlugins)
      .catch((e) => setPluginListMessage(String(e)));
  };

  const refreshGameDir = () => {
    api.getGameDirInfo().then(setGameDir).catch(() => setGameDir(null));
  };

  const refreshLogDir = () => {
    api.getLogDirInfo().then(setLogDir).catch(() => setLogDir(null));
  };

  useEffect(() => {
    refreshGameDir();
    refreshLogDir();
    api.getUninstallInfo().then(setUninstallInfo).catch(() => setUninstallInfo(null));
    refreshPlugins();
  }, []);

  const handlePickLogDir = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: t("settings.logDirChange"),
      });
      if (typeof selected !== "string" || !selected.trim()) return;
      await api.setLogDir(selected.trim());
      refreshLogDir();
      setMaintenanceMessage(t("settings.logDirSaved"));
    } catch (e) {
      setMaintenanceMessage(t("settings.logDirFailed", { error: String(e) }));
    }
  };

  const handleResetLogDir = async () => {
    try {
      await api.resetLogDir();
      refreshLogDir();
      setMaintenanceMessage(t("settings.logDirResetDone"));
    } catch (e) {
      setMaintenanceMessage(t("settings.logDirFailed", { error: String(e) }));
    }
  };

  const handleOpenLogDir = async () => {
    try {
      await api.openLogDir();
    } catch (e) {
      setMaintenanceMessage(t("settings.logDirFailed", { error: String(e) }));
    }
  };

  const handleClearCache = async () => {
    const ok = await dialogs.confirm(t("settings.clearCacheWarn"), {
      title: t("settings.clearCache"),
      confirmText: t("common.delete"),
      danger: true,
    });
    if (!ok) return;
    try {
      const result = await api.clearCache();
      setMaintenanceMessage(cleanupSummary(result, t));
    } catch (e) {
      setMaintenanceMessage(t("settings.cacheClearFailed", { error: String(e) }));
    }
  };

  const handleClearLogs = async () => {
    const ok = await dialogs.confirm(t("settings.clearLogsWarn"), {
      title: t("settings.clearLogs"),
      confirmText: t("common.delete"),
      danger: true,
    });
    if (!ok) return;
    try {
      const result = await api.clearLogFiles();
      refreshLogDir();
      setMaintenanceMessage(cleanupSummary(result, t));
    } catch (e) {
      setMaintenanceMessage(t("settings.logsClearFailed", { error: String(e) }));
    }
  };

  const patchSource = (
    id: string,
    patch: Partial<AppSettings["resource_sources"][number]>
  ) => {
    update(
      "resource_sources",
      draft.resource_sources.map((source) => {
        if (source.id !== id) return source;
        const next = { ...source, ...patch };
        if (patch.kind && !patch.categories) {
          next.categories = defaultCategoriesForKind(patch.kind);
        }
        if (!next.categories) {
          next.categories = defaultCategoriesForKind(next.kind);
        }
        return next;
      })
    );
  };

  const toggleSourceCategory = (id: string, category: string) => {
    const source = draft.resource_sources.find((item) => item.id === id);
    if (!source) return;
    const current = source.categories?.length
      ? source.categories
      : defaultCategoriesForKind(source.kind);
    patchSource(id, {
      categories: current.includes(category)
        ? current.filter((item) => item !== category)
        : [...current, category],
    });
  };

  const toggleNewSourceCategory = (category: string) => {
    setNewSource((prev) => ({
      ...prev,
      categories: prev.categories.includes(category)
        ? prev.categories.filter((item) => item !== category)
        : [...prev.categories, category],
    }));
  };

  const [exportKeys, setExportKeys] = useState<string[]>(() =>
    EXPORT_FIELDS.map((field) => field.key)
  );
  const [backupMessage, setBackupMessage] = useState("");

  const toggleExportKey = (key: string) => {
    setExportKeys((prev) =>
      prev.includes(key) ? prev.filter((item) => item !== key) : [...prev, key]
    );
  };

  const handleExportSettings = async () => {
    setBackupMessage("");
    try {
      const payload = await api.exportSettings(exportKeys);
      const selected = await save({
        filters: [{ name: "MSCL Settings", extensions: ["json"] }],
        defaultPath: `mscl-settings-${new Date().toISOString().slice(0, 10)}.json`,
        title: t("settings.exportTitle"),
      });
      if (typeof selected !== "string") return;
      await api.saveSettingsFile(selected, payload);
      setBackupMessage(t("settings.exported"));
    } catch (e) {
      setBackupMessage(String(e));
    }
  };

  const handleImportSettings = async () => {
    setBackupMessage("");
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "MSCL Settings", extensions: ["json"] }],
        title: t("settings.importTitle"),
      });
      if (typeof selected !== "string") return;
      const text = await api.readSettingsFile(selected);
      const payload = JSON.parse(text) as Record<string, unknown>;
      const next = await api.importSettings(payload);
      setDraft(next);
      setBackupMessage(t("settings.imported"));
    } catch (e) {
      setBackupMessage(String(e));
    }
  };

  const removeSource = (id: string) => {
    if (draft.resource_sources.length <= 1) return;
    update(
      "resource_sources",
      draft.resource_sources.filter((s) => s.id !== id)
    );
  };

  const addSource = () => {
    if (!newSource.name.trim()) {
      setDirMessage(t("settings.sourceNeedName"));
      return;
    }
    update("resource_sources", [
      ...draft.resource_sources,
      {
        id: `src-${Date.now()}`,
        ...newSource,
        categories:
          newSource.categories.length > 0
            ? newSource.categories
            : defaultCategoriesForKind(newSource.kind),
      },
    ]);
    setNewSource({
      name: "",
      kind: "curseforge",
      api_base: "",
      api_key: "",
      categories: defaultCategoriesForKind("curseforge"),
    });
  };

  const handleSetDir = async () => {
    if (!dirInput.trim()) {
      setDirMessage(t("settings.dirNeedPath"));
      return;
    }
    try {
      const info = await api.setGameDir(dirInput.trim());
      setGameDir(info);
      setDirInput("");
      setDirMessage(t("settings.dirSaved"));
    } catch (e) {
      setDirMessage(t("common.failed", { error: String(e) }));
    }
  };

  const handleDeleteData = async () => {
    const first = await dialogs.confirm(t("settings.dirDeleteWarn"), {
      title: t("settings.dirDelete"),
      confirmText: t("common.delete"),
      danger: true,
    });
    if (!first) return;
    const answer = await dialogs.prompt(t("settings.dirDeletePrompt"), {
      title: t("settings.dirDelete"),
      expected: "DELETE",
      placeholder: "DELETE",
      confirmText: t("common.delete"),
      danger: true,
    });
    if (answer !== "DELETE") {
      setDirMessage(t("settings.dirDeleteCancel"));
      return;
    }
    try {
      await api.deleteGameData("DELETE");
      setDirMessage(t("settings.dirDeleted"));
      refreshGameDir();
    } catch (e) {
      setDirMessage(t("settings.dirDeleteFailed", { error: String(e) }));
    }
  };

  const handleReset = async () => {
    const ok = await dialogs.confirm(t("settings.resetWarn"), {
      title: t("settings.resetConfirmTitle"),
      confirmText: t("common.confirm"),
      danger: true,
    });
    if (!ok) return;
    try {
      const defaults = await api.resetSettings();
      setDraft(defaults);
      await onSaveSettings(defaults);
      setSaveMessage(t("settings.resetDone"));
      refreshGameDir();
    } catch (e) {
      setSaveMessage(t("settings.resetFailed", { error: String(e) }));
    }
  };

  const handleCheckUpdate = async () => {
    setChecking(true);
    setUpdateMessage("");
    try {
      // 与启动检查共用同一套 GitHub Release 更新逻辑（不再用 Tauri 内置 updater）。
      const release = await api.checkLauncherUpdates();
      if (!release || !release.tag) {
        setUpdateMessage(t("settings.updateCurrent"));
        return;
      }
      setReleaseUpdate({
        version: release.tag,
        body: release.body || "",
        publishedAt: release.published_at,
        htmlUrl: release.html_url,
        kind: "launcher",
      });
    } catch (e) {
      setUpdateMessage(t("settings.updateFailed", { error: String(e) }));
    } finally {
      setChecking(false);
    }
  };

  const handleUninstallPlugin = async (plugin: PluginEntry) => {
    const ok = await dialogs.confirm(t("plugins.uninstallWarn", { name: plugin.name }), {
      title: t("plugins.uninstall"),
      confirmText: t("common.delete"),
      danger: true,
    });
    if (!ok) return;
    try {
      await api.uninstallPlugin(plugin.id);
      setPluginListMessage(t("plugins.uninstalled", { name: plugin.name }));
      refreshPlugins();
    } catch (e) {
      setPluginListMessage(String(e));
    }
  };

  const handleTogglePlugin = async (plugin: PluginEntry) => {
    try {
      await api.setPluginEnabled(plugin.id, !plugin.enabled);
      refreshPlugins();
    } catch (e) {
      setPluginListMessage(String(e));
    }
  };

  const handleAddTarget = async () => {
    if (!newTarget.name.trim() || !newTarget.path.trim()) {
      setTargetMessage(t("resources.target.choose"));
      return;
    }
    try {
      const list = await api.addResourceTargetDir(newTarget);
      setDraft((prev) => ({ ...prev, resource_targets: list }));
      setNewTarget({ id: "", name: "", path: "", kind: "local" });
      setTargetMessage(t("resources.target.added"));
    } catch (e) {
      setTargetMessage(String(e));
    }
  };

  const handleRemoveTarget = async (id: string) => {
    try {
      const list = await api.removeResourceTargetDir(id);
      setDraft((prev) => ({ ...prev, resource_targets: list }));
    } catch (e) {
      setTargetMessage(String(e));
    }
  };

  const handleCheckPluginUpdates = async () => {
    setCheckingPluginUpdates(true);
    setPluginUpdateMessage("");
    try {
      const repo = draft.plugin_repo_url.trim();
      if (!repo) {
        setPluginUpdateMessage(t("settings.pluginNeedRepo"));
        return;
      }
      const release = await api.checkPluginRepoUpdates(repo);
      if (!release || !release.tag) {
        setPluginUpdateMessage(t("settings.pluginNoUpdate"));
        return;
      }
      setReleaseUpdate({
        version: release.tag,
        body: release.body || "",
        publishedAt: release.published_at,
        htmlUrl: release.html_url,
        kind: "plugin",
      });
    } catch (e) {
      // 仓库未部署时给出明确提示
      setPluginUpdateMessage(String(e));
    } finally {
      setCheckingPluginUpdates(false);
    }
  };

  const handleReleaseUpdate = async () => {
    if (!releaseUpdate) return;
    setUpdatingRelease(true);
    const isLauncher = releaseUpdate.kind === "launcher";
    setUpdateProgress(
      isLauncher ? t("settings.launcherUpdating") : t("settings.pluginUpdatingBackup")
    );
    try {
      const result = isLauncher
        ? await api.prepareLauncherUpdate(releaseUpdate.version)
        : await api.updatePluginRepoSafe(
            draft.plugin_repo_url.trim(),
            releaseUpdate.version
          );
      setUpdateProgress("");
      setReleaseUpdate(null);
      if (isLauncher) {
        setUpdateMessage(result);
      } else {
        setPluginUpdateMessage(result);
      }
      // 安全关闭启动器，由独立更新助手完成替换；中断会自动回滚
      window.setTimeout(() => {
        api.windowControl("close").catch(() => window.close());
      }, 1200);
    } catch (e) {
      // 仓库不可达等错误，后端未改动任何文件
      setUpdateProgress("");
      if (isLauncher) {
        setUpdateMessage(String(e));
      } else {
        setPluginUpdateMessage(String(e));
      }
    } finally {
      setUpdatingRelease(false);
    }
  };

  const handleUninstallLauncher = async () => {
    const ok = await dialogs.confirm(t("settings.uninstallWarn"), {
      title: t("settings.uninstallTitle"),
      confirmText: t("settings.uninstallConfirmBtn"),
      danger: true,
    });
    if (!ok) return;
    try {
      await api.uninstallLauncher();
    } catch (e) {
      setMaintenanceMessage(t("settings.uninstallFailed", { error: String(e) }));
    }
  };

  useEffect(() => {
    api.backendStatus().then(setInfo).catch(() => setInfo(null));
  }, []);

  const update = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) =>
    setDraft((prev) => ({ ...prev, [key]: value }));

  const toggleIsolationResource = (kind: string) => {
    const selected = draft.isolation_resources.includes(kind);
    if (selected && draft.isolation_resources.length <= 1) {
      setSaveMessage(t("settings.isolationNeedOne"));
      return;
    }
    update(
      "isolation_resources",
      selected
        ? draft.isolation_resources.filter((value) => value !== kind)
        : [...draft.isolation_resources, kind]
    );
  };

  const [instanceIsoStates, setInstanceIsoStates] = useState<Record<string, IsolationEntry[]>>(
    {}
  );

  useEffect(() => {
    api.listInstances().then(async (instances) => {
      if (!draft.version_isolation || instances.length === 0) {
        setInstanceIsoStates({});
        return;
      }
      const entries: Record<string, IsolationEntry[]> = {};
      for (const instance of instances.slice(0, 8)) {
        try {
          entries[instance.version_id] = await api.getInstanceIsolation(instance.version_id);
        } catch {
          // 单个实例读取失败忽略
        }
      }
      setInstanceIsoStates(entries);
    });
  }, [draft.version_isolation, draft.isolation_resources.length, activeTab]);

  const changeTab = (tab: SettingsTab) => {
    setActiveTab(tab);
    const main = document.querySelector<HTMLElement>(".main-area");
    if (main) main.scrollTo({ top: 0, behavior: "smooth" });
  };

  const isCustomSource =
    draft.download_source !== "official" && draft.download_source !== "bmclapi";
  const sourceValue = isCustomSource ? "custom" : draft.download_source;

  const handleSourceChange = (value: string) => {
    if (value === "custom") {
      update("download_source", isCustomSource ? draft.download_source : "");
    } else {
      update("download_source", value);
    }
  };

  const canSave = Boolean(draft.download_source.trim());

  const handleSave = async () => {
    setSaving(true);
    setSaveMessage("");
    try {
      // 用户名由用户中心管理，设置页保存时不覆盖当前账户
      await onSaveSettings({ ...draft, username: settings.username });
      setSaveMessage(t("settings.saved"));
    } catch (e) {
      setSaveMessage(t("settings.saveFailed", { error: String(e) }));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="page settings-page">
      <UpdateDialog
        update={releaseUpdate}
        updating={updatingRelease}
        progress={updateProgress}
        onUpdate={handleReleaseUpdate}
        onLater={() => setReleaseUpdate(null)}
      />
      <header className="page-header">
        <h1>{t("settings.pageTitle")}</h1>
        <p>{t("settings.pageSubtitle")}</p>
      </header>

      <div className="settings-tabs" role="tablist" aria-label={t("settings.pageTitle")}>
        {SETTINGS_TABS.map((tab) => (
          <button
            key={tab.key}
            role="tab"
            aria-selected={activeTab === tab.key}
            className={`settings-tab ${activeTab === tab.key ? "settings-tab-active" : ""}`}
            onClick={() => changeTab(tab.key)}
          >
            {t(tab.labelKey)}
          </button>
        ))}
      </div>

      <div className="settings-content">
        <section className="settings-section" hidden={activeTab !== "game"}>
          <h2 className="section-title">{t("settings.game")}</h2>
          <div className="settings-row">
            <div className="settings-row-info">
              <div className="settings-label">{t("settings.memory")}</div>
              <div className="settings-desc text-tertiary">{t("settings.memoryDesc")}</div>
            </div>
            <select
              className="settings-input"
              value={draft.max_memory_mb}
              onChange={(e) => update("max_memory_mb", Number(e.target.value))}
            >
              <option value={0}>{t("settings.memoryAuto")}</option>
              <option value={1024}>1 GB</option>
              <option value={2048}>2 GB</option>
              <option value={4096}>4 GB</option>
              <option value={8192}>8 GB</option>
              <option value={16384}>16 GB</option>
            </select>
          </div>
          <div className="settings-row">
            <div className="settings-row-info">
              <div className="settings-label">{t("settings.isolation")}</div>
              <div className="settings-desc text-tertiary">{t("settings.isolationDesc")}</div>
            </div>
            <label className="switch">
              <input
                type="checkbox"
                checked={draft.version_isolation}
                onChange={(e) => update("version_isolation", e.target.checked)}
              />
              <span className="switch-slider" />
            </label>
          </div>

          {draft.version_isolation && (
            <div className="settings-subblock">
              <div className="settings-subblock-head">
                <div className="settings-label">{t("settings.isolationTypes")}</div>
                <div className="settings-desc text-tertiary">
                  {t("settings.isolationTypesDesc")}
                </div>
              </div>
              <div className="isolation-grid">
                {ISOLATION_RESOURCE_TYPES.map((kind) => {
                  const meta = ISOLATION_META[kind];
                  const checked = draft.isolation_resources.includes(kind);
                  return (
                    <label
                      key={kind}
                      className={`isolation-card ${checked ? "isolation-card-on" : ""}`}
                    >
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={() => toggleIsolationResource(kind)}
                      />
                      <span className="isolation-card-text">
                        <span className="isolation-card-label">{t(meta.label)}</span>
                        <span className="isolation-card-desc">{t(meta.desc)}</span>
                      </span>
                    </label>
                  );
                })}
              </div>
              <p className="settings-hint">{t("settings.isolationTypesHint")}</p>

              {Object.keys(instanceIsoStates).length > 0 && (
                <div className="isolation-status">
                  <div className="isolation-status-title">{t("settings.isolationStatus")}</div>
                  {Object.entries(instanceIsoStates).map(([versionId, entries]) => (
                    <div className="isolation-status-row" key={versionId}>
                      <span className="isolation-status-name">{versionId}</span>
                      <span className="isolation-status-chips">
                        {entries.map((entry) => (
                          <span
                            key={entry.kind}
                            className={`iso-chip ${
                              entry.state === "isolated"
                                ? "iso-chip-isolated"
                                : entry.state === "shared_with_files"
                                  ? "iso-chip-warn"
                                  : "iso-chip-shared"
                            }`}
                            title={
                              entry.state === "isolated"
                                ? t("settings.isoStateIsolated")
                                : entry.state === "shared_with_files"
                                  ? t("settings.isoStateFiles")
                                  : entry.state === "shared"
                                    ? t("settings.isoStateShared")
                                    : t("settings.isoStateUnknown")
                            }
                          >
                            {entry.kind}
                          </span>
                        ))}
                      </span>
                    </div>
                  ))}
                  <p className="settings-hint">{t("settings.isolationStatusHint")}</p>
                </div>
              )}
            </div>
          )}
        </section>

        <section className="settings-section" hidden={activeTab !== "verify"}>
          <h2 className="section-title">{t("settings.verify")}</h2>

          <div className="settings-row">
            <div className="settings-row-info">
              <div className="settings-label">{t("settings.checkDeps")}</div>
              <div className="settings-desc text-tertiary">{t("settings.checkDepsDesc")}</div>
            </div>
            <label className="switch">
              <input
                type="checkbox"
                checked={draft.check_dependencies}
                onChange={(e) => update("check_dependencies", e.target.checked)}
              />
              <span className="switch-slider" />
            </label>
          </div>

          <div className="settings-row">
            <div className="settings-row-info">
              <div className="settings-label">{t("settings.verifyGame")}</div>
              <div className="settings-desc text-tertiary">{t("settings.verifyGameDesc")}</div>
            </div>
            <label className="switch">
              <input
                type="checkbox"
                checked={draft.verify_game_on_launch}
                onChange={(e) => update("verify_game_on_launch", e.target.checked)}
              />
              <span className="switch-slider" />
            </label>
          </div>

          <div className="settings-row">
            <div className="settings-row-info">
              <div className="settings-label">{t("settings.verifyRes")}</div>
              <div className="settings-desc text-tertiary">{t("settings.verifyResDesc")}</div>
            </div>
            <label className="switch">
              <input
                type="checkbox"
                checked={draft.verify_resources_on_launch}
                onChange={(e) => update("verify_resources_on_launch", e.target.checked)}
              />
              <span className="switch-slider" />
            </label>
          </div>
        </section>

        <section className="settings-section" hidden={activeTab !== "sources"}>
          <h2 className="section-title">{t("settings.sources")}</h2>
          <p className="settings-hint" style={{ marginBottom: 10 }}>
            {t("settings.sourcesHint")}
          </p>

          <div className="sources-list">
          {draft.resource_sources.map((source) => {
            const categories = source.categories?.length
              ? source.categories
              : defaultCategoriesForKind(source.kind);
            return (
              <div className="source-card" key={source.id}>
                <div className="source-card-grid">
                  <div className="source-field">
                    <label>{t("settings.sourceName")}</label>
                    <input
                      className="settings-input"
                      value={source.name}
                      onChange={(e) => patchSource(source.id, { name: e.target.value })}
                      placeholder={t("settings.sourceName")}
                    />
                  </div>
                  <div className="source-field">
                    <label>{t("settings.sourceKind")}</label>
                    <select
                      className="settings-input"
                      value={source.kind}
                      onChange={(e) => patchSource(source.id, { kind: e.target.value })}
                    >
                      <option value="modrinth">Modrinth</option>
                      <option value="curseforge">CurseForge</option>
                      <option value="minebbs">MineBBS</option>
                      <option value="mcmod">MC百科</option>
                      <option value="spigot">Spigot</option>
                      <option value="custom">{t("settings.sourceCustomKind")}</option>
                    </select>
                  </div>
                  <div className="source-field">
                    <label>{t("settings.sourceApiLabel")}</label>
                    <input
                      className="settings-input"
                      value={source.api_base}
                      onChange={(e) => patchSource(source.id, { api_base: e.target.value })}
                      placeholder={t("settings.sourceApi")}
                    />
                  </div>
                  <div className="source-field">
                    <label>{t("settings.sourceKeyLabel")}</label>
                    <input
                      className="settings-input"
                      value={source.api_key}
                      onChange={(e) => patchSource(source.id, { api_key: e.target.value })}
                      placeholder={t("settings.sourceKeyShort")}
                    />
                  </div>
                  <button
                    className="btn-icon source-remove"
                    onClick={() => removeSource(source.id)}
                    disabled={draft.resource_sources.length <= 1}
                    title={t("settings.sourceRemove")}
                  >
                    <Icon name="trash" size={14} />
                  </button>
                </div>
                <div className="source-categories">
                  <span className="source-categories-label">{t("settings.sourceCategories")}</span>
                  <div className="source-category-list">
                    {RESOURCE_CATEGORIES.map((category) => (
                      <label
                        key={category}
                        className={`source-category-chip ${
                          categories.includes(category) ? "source-category-active" : ""
                        }`}
                      >
                        <input
                          type="checkbox"
                          checked={categories.includes(category)}
                          onChange={() => toggleSourceCategory(source.id, category)}
                        />
                        {t(RESOURCE_CATEGORY_LABELS[category])}
                      </label>
                    ))}
                  </div>
                </div>
              </div>
            );
          })}

          <div className="source-card source-card-add">
            <div className="source-card-grid">
              <div className="source-field">
                <label>{t("settings.sourceName")}</label>
                <input
                  className="settings-input"
                  value={newSource.name}
                  onChange={(e) => setNewSource({ ...newSource, name: e.target.value })}
                  placeholder={t("settings.sourceNewName")}
                />
              </div>
              <div className="source-field">
                <label>{t("settings.sourceKind")}</label>
                <select
                  className="settings-input"
                  value={newSource.kind}
                  onChange={(e) =>
                    setNewSource({
                      ...newSource,
                      kind: e.target.value,
                      categories: defaultCategoriesForKind(e.target.value),
                    })
                  }
                >
                  <option value="modrinth">Modrinth</option>
                  <option value="curseforge">CurseForge</option>
                  <option value="minebbs">MineBBS</option>
                  <option value="mcmod">MC百科</option>
                  <option value="spigot">Spigot</option>
                  <option value="custom">{t("settings.sourceCustomKind")}</option>
                </select>
              </div>
              <div className="source-field">
                <label>{t("settings.sourceApiLabel")}</label>
                <input
                  className="settings-input"
                  value={newSource.api_base}
                  onChange={(e) => setNewSource({ ...newSource, api_base: e.target.value })}
                  placeholder={t("settings.sourceApiOptional")}
                />
              </div>
              <div className="source-field">
                <label>{t("settings.sourceKeyLabel")}</label>
                <input
                  className="settings-input"
                  value={newSource.api_key}
                  onChange={(e) => setNewSource({ ...newSource, api_key: e.target.value })}
                  placeholder={t("settings.sourceKeyShort")}
                />
              </div>
              <div className="source-add-placeholder" />
            </div>
            <div className="source-categories">
              <span className="source-categories-label">{t("settings.sourceCategories")}</span>
              <div className="source-category-list">
                {RESOURCE_CATEGORIES.map((category) => (
                  <label
                    key={category}
                    className={`source-category-chip ${
                      newSource.categories.includes(category) ? "source-category-active" : ""
                    }`}
                  >
                    <input
                      type="checkbox"
                      checked={newSource.categories.includes(category)}
                      onChange={() => toggleNewSourceCategory(category)}
                    />
                    {t(RESOURCE_CATEGORY_LABELS[category])}
                  </label>
                ))}
              </div>
            </div>
            <div className="source-add-actions">
              <button className="btn btn-primary btn-inline" onClick={addSource}>
                <Icon name="download" size={13} />
                {t("settings.sourceAdd")}
              </button>
            </div>
          </div>

          <h3 className="section-title" style={{ marginTop: 20 }}>{t("resources.target.add")}</h3>
          <div className="source-card">
            <div className="source-card-grid">
              <div className="source-field">
                <label>{t("resources.target.name")}</label>
                <input
                  className="settings-input"
                  value={newTarget.name}
                  onChange={(e) => setNewTarget({ ...newTarget, name: e.target.value })}
                />
              </div>
              <div className="source-field">
                <label>{t("resources.target.kind")}</label>
                <select
                  className="settings-input"
                  value={newTarget.kind}
                  onChange={(e) => setNewTarget({ ...newTarget, kind: e.target.value as ResourceTargetDir["kind"] })}
                >
                  <option value="local">{t("resources.target.local")}</option>
                  <option value="server">{t("resources.target.server")}</option>
                  <option value="multiplayer">{t("resources.target.multiplayer")}</option>
                </select>
              </div>
              <div className="source-field">
                <label>{t("resources.target.path")}</label>
                <input
                  className="settings-input"
                  value={newTarget.path}
                  onChange={(e) => setNewTarget({ ...newTarget, path: e.target.value })}
                  placeholder="D:\\MC\\servers\\survival"
                />
              </div>
              <div className="source-add-actions">
                <button className="btn btn-ghost btn-inline" onClick={handleAddTarget}>
                  {t("resources.target.add")}
                </button>
              </div>
            </div>
            {(draft.resource_targets ?? []).map((dir) => (
              <div className="source-field" key={dir.id} style={{ marginTop: 8 }}>
                <code>{t(`resources.target.${dir.kind}`)} · {dir.name} · {dir.path}</code>
                <button
                  className="btn-icon"
                  onClick={() => handleRemoveTarget(dir.id)}
                  title={t("resources.target.remove")}
                >
                  <Icon name="trash" size={13} />
                </button>
              </div>
            ))}
          </div>
          {targetMessage && <div className="message-bar">{targetMessage}</div>}
          </div>
        </section>

        <section className="settings-section" hidden={activeTab !== "dir"}>
          <h2 className="section-title">{t("settings.gameDir")}</h2>
          <div className="settings-item">
            <div className="dir-row">
              <code className="dir-path">{gameDir?.path ?? t("common.loading")}</code>
              <button className="btn-icon" onClick={refreshGameDir} title={t("common.refresh")}>
                <Icon name="refresh" size={14} />
              </button>
            </div>
            <p className="settings-hint">
              {t("settings.dirStats", {
                n: gameDir?.installed_versions ?? 0,
                type: gameDir?.is_custom ? t("settings.dirCustom") : t("settings.dirDefault"),
              })}
            </p>

            <div className="dir-actions">
              <input
                className="settings-input"
                value={dirInput}
                onChange={(e) => setDirInput(e.target.value)}
                placeholder={t("settings.dirNewPlaceholder")}
              />
              <button className="btn btn-ghost btn-inline" onClick={handleSetDir}>
                {t("settings.dirChange")}
              </button>
            </div>
            <p className="settings-hint">{t("settings.dirImportHint")}</p>

            <button className="btn btn-danger btn-inline" onClick={handleDeleteData}>
              <Icon name="trash" size={14} />
              {t("settings.dirDelete")}
            </button>

            {dirMessage && <p className="settings-hint">{dirMessage}</p>}
          </div>
        </section>

        <section className="settings-section" hidden={activeTab !== "logs"}>
          <h2 className="section-title">{t("settings.logCache")}</h2>

          <div className="settings-subblock">
            <div className="settings-subblock-head">
              <div className="settings-label">{t("settings.logDisplay")}</div>
              <div className="settings-desc text-tertiary">{t("settings.logDisplayDesc")}</div>
            </div>
            <div className="settings-row">
              <div className="settings-row-info">
                <div className="settings-label">{t("settings.hideInstallLogs")}</div>
                <div className="settings-desc text-tertiary">
                  {t("settings.hideInstallLogsDesc")}
                </div>
              </div>
              <label className="switch">
                <input
                  type="checkbox"
                  checked={draft.hide_install_logs}
                  onChange={(e) => update("hide_install_logs", e.target.checked)}
                />
                <span className="switch-slider" />
              </label>
            </div>
            <div className="settings-row">
              <div className="settings-row-info">
                <div className="settings-label">{t("settings.hideUpdateLogs")}</div>
                <div className="settings-desc text-tertiary">
                  {t("settings.hideUpdateLogsDesc")}
                </div>
              </div>
              <label className="switch">
                <input
                  type="checkbox"
                  checked={draft.hide_update_logs}
                  onChange={(e) => update("hide_update_logs", e.target.checked)}
                />
                <span className="switch-slider" />
              </label>
            </div>
            <div className="settings-row">
              <div className="settings-row-info">
                <div className="settings-label">{t("settings.hideLaunchLogs")}</div>
                <div className="settings-desc text-tertiary">
                  {t("settings.hideLaunchLogsDesc")}
                </div>
              </div>
              <label className="switch">
                <input
                  type="checkbox"
                  checked={draft.hide_launch_logs}
                  onChange={(e) => update("hide_launch_logs", e.target.checked)}
                />
                <span className="switch-slider" />
              </label>
            </div>
          </div>
          <div className="settings-item">
            <div className="settings-label">{t("settings.logDir")}</div>
            <div className="dir-row">
              <code className="dir-path">{logDir?.path ?? t("common.loading")}</code>
              <button className="btn-icon" onClick={refreshLogDir} title={t("common.refresh")}>
                <Icon name="refresh" size={14} />
              </button>
            </div>
            <p className="settings-hint">
              {t("settings.logDirStats", {
                n: logDir?.file_count ?? 0,
                size: formatBytes(logDir?.total_bytes ?? 0),
                type: logDir?.is_custom ? t("settings.dirCustom") : t("settings.dirDefault"),
              })}
            </p>
            <div className="dir-actions">
              <button className="btn btn-ghost btn-inline" onClick={handlePickLogDir}>
                <Icon name="folder" size={14} />
                {t("settings.logDirChange")}
              </button>
              <button className="btn btn-ghost btn-inline" onClick={handleOpenLogDir}>
                {t("settings.logDirOpen")}
              </button>
              <button
                className="btn btn-ghost btn-inline"
                onClick={handleResetLogDir}
                disabled={!logDir?.is_custom}
              >
                {t("settings.logDirReset")}
              </button>
            </div>
            <p className="settings-hint">{t("settings.logDirHint")}</p>
          </div>

          <div className="settings-row">
            <div className="settings-row-info">
              <div className="settings-label">{t("settings.clearLogs")}</div>
              <div className="settings-desc text-tertiary">{t("settings.clearLogsDesc")}</div>
            </div>
            <button className="btn btn-danger btn-inline" onClick={handleClearLogs}>
              <Icon name="trash" size={14} />
              {t("settings.clearLogs")}
            </button>
          </div>

          <div className="settings-row">
            <div className="settings-row-info">
              <div className="settings-label">{t("settings.clearCache")}</div>
              <div className="settings-desc text-tertiary">{t("settings.clearCacheDesc")}</div>
            </div>
            <button className="btn btn-danger btn-inline" onClick={handleClearCache}>
              <Icon name="trash" size={14} />
              {t("settings.clearCache")}
            </button>
          </div>

          {maintenanceMessage && <p className="settings-hint">{maintenanceMessage}</p>}
        </section>

        <section className="settings-section" hidden={activeTab !== "download"}>
          <h2 className="section-title">{t("settings.download")}</h2>
          <div className="settings-row">
            <div className="settings-row-info">
              <div className="settings-label">{t("settings.downloadSource")}</div>
              <div className="settings-desc text-tertiary">{t("settings.downloadSourceDesc")}</div>
            </div>
            <select
              className="settings-input"
              value={sourceValue}
              onChange={(e) => handleSourceChange(e.target.value)}
            >
              <option value="official">{t("settings.sourceOfficial")}</option>
              <option value="bmclapi">{t("settings.sourceBmclapi")}</option>
              <option value="custom">{t("settings.sourceCustom")}</option>
            </select>
          </div>
          {isCustomSource && (
            <div className="settings-row">
              <div className="settings-row-info">
                <div className="settings-label">{t("settings.mirrorRoot")}</div>
                <div className="settings-desc text-tertiary">{t("settings.mirrorRootDesc")}</div>
              </div>
              <input
                className="settings-input"
                value={draft.download_source}
                onChange={(e) => update("download_source", e.target.value)}
                placeholder="https://..."
              />
            </div>
          )}
          <div className="settings-row">
            <div className="settings-row-info">
              <div className="settings-label">{t("settings.concurrency")}</div>
              <div className="settings-desc text-tertiary">{t("settings.concurrencyDesc")}</div>
            </div>
            <select
              className="settings-input"
              value={draft.download_concurrency}
              onChange={(e) => update("download_concurrency", Number(e.target.value))}
            >
              {[4, 8, 16, 32].map((n) => (
                <option key={n} value={n}>
                  {t("settings.threads", { n })}
                </option>
              ))}
            </select>
          </div>
        </section>

        <section className="settings-section" hidden={activeTab !== "advanced"}>
          <h2 className="section-title">{t("settings.advanced")}</h2>
          <div className="settings-item">
            <label className="settings-label">{t("settings.language")}</label>
            <select
              className="settings-input"
              value={draft.launcher_language}
              onChange={(e) => update("launcher_language", e.target.value)}
            >
              <option value="zh-CN">{t("settings.langZh")}</option>
              <option value="en-US">{t("settings.langEn")}</option>
            </select>
            <p className="settings-hint">{t("settings.languageHint")}</p>
          </div>
          <div className="settings-item">
            <label className="settings-label">{t("settings.jvmArgs")}</label>
            <input
              className="settings-input"
              value={draft.extra_jvm_args}
              onChange={(e) => update("extra_jvm_args", e.target.value)}
              placeholder="-XX:+UseG1GC -XX:+ParallelRefProcEnabled"
            />
            <p className="settings-hint">{t("settings.jvmArgsHint")}</p>
          </div>
          <div className="settings-item">
            <label className="settings-label">{t("settings.gameArgs")}</label>
            <input
              className="settings-input"
              value={draft.extra_game_args}
              onChange={(e) => update("extra_game_args", e.target.value)}
              placeholder="--fullscreen"
            />
            <p className="settings-hint">{t("settings.gameArgsHint")}</p>
          </div>
        </section>

        <section className="settings-section" hidden={activeTab !== "plugins"}>
          <h2 className="section-title">{t("settings.plugins")}</h2>
          <p className="settings-hint">{t("settings.pluginsHint")}</p>

          <div className="settings-item">
            <label className="settings-label">{t("settings.pluginRepoUrl")}</label>
            <input
              className="settings-input"
              value={draft.plugin_repo_url}
              onChange={(e) => update("plugin_repo_url", e.target.value)}
              placeholder="https://github.com/owner/repo"
            />
            <p className="settings-hint">{t("settings.pluginRepoUrlHint")}</p>
          </div>

          <div className="settings-item">
            <label className="settings-label">{t("settings.pluginRepoBranch")}</label>
            <input
              className="settings-input"
              value={draft.plugin_repo_branch}
              onChange={(e) => update("plugin_repo_branch", e.target.value)}
              placeholder="main"
            />
          </div>

          <div className="settings-item">
            <label className="settings-label">{t("settings.pluginRepoToken")}</label>
            <input
              className="settings-input"
              type="password"
              value={draft.plugin_repo_token}
              onChange={(e) => update("plugin_repo_token", e.target.value)}
              placeholder={t("settings.pluginRepoTokenHint")}
            />
            <p className="settings-hint">{t("settings.pluginRepoTokenDesc")}</p>
          </div>

          <div className="settings-row">
            <div className="settings-row-info">
              <div className="settings-label">{t("settings.launcherAutoUpdate")}</div>
              <div className="settings-desc text-tertiary">{t("settings.launcherAutoUpdateDesc")}</div>
            </div>
            <input
              type="checkbox"
              checked={draft.launcher_auto_update}
              onChange={(e) => update("launcher_auto_update", e.target.checked)}
            />
          </div>

          <div className="settings-row">
            <div className="settings-row-info">
              <div className="settings-label">{t("settings.pluginCheckUpdates")}</div>
              <div className="settings-desc text-tertiary">{t("settings.pluginCheckUpdatesDesc")}</div>
            </div>
            <input
              type="checkbox"
              checked={draft.plugin_check_updates}
              onChange={(e) => update("plugin_check_updates", e.target.checked)}
            />
          </div>

          <div className="settings-row">
            <div className="settings-row-info">
              <div className="settings-label">{t("settings.pluginAutoUpdate")}</div>
              <div className="settings-desc text-tertiary">{t("settings.pluginAutoUpdateDesc")}</div>
            </div>
            <input
              type="checkbox"
              checked={draft.plugin_auto_update}
              onChange={(e) => update("plugin_auto_update", e.target.checked)}
            />
          </div>

          <div className="settings-row">
            <div className="settings-row-info">
              <div className="settings-label">{t("settings.pluginCheckNow")}</div>
              <div className="settings-desc text-tertiary">{t("settings.pluginCheckNowDesc")}</div>
            </div>
            <button
              className="btn btn-ghost btn-inline"
              onClick={handleCheckPluginUpdates}
              disabled={checkingPluginUpdates}
            >
              <Icon name="refresh" size={14} />
              {checkingPluginUpdates ? t("settings.checking") : t("settings.pluginCheckBtn")}
            </button>
          </div>
          {pluginUpdateMessage && <div className="update-hint">{pluginUpdateMessage}</div>}

          <h3 className="section-title" style={{ marginTop: 20 }}>{t("plugins.externalSection")}</h3>
          {installedPlugins.length === 0 ? (
            <p className="settings-hint">{t("plugins.noInstallable")}</p>
          ) : (
            <div className="backup-list">
              {installedPlugins.map((plugin) => (
                <div className="backup-item" key={plugin.id}>
                  <div className="backup-item-main">
                    <div className="backup-item-title">
                      {plugin.name}
                      <span className="plugin-item-ver">v{plugin.version}</span>
                      {!plugin.enabled && <span className="plugin-item-tag disabled">{t("plugins.disabled")}</span>}
                    </div>
                    <div className="backup-item-meta text-tertiary">{plugin.id} · {plugin.layer ?? "feature"}</div>
                  </div>
                  <div className="backup-item-actions">
                    <button
                      className="btn btn-ghost btn-inline"
                      onClick={() => handleTogglePlugin(plugin)}
                      disabled={plugin.required}
                    >
                      {plugin.enabled ? t("plugins.disable") : t("plugins.enable")}
                    </button>
                    <button
                      className="btn btn-danger btn-inline"
                      onClick={() => handleUninstallPlugin(plugin)}
                      disabled={plugin.required || plugin.builtin}
                    >
                      {t("plugins.uninstall")}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
          {pluginListMessage && <div className="message-bar">{pluginListMessage}</div>}
        </section>

        <section className="settings-section" hidden={activeTab !== "backup"}>
          <h2 className="section-title">{t("backup.title")}</h2>
          <BackupPanel />

          <h2 className="section-title" style={{ marginTop: 24 }}>{t("settings.backup")}</h2>
          <p className="settings-hint">{t("settings.backupHint")}</p>

          <div className="backup-field-list">
            {EXPORT_FIELDS.map((field) => (
              <label key={field.key} className="check-chip">
                <input
                  type="checkbox"
                  checked={exportKeys.includes(field.key)}
                  onChange={() => toggleExportKey(field.key)}
                />
                {t(field.labelKey)}
              </label>
            ))}
          </div>

          <div className="backup-actions">
            <button
              className="btn btn-primary btn-inline"
              onClick={handleExportSettings}
              disabled={exportKeys.length === 0}
            >
              {t("settings.exportBtn")}
            </button>
            <button className="btn btn-ghost btn-inline" onClick={handleImportSettings}>
              {t("settings.importBtn")}
            </button>
            <button
              className="btn btn-ghost btn-inline"
              onClick={() => setExportKeys(EXPORT_FIELDS.map((field) => field.key))}
            >
              {t("settings.selectAll")}
            </button>
            <button className="btn btn-ghost btn-inline" onClick={() => setExportKeys([])}>
              {t("settings.clearSelection")}
            </button>
          </div>
          <p className="settings-hint">{t("settings.backupMergeHint")}</p>

          {backupMessage && <div className="message-bar">{backupMessage}</div>}
        </section>

        <section className="settings-section" hidden={activeTab !== "appearance"}>
          <h2 className="section-title">{t("settings.appearance")}</h2>
          <div className="settings-row">
            <div className="settings-row-info">
              <div className="settings-label">{t("settings.themeMode")}</div>
              <div className="settings-desc text-tertiary">{t("settings.themeDesc")}</div>
            </div>
            <div className="theme-switch">
              <button className={theme === "dark" ? "active" : ""} onClick={() => onThemeChange("dark")}>
                {t("settings.themeDark")}
              </button>
              <button className={theme === "light" ? "active" : ""} onClick={() => onThemeChange("light")}>
                {t("settings.themeLight")}
              </button>
            </div>
          </div>
        </section>

        <section className="settings-section" hidden={activeTab !== "about"}>
          <h2 className="section-title">{t("settings.about")}</h2>
          <div className="info-card">
            <div className="settings-row">
              <span className="text-secondary">{t("settings.versionLabel")}</span>
              <div className="update-row">
                <span>MSCL 0.1.0</span>
                <button
                  className="btn btn-ghost btn-inline"
                  onClick={handleCheckUpdate}
                  disabled={checking}
                >
                  <Icon name="refresh" size={14} />
                  {checking ? t("settings.checking") : t("settings.checkUpdate")}
                </button>
              </div>
            </div>
            {updateMessage && <div className="update-hint">{updateMessage}</div>}
            <div className="settings-row">
              <span className="text-secondary">{t("settings.dataDir")}</span>
              <span className="text-tertiary" style={{ fontSize: 12, maxWidth: 260, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {info?.game_root ?? "—"}
              </span>
            </div>
          </div>

          <div className="info-card danger-card">
            <div className="settings-row">
              <div>
                <div className="settings-label">{t("settings.uninstallTitle")}</div>
                <div className="settings-desc text-tertiary">
                  {!uninstallInfo
                    ? t("settings.uninstallDesc")
                    : !uninstallInfo.supported
                      ? t("settings.uninstallUnsupported")
                      : !uninstallInfo.uninstaller
                        ? t("settings.uninstallNotFound")
                        : t("settings.uninstallDesc")}
                </div>
                {uninstallInfo?.install_dir && (
                  <div
                    className="settings-desc text-tertiary"
                    title={uninstallInfo.install_dir}
                    style={{ maxWidth: 380, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", marginTop: 4 }}
                  >
                    {uninstallInfo.install_dir}
                  </div>
                )}
              </div>
              <button
                className="btn btn-danger btn-inline"
                onClick={handleUninstallLauncher}
                disabled={!uninstallInfo?.uninstaller}
              >
                <Icon name="trash" size={14} />
                {t("settings.uninstallTitle")}
              </button>
            </div>
          </div>
        </section>

        <div className="settings-actions">
          <button className="btn btn-ghost" onClick={handleReset}>
            {t("settings.resetAll")}
          </button>
          <button className="btn btn-primary" onClick={handleSave} disabled={saving || !canSave}>
            <Icon name="check" size={16} />
            {saving ? t("settings.saving") : t("btn.save")}
          </button>
          {saveMessage && <span className="settings-save-hint">{saveMessage}</span>}
        </div>
      </div>
    </div>
  );
}
