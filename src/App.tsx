/**
 * MSCL 主应用：布局骨架 + 页面路由（本地状态，无路由库）
 * 底部全局状态坞：统一下载进度条与安装日志（阶段 13）
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { TitleBar } from "./components/TitleBar";
import { Sidebar } from "./components/Sidebar";
import { Icon } from "./components/Icon";
import { ConfirmProvider } from "./components/ConfirmDialog";
import { UpdateDialog, type ReleaseUpdate } from "./components/UpdateDialog";
import { HomePage } from "./pages/HomePage";
import { VersionsPage } from "./pages/VersionsPage";
import { ResourcesPage } from "./pages/ResourcesPage";
import { FeaturesPage } from "./pages/FeaturesPage";
import { SettingsPage } from "./pages/SettingsPage";
import { UsersPage } from "./pages/UsersPage";
import { useTheme } from "./hooks/useTheme";
import { LangContext, translate, type Lang } from "./lib/i18n";
import {
  api,
  defaultCategoriesForKind,
  ISOLATION_RESOURCE_TYPES,
  onInstallLog,
  onInstallProgress,
  type AppSettings,
  type InstallLogEvent,
  type InstallProgressEvent,
  type ResourceSource,
} from "./lib/api";

export type Page =
  | "home"
  | "users"
  | "versions"
  | "resources"
  | "features"
  | "settings";

type DockLog = InstallLogEvent & { id: number };

/** 多步骤任务（创建实例）的阶段进度 */
type StepProgress = {
  label: string;
  step: number;
  steps: number;
  completed: number;
  total: number;
};

/** 安装控制状态（后端推送） */
type InstallControlState = "idle" | "running" | "paused" | "cancelled";

const SPIGOT_SOURCE: ResourceSource = {
  id: "spigot",
  name: "Spigot（Spiget）",
  kind: "spigot",
  api_base: "https://api.spiget.org/v2",
  api_key: "",
  categories: ["plugin"],
};

const MCMOD_SOURCE: ResourceSource = {
  id: "mcmod",
  name: "MC百科（网页检索）",
  kind: "mcmod",
  api_base: "https://www.mcmod.cn",
  api_key: "",
  categories: ["mod", "resourcepack", "datapack", "modpack"],
};

function withDefaultSources(list: ResourceSource[]): ResourceSource[] {
  const normalized = list.map((source) => ({
    ...source,
    categories:
      source.categories && source.categories.length > 0
        ? source.categories
        : defaultCategoriesForKind(source.kind),
  }));
  return [...normalized, SPIGOT_SOURCE, MCMOD_SOURCE].filter(
    (source, index, all) => all.findIndex((item) => item.id === source.id) === index
  );
}

