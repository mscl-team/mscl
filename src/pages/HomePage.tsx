/**
 * 首页：核心交互入口
 * - 从已安装版本中选择要启动的版本
 * - 安装 / 启动 / 停止游戏
 * - 实时显示安装进度与游戏日志（可切换详细日志查看离线噪音）
 */

import { useEffect, useRef, useState } from "react";
import { Icon } from "../components/Icon";
import { MinecraftIcon } from "../components/MinecraftIcon";
import { UserAvatar } from "../components/UserAvatar";
import { useT } from "../lib/i18n";
import { loaderSpriteId } from "../lib/minecraftIcons";
import {
  api,
  onInstallProgress,
  onLaunchStatus,
  onGameOutput,
  type AccountInfo,
  type InstanceInfo,
  type JavaInfo,
  type InstallProgressEvent,
  type LaunchStatusEvent,
  type GameOutputEvent,
} from "../lib/api";

interface HomePageProps {
  username: string;
  /** 分配内存 MB，0 表示自动 */
  maxMemoryMb: number;
  /** 账户变更后触发重新读取当前账户 */
  accountRevision: number;
  onOpenUsers: () => void;
  /** 设置里关闭启动日志后不再显示游戏输出 */
  hideLaunchLogs?: boolean;
  /** 切换账号后通知外层刷新（用户名、皮肤等） */
  onAccountChanged?: () => void;
}