const DEFAULT_SETTINGS: AppSettings = {
  username: "Player",
  max_memory_mb: 0, // 0 = 自动
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
    SPIGOT_SOURCE,
    MCMOD_SOURCE,
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

export default function App() {
  const [page, setPage] = useState<Page>("home");
  const [accountRevision, setAccountRevision] = useState(0);
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const { theme, toggleTheme, setTheme } = useTheme();

  // 全局下载进度与安装日志
  const [progress, setProgress] = useState<InstallProgressEvent | null>(null);
  const [stepProgress, setStepProgress] = useState<StepProgress | null>(null);
  const [logs, setLogs] = useState<DockLog[]>([]);
  const [installState, setInstallState] = useState<InstallControlState>("idle");
  const [busyAction, setBusyAction] = useState<"" | "pause" | "resume" | "cancel">("");
  const stickyPercent = useRef(0);
  const logSeq = useRef(0);
  const clearProgressTimer = useRef<number | null>(null);
  const [dockOpen, setDockOpen] = useState(false);
  const startupChecked = useRef(false);
  const [update, setUpdate] = useState<ReleaseUpdate | null>(null);
  const [updating, setUpdating] = useState(false);
  const [updateProgress, setUpdateProgress] = useState("");
  const [autoCountdown, setAutoCountdown] = useState<number | null>(null);

  // 启动时从后端读取持久化设置（首次启动返回默认值）
  useEffect(() => {
    api.getSettings()
      .then((s) =>
        setSettings({
          username: s.username?.trim() || "Player",
          max_memory_mb: Number.isFinite(s.max_memory_mb) ? s.max_memory_mb : 0,
          java_path: s.java_path ?? null,
          download_source: s.download_source || "official",
          download_concurrency: Number.isFinite(s.download_concurrency)
            ? s.download_concurrency
            : 8,
          version_isolation: Boolean(s.version_isolation),
          isolation_resources:
            Array.isArray(s.isolation_resources) && s.isolation_resources.length > 0
              ? s.isolation_resources
              : [...ISOLATION_RESOURCE_TYPES],
          modrinth_api: s.modrinth_api || "https://api.modrinth.com/v2",
          resource_sources:
            s.resource_sources && s.resource_sources.length > 0
              ? withDefaultSources(s.resource_sources)
              : DEFAULT_SETTINGS.resource_sources,
          check_dependencies: s.check_dependencies !== false,
          verify_resources_on_launch: Boolean(s.verify_resources_on_launch),
          verify_game_on_launch: s.verify_game_on_launch !== false,
          launcher_language: s.launcher_language || "zh-CN",
          extra_jvm_args: s.extra_jvm_args || "",
          extra_game_args: s.extra_game_args || "",
          hide_install_logs: Boolean(s.hide_install_logs),
          hide_update_logs: Boolean(s.hide_update_logs),
          hide_launch_logs: Boolean(s.hide_launch_logs),
          plugin_repo_url: s.plugin_repo_url || "https://github.com/mscl-team/mscl-plugins",
          plugin_repo_branch: s.plugin_repo_branch || "main",
          plugin_repo_token: s.plugin_repo_token || "",
          plugin_auto_update: s.plugin_auto_update !== false,
          plugin_check_updates: s.plugin_check_updates !== false,
          launcher_auto_update: s.launcher_auto_update !== false,
          resource_targets: Array.isArray(s.resource_targets) ? s.resource_targets : [],
        })
      )
      .catch(() => {
        // 读取失败时也允许正常使用，用默认值
      })
      .finally(() => setSettingsLoaded(true));
  }, []);

  // 启动时按自动更新设置检查 release；检查失败不打断启动
  useEffect(() => {
    if (!settingsLoaded || startupChecked.current) return;
    startupChecked.current = true;
    api.checkStartupUpdates()
      .then((result) => {
        const launcher = result.launcher;
        const plugins = result.plugins;
        if (!launcher && !plugins) return;
        const chosen = launcher ?? plugins!;
        const autoEnabled = launcher
          ? result.launcher_auto_update
          : result.plugins_auto_update;
        if (!autoEnabled) return;
        setUpdate({
          version: chosen.tag,
          body: chosen.body,
          publishedAt: chosen.published_at,
          htmlUrl: chosen.html_url,
          kind: launcher ? "launcher" : "plugin",
        });
        setAutoCountdown(8);
      })
      .catch(() => {});
  }, [settingsLoaded]);

  const applyStartupUpdate = useCallback(async () => {
    if (!update || updating) return;
    setUpdating(true);
    setAutoCountdown(null);
    try {
      const message =
        update.kind === "launcher"
          ? await api.prepareLauncherUpdate(update.version)
          : await api.updatePluginRepoSafe(settings.plugin_repo_url, update.version);
      setUpdateProgress(`${message}，即将关闭启动器…`);
      window.setTimeout(() => {
        api.windowControl("close").catch(() => window.close());
      }, 1200);
    } catch (error) {
      setUpdateProgress(String(error));
      setUpdating(false);
    }
  }, [update, updating, settings.plugin_repo_url]);

  useEffect(() => {
    if (autoCountdown === null || !update || updating) return;
    if (autoCountdown <= 0) {
      void applyStartupUpdate();
      return;
    }
    const timer = window.setTimeout(() => setAutoCountdown((value) => (value ?? 1) - 1), 1000);
    return () => window.clearTimeout(timer);
  }, [autoCountdown, update, updating, applyStartupUpdate]);

  // 订阅统一的进度与日志事件
  useEffect(() => {
    const unlisteners: Array<() => void> = [];

    onInstallProgress((e) => {
      // 新事件到达就取消上一次的自动清理，避免多步骤流程中途被清空
      if (clearProgressTimer.current !== null) {
        window.clearTimeout(clearProgressTimer.current);
        clearProgressTimer.current = null;
      }
      if (e.stage === "control") {
        const next = e.state;
        if (next === "paused") {
          setInstallState("paused");
        } else if (next === "running") {
          setInstallState("running");
        } else if (next === "cancelled") {
          // 取消立即反映到界面，不等到命令返回
          setInstallState("cancelled");
          setBusyAction("");
          setProgress(null);
          setStepProgress(null);
        }
        return;
      }
      if (e.stage === "step") {
        setInstallState("running");
        if (e.step === 1) stickyPercent.current = 0;
        setStepProgress({
          label: e.label,
          step: e.step,
          steps: e.steps,
          completed: e.completed,
          total: e.total,
        });
        // 换步骤时清掉上一步的细分进度，避免百分比串档
        setProgress(null);
        return;
      }
      if (e.stage === "started") {
        // 新一次下载开始：进度从 0 重新计数
        setInstallState("running");
        stickyPercent.current = 0;
      }
      if (e.stage === "task_done" && e.total > 0) {
        stickyPercent.current = Math.min(100, (e.completed / e.total) * 100);
      }
      setProgress(e);
      if (e.stage === "finished" || e.stage === "error") {
        clearProgressTimer.current = window.setTimeout(() => {
          setProgress(null);
          setStepProgress(null);
          setInstallState("idle");
          setBusyAction("");
          clearProgressTimer.current = null;
        }, 4000);
      }
    }).then((fn) => unlisteners.push(fn));

    onInstallLog((e) => {
      logSeq.current += 1;
      const entry: DockLog = { ...e, id: logSeq.current };
      setLogs((prev) => [...prev.slice(-199), entry]);
    }).then((fn) => unlisteners.push(fn));

    return () => unlisteners.forEach((fn) => fn());
  }, []);

  const handleSaveSettings = async (next: AppSettings) => {
    await api.saveSettings(next);
    setSettings(next);
  };

  const handleUsernameChanged = (username: string) => {
    setSettings((prev) => (prev.username === username ? prev : { ...prev, username }));
  };

  const handleAccountsChanged = () => setAccountRevision((value) => value + 1);

  /** 一键清空全部安装日志并收起状态坞 */
  const clearAllLogs = () => {
    setLogs([]);
    setDockOpen(false);
  };

  /** 安全中断实例安装：只请求，后端在下个检查点停下 */
  const handleCancelInstall = async () => {
    if (busyAction) return;
    setBusyAction("cancel");
    setInstallState("cancelled");
    try {
      await api.cancelInstanceInstall();
    } catch {
      // 取消失败不阻塞界面，后端没有安装任务时也会安全返回
    } finally {
      window.setTimeout(() => setBusyAction(""), 1200);
    }
  };

  /** 暂停下载：停止拉取，已完成文件保留 */
  const handlePauseInstall = async () => {
    if (busyAction) return;
    setBusyAction("pause");
    setInstallState("paused");
    try {
      await api.pauseInstanceInstall();
    } catch {
      setInstallState("running");
    } finally {
      window.setTimeout(() => setBusyAction(""), 1200);
    }
  };

  /** 继续下载：从暂停处接着跑 */
  const handleResumeInstall = async () => {
    if (busyAction) return;
    setBusyAction("resume");
    setInstallState("running");
    try {
      await api.resumeInstanceInstall();
    } catch {
      setInstallState("paused");
    } finally {
      window.setTimeout(() => setBusyAction(""), 1200);
    }
  };

  const lang: Lang = settings.launcher_language === "en-US" ? "en-US" : "zh-CN";
  const t = (key: string, params?: Record<string, string | number>) =>
    translate(lang, key, params);

  // 按设置过滤日志：安装类 / 更新类分别隐藏
  const visibleLogs = logs.filter((log) =>
    (log.category ?? "install") === "update"
      ? !settings.hide_update_logs
      : !settings.hide_install_logs
  );
  const latestLog = visibleLogs[visibleLogs.length - 1];

  /**
   * 只展示真实进度：已完成任务 / 总任务，保留两位小数。
   * 阶段内暂时没有任务计数时沿用上一次的真实百分比，不伪造跳变。
   */
  const rawPercent =
    progress && progress.stage === "task_done" && progress.total > 0
      ? (progress.completed / progress.total) * 100
      : null;
  const percent = rawPercent === null ? null : Math.min(100, Math.max(0, rawPercent));

  /** 是否有安装/更新任务在进行 */
  const installing =
    stepProgress !== null ||
    (progress !== null && progress.stage !== "finished" && progress.stage !== "error");

  const displayPercent = percent ?? (installing ? stickyPercent.current : null);
  const percentText = displayPercent === null ? null : `${displayPercent.toFixed(2)}%`;
  /** 底边细线只在真正下载时滚动，暂停/取消即停 */
  const lineRunning = installing && installState !== "paused" && installState !== "cancelled";

  const taskCounts =
    progress && progress.stage === "task_done" && progress.total > 0
      ? t("dock.downloading", { done: progress.completed, total: progress.total })
      : null;

  const baseLabel = stepProgress
    ? taskCounts
      ? `${stepProgress.label} · ${taskCounts}`
      : stepProgress.label
    : taskCounts || t("dock.preparing");

  const progressLabel =
    installState === "paused"
      ? `${t("dock.paused")} · ${baseLabel}`
      : installState === "cancelled"
        ? t("dock.cancelling")
        : baseLabel;

  return (
    <LangContext.Provider value={lang}>
    <ConfirmProvider>
    <div className="app-shell">
      <TitleBar />
      <div className="app-body">
        <Sidebar
          current={page}
          onNavigate={setPage}
          theme={theme}
          onToggleTheme={toggleTheme}
        />
        <main className="main-area">
          {/*
            所有页面保持挂载，仅切换可见性。
            这样在模块间来回切换时，页面内部状态（如“启动中”、实时日志、
            表单编辑进度）不会因组件卸载而丢失或重新刷新。
          */}
          <div className="page-panel" style={{ display: page === "home" ? undefined : "none" }}>
            <HomePage
              username={settings.username}
              maxMemoryMb={settings.max_memory_mb}
              accountRevision={accountRevision}
              onOpenUsers={() => setPage("users")}
              hideLaunchLogs={settings.hide_launch_logs}
              onAccountChanged={handleAccountsChanged}
            />
          </div>
          <div className="page-panel" style={{ display: page === "users" ? undefined : "none" }}>
            <UsersPage
              onAccountsChanged={handleAccountsChanged}
              onUsernameChanged={handleUsernameChanged}
            />
          </div>
          <div className="page-panel" style={{ display: page === "versions" ? undefined : "none" }}>
            <VersionsPage onInstalled={() => {}} />
          </div>
          <div className="page-panel" style={{ display: page === "resources" ? undefined : "none" }}>
            <ResourcesPage />
          </div>
          <div className="page-panel" style={{ display: page === "features" ? undefined : "none" }}>
            <FeaturesPage />
          </div>
          <div className="page-panel" style={{ display: page === "settings" ? undefined : "none" }}>
            {settingsLoaded ? (
              <SettingsPage
                settings={settings}
                onSaveSettings={handleSaveSettings}
                theme={theme}
                onThemeChange={setTheme}
              />
            ) : (
              <div className="empty-state">{t("dock.loadingSettings")}</div>
            )}
          </div>
        </main>
      </div>

      {/* 底部状态坞：统一进度 + 安装日志 */}
      {(progress || stepProgress || visibleLogs.length > 0) && (
        <div className={`status-dock ${dockOpen ? "status-dock-open" : ""}`}>
          <div className="dock-bar">
            <button
              className="dock-icon-btn dock-clear"
              onClick={clearAllLogs}
              title={t("dock.clearLogs")}
              aria-label={t("dock.clearLogs")}
            >
              <Icon name="close" size={12} />
            </button>
            <button className="dock-main" onClick={() => setDockOpen((v) => !v)}>
              {installing && percentText ? (
                <>
                  <span className="dock-text">{progressLabel}</span>
                  <span className="dock-percent">{percentText}</span>
                  <div className="dock-track">
                    <span style={{ width: `${displayPercent ?? 0}%` }} />
                  </div>
                </>
              ) : (
                <span className="dock-text">{latestLog?.message || t("dock.ready")}</span>
              )}
            </button>
            {stepProgress && installState !== "cancelled" && (
              <>
                {installState === "paused" ? (
                  <button
                    className="dock-action"
                    onClick={handleResumeInstall}
                    disabled={busyAction === "resume"}
                    title={t("dock.resumeHint")}
                  >
                    {busyAction === "resume" ? t("dock.resuming") : t("dock.resume")}
                  </button>
                ) : (
                  <button
                    className="dock-action"
                    onClick={handlePauseInstall}
                    disabled={busyAction === "pause"}
                    title={t("dock.pauseHint")}
                  >
                    {busyAction === "pause" ? t("dock.pausing") : t("dock.pause")}
                  </button>
                )}
                <button
                  className="dock-action dock-action-danger"
                  onClick={handleCancelInstall}
                  disabled={busyAction === "cancel"}
                  title={t("dock.cancelInstallHint")}
                >
                  {busyAction === "cancel" ? t("dock.cancelling") : t("dock.cancelInstall")}
                </button>
              </>
            )}
            <button
              className="dock-icon-btn dock-chevron"
              onClick={() => setDockOpen((v) => !v)}
              aria-label={t("dock.toggleLogs")}
            >
              <Icon name="chevron-down" size={14} />
            </button>
          </div>

          {dockOpen && (
            <div className="dock-logs">
              {visibleLogs.length === 0 ? (
                <div className="dock-log text-tertiary">{t("dock.noLogs")}</div>
              ) : (
                visibleLogs.slice(-80).map((log) => (
                  <div key={log.id} className={`dock-log dock-${log.level}`}>
                    <span className="dock-log-text">{log.message}</span>
                    <button
                      className="dock-log-close"
                      onClick={() =>
                        setLogs((prev) => prev.filter((entry) => entry.id !== log.id))
                      }
                      title={t("dock.closeLog")}
                      aria-label={t("dock.closeLog")}
                    >
                      <Icon name="close" size={12} />
                    </button>
                  </div>
                ))
              )}
            </div>
          )}

          {/* 底边细线：只在真正下载时循环滚动，暂停/取消立刻停住 */}
          {lineRunning && (
            <div className="dock-progress-line dock-progress-line-run" aria-hidden="true">
              <span />
              <span />
              <span />
            </div>
          )}
        </div>
      )}
      <UpdateDialog
        update={update}
        updating={updating}
        progress={updateProgress || (autoCountdown !== null ? `${autoCountdown} 秒后自动更新，可点击暂不更新取消` : undefined)}
        onUpdate={() => void applyStartupUpdate()}
        onLater={() => { setUpdate(null); setAutoCountdown(null); setUpdateProgress(""); }}
      />
    </div>
    </ConfirmProvider>
    </LangContext.Provider>
  );
}