export function HomePage({
  username,
  maxMemoryMb,
  accountRevision,
  onOpenUsers,
  hideLaunchLogs = false,
  onAccountChanged,
}: HomePageProps) {
  const t = useT();
  const [latestRelease, setLatestRelease] = useState<string>(t("home.detectChecking"));
  const [installed, setInstalled] = useState<string[]>([]);
  const [selected, setSelected] = useState<string>("");
  const [java, setJava] = useState<JavaInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<InstallProgressEvent | null>(null);
  const [launchStatus, setLaunchStatus] = useState<LaunchStatusEvent | null>(null);
  const [logs, setLogs] = useState<GameOutputEvent[]>([]);
  const [showVerbose, setShowVerbose] = useState(false);
  const [message, setMessage] = useState<string>("");
  const [accounts, setAccounts] = useState<AccountInfo[]>([]);
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  const [versionMenuOpen, setVersionMenuOpen] = useState(false);
  const versionMenuRef = useRef<HTMLDivElement>(null);
  const [accountMenuOpen, setAccountMenuOpen] = useState(false);
  const [switchingAccount, setSwitchingAccount] = useState(false);
  const accountMenuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // 已安装列表是本地扫描（快），先加载并让版本选择器立即可用；
    // 远端版本清单走网络（慢），只用于"推荐最新版"的默认选择。
    let latest = "";
    api.getVersionList()
      .then((list) => {
        latest = list.latest_release;
        setLatestRelease(latest || t("home.detectFailed"));
      })
      .catch(() => setLatestRelease(t("home.detectFailed")));

    api.getInstalledVersions()
      .then((versions) => {
        setInstalled(versions);
        if (versions.length > 0) {
          setSelected(
            (prev) =>
              prev || (latest && versions.includes(latest) ? latest : versions[0])
          );
        }
      })
      .catch(() => setInstalled([]));

    api.detectJava()
      .then(setJava)
      .catch(() => setJava(null));
    api.backendStatus()
      .then((s) => setRunning(s.running_pid !== null))
      .catch(() => {});

    // 监听后端事件
    const unlisteners: Array<() => void> = [];
    onInstallProgress((e) => {
      setProgress(e);
      if (e.stage === "finished") {
        setBusy(false);
        setMessage(
          t("home.installDone", {
            downloaded: e.downloaded,
            skipped: e.skipped,
            failed: e.failed ? t("home.installDoneFailed", { n: e.failed }) : "",
          })
        );
      } else if (e.stage === "error") {
        setBusy(false);
        setMessage(t("home.installFailed", { message: e.message }));
      }
    }).then((fn) => unlisteners.push(fn));

    onLaunchStatus((e) => {
      setLaunchStatus(e);
      if (e.status === "failed") {
        setMessage(t("home.launchFailEvent", { message: e.message }));
        setBusy(false);
      } else if (e.status === "preparing") {
        setMessage(e.message);
      } else if (e.status === "started") {
        setRunning(true);
        setBusy(false);
      } else if (e.status === "exited") {
        setRunning(false);
        setBusy(false);
        setMessage(t("home.exited", { code: e.code }));
      }
    }).then((fn) => unlisteners.push(fn));

    onGameOutput((e: GameOutputEvent) => {
      setLogs((prev) => [...prev.slice(-199), e]);
    }).then((fn) => unlisteners.push(fn));

    return () => unlisteners.forEach((fn) => fn());
  }, []);

  useEffect(() => {
    api.listAccounts()
      .then(setAccounts)
      .catch(() => setAccounts([]));
    api.listInstances()
      .then(setInstances)
      .catch(() => setInstances([]));
  }, [accountRevision]);

  useEffect(() => {
    if (!versionMenuOpen) return;
    const onPointerDown = (event: MouseEvent) => {
      if (!versionMenuRef.current?.contains(event.target as Node)) {
        setVersionMenuOpen(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setVersionMenuOpen(false);
    };
    document.addEventListener("mousedown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [versionMenuOpen]);

  // 账号下拉：点击外部或 Esc 收起
  useEffect(() => {
    if (!accountMenuOpen) return;
    const onPointerDown = (event: MouseEvent) => {
      if (!accountMenuRef.current?.contains(event.target as Node)) {
        setAccountMenuOpen(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setAccountMenuOpen(false);
    };
    document.addEventListener("mousedown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [accountMenuOpen]);

  /** 切换当前登录账号：后端会持久化为当前账户 */
  const handleSwitchAccount = async (id: string) => {
    if (switchingAccount) return;
    setSwitchingAccount(true);
    try {
      const next = await api.setActiveAccount(id);
      setAccounts(next);
      const active = next.find((account) => account.is_active);
      setMessage(t("home.accountSwitched", { name: active?.username ?? "" }));
      setAccountMenuOpen(false);
      onAccountChanged?.();
    } catch (e) {
      setMessage(t("home.accountSwitchFailed", { error: String(e) }));
    } finally {
      setSwitchingAccount(false);
    }
  };

  const refreshInstalled = () =>
    api.getInstalledVersions()
      .then(setInstalled)
      .catch(() => {});

  const isSelectedInstalled = selected !== "" && installed.includes(selected);

  const handleInstall = async () => {
    if (busy) return;
    const target = selected || latestRelease;
    if (!target || target.includes("失败") || target.includes("检测") || target.includes("Unavailable") || target.includes("Detecting")) {
      setMessage(t("home.noVersion"));
      return;
    }
    setBusy(true);
    setMessage("");
    setProgress(null);
    try {
      const result = await api.installGame(target);
      if (result.failed > 0) {
        const detail = result.errors.slice(0, 5).join("；");
        setMessage(t("home.installPartial", { n: result.failed, detail: detail ? `：${detail}` : "" }));
      }
      await refreshInstalled();
      if (!selected) setSelected(target);
    } catch (e) {
      setMessage(t("home.installError", { error: String(e) }));
    } finally {
      setBusy(false);
    }
  };

  const handleLaunch = async () => {
    if (busy || running) return;
    if (!selected) {
      setMessage(t("home.launchNoInstall"));
      return;
    }
    if (!isSelectedInstalled) {
      setMessage(t("home.launchNotInstalled", { v: selected }));
      return;
    }
    setBusy(true);
    setMessage(t("home.launching"));
    setLogs([]);
    try {
      await api.launchGame(selected, username || "Player", maxMemoryMb);
      setRunning(true);
      setMessage(t("home.launched", { v: selected }));
    } catch (e) {
      setMessage(t("home.launchError", { error: String(e) }));
    } finally {
      setBusy(false);
    }
  };

  const handleStop = async () => {
    if (!running) return;
    setBusy(true);
    setMessage(t("home.stopping"));
    try {
      await api.stopGame();
      setRunning(false);
      setMessage(t("home.stopped"));
    } catch (e) {
      setMessage(t("home.stopError", { error: String(e) }));
    } finally {
      setBusy(false);
    }
  };

  // 计算进度百分比
  let progressPercent = 0;
  if (progress?.stage === "task_done" && progress.total > 0) {
    progressPercent = Math.round((progress.completed / progress.total) * 100);
  }

  const visibleLogs = showVerbose ? logs : logs.filter((l) => !l.noise);
  const activeAccount = accounts.find((account) => account.is_active) ?? null;
  const iconForVersion = (versionId: string) => {
    const meta = instances.find((instance) => instance.version_id === versionId);
    return meta?.icon || loaderSpriteId(meta?.loader ?? "", versionId);
  };
  const versionIconId = iconForVersion(selected);
  const activeMethod = activeAccount?.kind === "external"
    ? t("users.methodExternal")
    : activeAccount?.kind === "premium"
      ? t("users.methodPremium")
      : t("users.methodOffline");

  return (
    <div className="page home-page">
      <header className="page-header home-page-header">
        <div className="home-page-heading">
          <h1>{t("home.greeting", { name: activeAccount?.username || username || t("home.adventurer") })}</h1>
          <p>{t("home.ready")}</p>
        </div>
        <div className="home-user-select" ref={accountMenuRef}>
          <button
            className={`home-user-chip ${accountMenuOpen ? "open" : ""}`}
            onClick={() => setAccountMenuOpen((open) => !open)}
            aria-haspopup="listbox"
            aria-expanded={accountMenuOpen}
            title={t("home.switchAccount")}
          >
            <UserAvatar
              src={activeAccount?.avatar_url}
              name={activeAccount?.username || username || "Player"}
              size={36}
            />
            <span className="home-user-text">
              <strong>{activeAccount?.username || username || t("home.adventurer")}</strong>
              <span className={`auth-badge auth-${activeAccount?.kind ?? "offline"}`}>
                {activeMethod}
              </span>
            </span>
            <Icon name="chevron-down" size={13} />
          </button>

          {accountMenuOpen && (
            <div className="account-select-menu" role="listbox">
              {accounts.length === 0 ? (
                <div className="account-select-empty">{t("home.noAccount")}</div>
              ) : (
                accounts.map((account) => (
                  <button
                    key={account.id}
                    className={`account-select-option ${
                      account.is_active ? "active" : ""
                    }`}
                    role="option"
                    aria-selected={account.is_active}
                    disabled={switchingAccount}
                    onClick={() => handleSwitchAccount(account.id)}
                  >
                    <UserAvatar
                      src={account.avatar_url}
                      name={account.username}
                      size={24}
                    />
                    <span className="account-select-name">{account.username}</span>
                    <span className={`auth-badge auth-${account.kind}`}>
                      {account.kind === "external"
                        ? t("users.methodExternal")
                        : account.kind === "premium"
                          ? t("users.methodPremium")
                          : t("users.methodOffline")}
                    </span>
                    {account.is_active && (
                      <span className="version-select-check">
                        <Icon name="check" size={13} />
                      </span>
                    )}
                  </button>
                ))
              )}
              <button
                className="account-select-manage"
                onClick={() => {
                  setAccountMenuOpen(false);
                  onOpenUsers();
                }}
              >
                <Icon name="arrow" size={12} />
                {t("home.manageAccounts")}
              </button>
            </div>
          )}
        </div>
      </header>

      <div className="home-content">
        {/* 主版本卡片 */}
        <section className="hero-card">
          <div className="hero-card-top">
            <div className="hero-badge">{t("home.heroBadge")}</div>
            <div className="hero-moon">☾</div>
          </div>

          <div className="hero-version-row">
            <div className="hero-version">{selected || t("home.heroNone")}</div>
            {installed.length > 0 && (
              <div className="home-version-select" ref={versionMenuRef}>
                <button
                  className="version-select-trigger"
                  onClick={() => setVersionMenuOpen((open) => !open)}
                  aria-haspopup="listbox"
                  aria-expanded={versionMenuOpen}
                  title={t("home.heroSelect")}
                >
                  <MinecraftIcon id={versionIconId} size={17} />
                  <span className="version-select-label">
                    {selected || t("home.heroNone")}
                  </span>
                  <Icon name="chevron-down" size={12} />
                </button>

                {versionMenuOpen && (
                  <div className="version-select-menu" role="listbox">
                    {installed.map((v) => (
                      <button
                        key={v}
                        className={`version-select-option ${v === selected ? "active" : ""}`}
                        role="option"
                        aria-selected={v === selected}
                        onClick={() => {
                          setSelected(v);
                          setVersionMenuOpen(false);
                        }}
                      >
                        <MinecraftIcon id={iconForVersion(v)} size={18} />
                        <span className="version-select-name">{v}</span>
                        {v === selected && (
                          <span className="version-select-check">
                            <Icon name="check" size={13} />
                          </span>
                        )}
                      </button>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>

          <div className="hero-meta">
            <span className={`meta-chip ${isSelectedInstalled ? "" : "meta-chip-muted"}`}>
              {isSelectedInstalled ? t("common.installed") : t("common.notInstalled")}
            </span>
            <span className="text-tertiary">
              {installed.length > 0
                ? t("home.totalInstalled", { n: installed.length })
                : t("home.launchNoInstall")}
            </span>
          </div>

          <div className="hero-actions">
            <button
              className="btn btn-primary"
              onClick={handleLaunch}
              disabled={busy || running || !isSelectedInstalled}
            >
              <Icon name="play" size={18} />
              {t("home.play")}
            </button>
            <button className="btn btn-ghost" onClick={handleInstall} disabled={busy || running}>
              <Icon name="download" size={18} />
              {busy ? t("home.wait") : isSelectedInstalled ? t("home.reinstall") : t("home.installUpdate")}
            </button>
            {running && (
              <button className="btn btn-danger" onClick={handleStop} disabled={busy}>
                <Icon name="stop" size={18} />
                {t("home.stop")}
              </button>
            )}
          </div>
        </section>

        {/* 状态区 */}
        <div className="status-grid">
          <div className="info-card">
            <div className="info-label">{t("home.java")}</div>
            <div className="info-value">
              {java ? (java.major_version ? t("home.javaVersion", { n: java.major_version }) : t("home.javaDetected")) : t("home.javaMissing")}
            </div>
            <div className="info-hint text-tertiary">
              {java ? java.path : t("home.javaHint")}
            </div>
          </div>
          <div className="info-card">
            <div className="info-label">{t("home.status")}</div>
            <div className="info-value">
              {running ? t("home.statusPlaying") : busy ? t("home.statusBusy") : t("home.statusReady")}
            </div>
            <div className="info-hint text-tertiary">
              {running && launchStatus?.status === "started"
                ? `PID ${launchStatus.pid}`
                : t("home.awaiting")}
            </div>
          </div>
        </div>

        {/* 进度条 */}
        {progress && progress.stage !== "finished" && (
          <div className="progress-block">
            <div className="progress-head">
              <span>{t("home.downloading")}</span>
              <span>{progressPercent}%</span>
            </div>
            <div className="progress-track">
              <div className="progress-fill" style={{ width: `${progressPercent}%` }} />
            </div>
          </div>
        )}

        {/* 消息提示 */}
        {message && (
          <div key={message} className="message-bar">
            {message}
          </div>
        )}

        {/* 游戏日志（可在设置 → 日志与缓存 里隐藏） */}
        {!hideLaunchLogs && logs.length > 0 && (
          <div className="log-panel">
            <div className="log-head">
              <span>{t("home.gameLog")}</span>
              <button
                className={`log-toggle ${showVerbose ? "active" : ""}`}
                onClick={() => setShowVerbose((v) => !v)}
                title={t("home.verboseTitle")}
              >
                <Icon name="info" size={14} />
                {t("home.verbose")}
              </button>
            </div>
            <div className="log-box">
              {visibleLogs.length === 0 ? (
                <div className="log-line text-tertiary">{t("home.logHidden")}</div>
              ) : (
                visibleLogs.map((l, i) => (
                  <div key={i} className={`log-line ${l.stream === "stderr" ? "log-stderr" : ""}`}>
                    {l.line}
                  </div>
                ))
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
