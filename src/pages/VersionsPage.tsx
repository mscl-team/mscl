/**
 * 版本管理页：展示已安装版本 + 可获取的最新版本
 * 支持逐行安装 / 校验 / 修复 / 卸载（卸载需二次确认防止误删）
 * "全部"列表支持分页，避免一次渲染上千个版本导致卡顿。
 */

import { useContext, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Icon } from "../components/Icon";
import { useConfirm } from "../components/ConfirmDialog";
import { ItemIconPicker } from "../components/ItemIconPicker";
import { LoaderBadge, MinecraftIcon } from "../components/MinecraftIcon";
import {
  inferLoaderFromId,
  isCustomImageIcon,
  loaderSpriteId,
  spriteLabel,
} from "../lib/minecraftIcons";
import { LangContext, useT } from "../lib/i18n";
import {
  api,
  onInstallProgress,
  type GameRootInfo,
  type InstallProgressEvent,
  type InstanceInfo,
  type InstanceLaunchOptions,
  type InstanceResource,
  type RunningGameInfo,
  type VerifyReport,
  type VersionInfo,
} from "../lib/api";

interface VersionsPageProps {
  onInstalled: () => void;
}

const PAGE_SIZE = 20;

/** 字节数人类可读化 */
function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / Math.pow(1024, i);
  return `${value.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

function sameRoot(left: string, right: string): boolean {
  return left.replace(/\//g, "\\").toLowerCase() === right.replace(/\//g, "\\").toLowerCase();
}

function rootLabel(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.slice(-2).join("\\") || path;
}

function instanceKey(instance: InstanceInfo): string {
  return `${instance.game_root || ""}|${instance.version_id}`;
}

export function VersionsPage({ onInstalled }: VersionsPageProps) {
  const t = useT();
  const lang = useContext(LangContext);
  const dialogs = useConfirm();
  const iconName = (id: string) =>
    isCustomImageIcon(id) ? t("versions.iconCustomImage") : spriteLabel(id, lang);
  const [versions, setVersions] = useState<VersionInfo[]>([]);
  const [installed, setInstalled] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState<"installed" | "all" | "running">("installed");
  const [allPage, setAllPage] = useState(0);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [verifyingId, setVerifyingId] = useState<string | null>(null);
  const [confirmId, setConfirmId] = useState<string | null>(null);
  const [message, setMessage] = useState("");
  const [reportId, setReportId] = useState<string | null>(null);
  const [report, setReport] = useState<VerifyReport | null>(null);
  // 新建实例表单
  const [builderOpen, setBuilderOpen] = useState(false);
  const [instName, setInstName] = useState("");
  const [instMc, setInstMc] = useState("");
  const [instLoader, setInstLoader] = useState<"vanilla" | "fabric" | "forge" | "neoforge">(
    "fabric"
  );
  const [withFabricApi, setWithFabricApi] = useState(true);
  const [withOptifine, setWithOptifine] = useState(false);
  const [instDesc, setInstDesc] = useState("");
  const [instIcon, setInstIcon] = useState("");
  const [instTags, setInstTags] = useState("");
  // 安装进度（内联展示）
  const [progress, setProgress] = useState<InstallProgressEvent | null>(null);
  // 已安装版本的多选（批量编辑）
  const [selectedInstalled, setSelectedInstalled] = useState<string[]>([]);
  const [launchingId, setLaunchingId] = useState<string | null>(null);
  // 运行实例 / 多游戏目录
  const [running, setRunning] = useState<RunningGameInfo[]>([]);
  const [allInstances, setAllInstances] = useState<InstanceInfo[]>([]);
  const [gameRoots, setGameRoots] = useState<GameRootInfo[]>([]);
  const [runSelected, setRunSelected] = useState<string[]>([]);
  const [runBusy, setRunBusy] = useState(false);
  // 自定义启动
  const [customOpen, setCustomOpen] = useState(false);
  const [customTarget, setCustomTarget] = useState<InstanceInfo | null>(null);
  const [customMemory, setCustomMemory] = useState(0);
  const [customJvm, setCustomJvm] = useState("");
  const [customGameArgs, setCustomGameArgs] = useState("");
  const [customSaving, setCustomSaving] = useState(false);
  // 实例列表与编辑弹窗
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  const [editOpen, setEditOpen] = useState(false);
  const [editId, setEditId] = useState("");
  const [editName, setEditName] = useState("");
  const [editDesc, setEditDesc] = useState("");
  const [editIcon, setEditIcon] = useState("");
  const [editTags, setEditTags] = useState("");
  const [editMc, setEditMc] = useState("");
  const [editLoader, setEditLoader] = useState("vanilla");
  const [editResourceList, setEditResourceList] = useState<InstanceResource[]>([]);
  const [editVerify, setEditVerify] = useState<"inherit" | "on" | "off">("inherit");
  const [editVerifyRes, setEditVerifyRes] = useState<"inherit" | "on" | "off">("inherit");
  const [editCheckDeps, setEditCheckDeps] = useState<"inherit" | "on" | "off">("inherit");
  const [builderPickerOpen, setBuilderPickerOpen] = useState(false);
  const [editPickerOpen, setEditPickerOpen] = useState(false);
  const [batchPickerOpen, setBatchPickerOpen] = useState(false);
  // 批量编辑弹窗
  const [batchOpen, setBatchOpen] = useState(false);
  const [batchIcon, setBatchIcon] = useState("");
  const [batchDesc, setBatchDesc] = useState("");
  const [batchTagsInput, setBatchTagsInput] = useState("");

  useEffect(() => {
    const unlisten = onInstallProgress((event) => {
      setProgress(event);
      if (event.stage === "finished") {
        window.setTimeout(() => setProgress(null), 3000);
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  const refreshInstances = () =>
    api.listInstances().then(setInstances).catch(() => {});

  const refreshRunning = () =>
    api.getRunningInstances().then(setRunning).catch(() => setRunning([]));

  const refreshAllInstances = () => {
    api.listAllInstances().then(setAllInstances).catch(() => setAllInstances([]));
    api.listGameRoots().then(setGameRoots).catch(() => setGameRoots([]));
  };

  useEffect(() => {
    refreshInstances();
  }, [installed]);

  useEffect(() => {
    refreshRunning();
    const timer = window.setInterval(refreshRunning, 2000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    refreshAllInstances();
  }, [installed]);

  const toggleInstalled = (id: string) => {
    setSelectedInstalled((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id]
    );
  };

  const applyBatchEdit = async () => {
    let ok = 0;
    for (const id of selectedInstalled) {
      const info = instances.find((i) => i.version_id === id);
      if (!info) continue;
      const tags = batchTagsInput.trim()
        ? batchTagsInput.split(",").map((x) => x.trim()).filter(Boolean)
        : info.tags ?? [];
      try {
        await api.updateInstanceMeta({
          version_id: id,
          name: info.name,
          description: batchDesc.trim() || info.description || "",
          icon: batchIcon.trim() || info.icon || "",
          tags,
          verify_game_on_launch: info.verify_game_on_launch ?? null,
          verify_resources_on_launch: info.verify_resources_on_launch ?? null,
          check_dependencies: info.check_dependencies ?? null,
        });
        ok += 1;
      } catch {
        // 单个失败继续
      }
    }
    setMessage(
      t("versions.batchUpdated", {
        n: ok,
        suffix: ok > 0 ? "" : t("versions.batchNoMeta"),
      })
    );
    setBatchOpen(false);
    setSelectedInstalled([]);
    setBatchTagsInput("");
    setBatchIcon("");
    setBatchDesc("");
    refreshInstances();
  };

  const fallbackInstance = (versionId: string): InstanceInfo => ({
    version_id: versionId,
    name: versionId,
    minecraft_version: versionId,
    loader: "vanilla",
    loader_version: null,
    description: "",
    icon: "",
    tags: [],
    verify_game_on_launch: null,
    verify_resources_on_launch: null,
    check_dependencies: null,
    game_root: "",
  });

  const runsFor = (instance: InstanceInfo) =>
    running.filter(
      (run) =>
        run.version_id === instance.version_id &&
        sameRoot(run.game_root, instance.game_root || "")
    );

  const launchOne = async (
    instance: InstanceInfo,
    options?: InstanceLaunchOptions,
    silent = false
  ) => {
    const key = instanceKey(instance);
    setLaunchingId(key);
    if (!silent) setMessage(t("versions.launchMsg", { id: instance.name || instance.version_id }));
    try {
      const settings = await api.getSettings();
      const result = await api.launchInstance({
        version_id: instance.version_id,
        username: settings.username?.trim() || "Player",
        max_memory_mb: options?.max_memory_mb ?? 0,
        game_root: instance.game_root || null,
        jvm_args: options?.jvm_args || null,
        game_args: options?.game_args || null,
      });
      if (result.pid) {
        if (!silent) {
          setMessage(
            t("versions.launchedPid", { id: instance.name || instance.version_id, pid: result.pid })
          );
        }
      } else {
        setMessage(t("versions.launchFailMsg", { id: instance.version_id }));
      }
      await refreshRunning();
      await refreshAllInstances();
    } catch (e) {
      setMessage(t("versions.launchFailDetail", { error: String(e) }));
    } finally {
      setLaunchingId(null);
    }
  };

  const handleLaunch = async (versionId: string) => {
    if (launchingId) return;
    const instance =
      instances.find((item) => item.version_id === versionId) ??
      allInstances.find((item) => item.version_id === versionId) ??
      fallbackInstance(versionId);
    await launchOne(instance);
  };

  const handleStopRun = async (run: RunningGameInfo) => {
    try {
      await api.stopRun(run.run_id);
      await refreshRunning();
      setMessage(t("versions.instanceStopped", { name: run.version_id }));
    } catch (e) {
      setMessage(t("versions.stopFailed", { error: String(e) }));
    }
  };

  const handleStopInstance = async (instance: InstanceInfo) => {
    const targets = runsFor(instance);
    if (targets.length === 0) return;
    try {
      for (const run of targets) await api.stopRun(run.run_id);
      await refreshRunning();
      setMessage(t("versions.instanceStopped", { name: instance.name || instance.version_id }));
    } catch (e) {
      setMessage(t("versions.stopFailed", { error: String(e) }));
    }
  };

  const handleBatchStart = async () => {
    const selected = allInstances.filter((item) => runSelected.includes(instanceKey(item)));
    if (selected.length === 0) {
      setMessage(t("versions.selectInstancesFirst"));
      return;
    }
    setRunBusy(true);
    try {
      for (const instance of selected) {
        await launchOne(instance, undefined, true);
      }
      setMessage(t("versions.batchStarted", { n: selected.length }));
    } finally {
      setRunBusy(false);
    }
  };

  const handleBatchStop = async () => {
    const selectedKeys = new Set(runSelected);
    const targets = running.filter((run) =>
      selectedKeys.has(`${run.game_root}|${run.version_id}`)
    );
    if (targets.length === 0) {
      setMessage(t("versions.noSelectedRunning"));
      return;
    }
    setRunBusy(true);
    try {
      for (const run of targets) await api.stopRun(run.run_id);
      await refreshRunning();
      setMessage(t("versions.batchStopped", { n: targets.length }));
    } catch (e) {
      setMessage(t("versions.stopFailed", { error: String(e) }));
    } finally {
      setRunBusy(false);
    }
  };

  const handleStopAll = async () => {
    const ok = await dialogs.confirm(t("versions.stopAllConfirm"), {
      title: t("versions.stopAll"),
      confirmText: t("versions.stopAll"),
      danger: true,
    });
    if (!ok) return;
    setRunBusy(true);
    try {
      await api.stopGame();
      await refreshRunning();
      setMessage(t("versions.allStopped"));
    } catch (e) {
      setMessage(t("versions.stopFailed", { error: String(e) }));
    } finally {
      setRunBusy(false);
    }
  };

  const handleAddGameRoot = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: t("versions.addGameRoot"),
      });
      if (typeof selected !== "string" || !selected.trim()) return;
      const roots = await api.addGameRoot(selected.trim());
      setGameRoots(roots);
      refreshAllInstances();
      setMessage(t("versions.gameRootAdded"));
    } catch (e) {
      setMessage(t("versions.gameRootFailed", { error: String(e) }));
    }
  };

  const handleRemoveGameRoot = async (path: string) => {
    const ok = await dialogs.confirm(t("versions.removeGameRootConfirm", { path }), {
      title: t("versions.removeGameRoot"),
      confirmText: t("common.delete"),
      danger: true,
    });
    if (!ok) return;
    try {
      const roots = await api.removeGameRoot(path);
      setGameRoots(roots);
      refreshAllInstances();
      setMessage(t("versions.gameRootRemoved"));
    } catch (e) {
      setMessage(t("versions.gameRootFailed", { error: String(e) }));
    }
  };

  const openCustomLaunch = async (instance: InstanceInfo) => {
    setCustomTarget(instance);
    setCustomMemory(0);
    setCustomJvm("");
    setCustomGameArgs("");
    setCustomOpen(true);
    try {
      const options = await api.getInstanceLaunchOptions(
        instance.version_id,
        instance.game_root || null
      );
      setCustomMemory(options.max_memory_mb ?? 0);
      setCustomJvm(options.jvm_args || "");
      setCustomGameArgs(options.game_args || "");
    } catch {
      // 无配置时使用空表单
    }
  };

  const handleCustomLaunch = async () => {
    if (!customTarget) return;
    setCustomSaving(true);
    try {
      const options: InstanceLaunchOptions = {
        max_memory_mb: customMemory || null,
        jvm_args: customJvm.trim(),
        game_args: customGameArgs.trim(),
      };
      await api.saveInstanceLaunchOptions(
        customTarget.version_id,
        options,
        customTarget.game_root || null
      );
      await launchOne(customTarget, options);
      setCustomOpen(false);
    } catch (e) {
      setMessage(t("versions.launchFailDetail", { error: String(e) }));
    } finally {
      setCustomSaving(false);
    }
  };

  const toggleRunSelected = (instance: InstanceInfo) => {
    const key = instanceKey(instance);
    setRunSelected((prev) =>
      prev.includes(key) ? prev.filter((value) => value !== key) : [...prev, key]
    );
  };


  const openEdit = (id: string) => {
    const target = instances.find((ins) => ins.version_id === id);
    setEditId(id);
    // 无实例元数据也能编辑：版本 id 即默认名称与游戏版本
    setEditName(target?.name ?? id);
    setEditDesc(target?.description ?? "");
    setEditIcon(target?.icon ?? "");
    setEditTags((target?.tags ?? []).join(", "));
    setEditMc(target?.minecraft_version ?? id);
    setEditLoader(target?.loader ?? "vanilla");
    const tri = (v: boolean | null | undefined): "inherit" | "on" | "off" =>
      v === null || v === undefined ? "inherit" : v ? "on" : "off";
    setEditVerify(tri(target?.verify_game_on_launch));
    setEditVerifyRes(tri(target?.verify_resources_on_launch));
    setEditCheckDeps(tri(target?.check_dependencies));
    // 实例资源列表（无元数据时为空）
    api.listInstanceResources(id).then(setEditResourceList).catch(() => setEditResourceList([]));
    setEditOpen(true);
  };

  const handleSaveMeta = async () => {
    if (!editId) return;
    try {
      await api.updateInstanceMeta({
        version_id: editId,
        name: editName,
        description: editDesc,
        icon: editIcon,
        tags: editTags.split(",").map((s) => s.trim()).filter(Boolean),
        minecraft_version: editMc || null,
        loader: editLoader || null,
        verify_game_on_launch: editVerify === "inherit" ? null : editVerify === "on",
        verify_resources_on_launch: editVerifyRes === "inherit" ? null : editVerifyRes === "on",
        check_dependencies: editCheckDeps === "inherit" ? null : editCheckDeps === "on",
      });
      setMessage(t("versions.saved"));
      setEditOpen(false);
      refreshInstances();
      refreshInstalled();
    } catch (e) {
      setMessage(t("versions.saveFailed", { error: String(e) }));
    }
  };

  const handleDeleteInstance = async () => {
    if (!editId) return;
    const ok = await dialogs.confirm(
      t("versions.deleteInstanceConfirm", { name: editName || editId }),
      {
        title: t("versions.deleteInstance"),
        confirmText: t("common.delete"),
        danger: true,
      }
    );
    if (!ok) return;
    try {
      await api.deleteInstance(editId);
      setMessage(t("versions.instanceDeleted"));
      setEditOpen(false);
      setEditId("");
      refreshInstances();
      refreshInstalled();
    } catch (e) {
      setMessage(t("versions.deleteInstanceFailed", { error: String(e) }));
    }
  };

  const handleOpenFolder = async () => {
    if (!editId) return;
    try {
      await api.openInstanceDir(editId);
    } catch (e) {
      setMessage(t("versions.openFolderFailed", { error: String(e) }));
    }
  };

  const load = () => {
    setLoading(true);
    api.getVersionList()
      .then((list) => {
        setVersions(list.versions);
        setAllPage(0);
      })
      .catch(() => setVersions([]))
      .finally(() => setLoading(false));
    refreshInstalled();
  };

  const refreshInstalled = () => {
    api.getInstalledVersions()
      .then(setInstalled)
      .catch(() => setInstalled([]));
  };

  useEffect(load, []);

  // 默认选中最新正式版，便于直接创建实例
  useEffect(() => {
    if (!instMc && versions.length > 0) {
      const latest = versions.find(
        (v) => v.version_type === "release" && v.is_latest_release
      );
      setInstMc(latest?.id ?? versions.find((v) => v.version_type === "release")?.id ?? "");
    }
  }, [versions, instMc]);

  const handleCreateInstance = async () => {
    if (busyId || !instMc) return;
    setBusyId("create");
    setMessage(t("versions.creatingMsg", { name: instName.trim() || instMc }));
    try {
      const result = await api.createInstance({
        name: instName.trim() || t("versions.defaultName", { v: instMc }),
        minecraft_version: instMc,
        loader: instLoader,
        install_fabric_api: withFabricApi && instLoader === "fabric",
        install_optifine: withOptifine,
        description: instDesc,
        icon: instIcon,
        tags: instTags.split(",").map((s) => s.trim()).filter(Boolean),
      });
      setMessage(t("versions.created", { id: result.version_id }));
      setBuilderOpen(false);
      setInstName("");
      refreshInstalled();
      onInstalled();
    } catch (e) {
      const text = String(e);
      if (text.includes("已取消")) {
        setMessage(t("versions.createCancelled"));
        refreshInstalled();
      } else {
        setMessage(t("versions.createFailed", { error: text }));
      }
    } finally {
      setBusyId(null);
    }
  };

  const handleInstall = async (id: string, isInstalled = false) => {
    if (busyId || verifyingId) return;

    // 未安装的版本：先弹出实例编辑框，编辑确认后再下载
    if (!isInstalled) {
      setInstMc(id);
      setInstName(t("versions.defaultName", { v: id }));
      setInstLoader("vanilla");
      setWithFabricApi(false);
      setWithOptifine(false);
      setInstDesc("");
      setInstIcon("");
      setInstTags("");
      setBuilderOpen(true);
      setMessage(t("versions.selectedEdit", { id }));
      return;
    }

    setBusyId(id);
    setMessage(t("versions.installing", { id }));
    try {
      const result = await api.installGame(id);
      if (result.failed > 0) {
        const detail = result.errors.slice(0, 5).join("；");
        setMessage(
          t("versions.installPartial", {
            id,
            n: result.failed,
            detail: detail ? `：${detail}` : "",
          })
        );
      } else {
        setMessage(
          t("versions.installDone", {
            id,
            downloaded: result.downloaded,
            skipped: result.skipped,
          })
        );
      }
      refreshInstalled();
      onInstalled();
    } catch (e) {
      setMessage(t("versions.installFailed", { id, error: String(e) }));
    } finally {
      setBusyId(null);
    }
  };

  const handleVerify = async (id: string) => {
    if (verifyingId || busyId) return;
    setVerifyingId(id);
    setReportId(null);
    setReport(null);
    setMessage(t("versions.verifying", { id }));
    try {
      const result = await api.verifyVersion(id);
      setReportId(id);
      setReport(result);
      setMessage(
        result.missing === 0 && result.corrupted === 0
          ? t("versions.verifyOk", { id, total: result.total })
          : t("versions.verifyBad", { id, missing: result.missing, corrupted: result.corrupted })
      );
    } catch (e) {
      setMessage(t("versions.verifyFailed", { id, error: String(e) }));
    } finally {
      setVerifyingId(null);
    }
  };

  const handleRepair = async (id: string) => {
    if (busyId || verifyingId) return;
    setBusyId(id);
    setMessage(t("versions.repairing", { id }));
    try {
      const result = await api.repairVersion(id);
      setMessage(
        t("versions.repaired", {
          downloaded: result.downloaded,
          skipped: result.skipped,
          failed: result.failed ? t("versions.repairFailedPart", { n: result.failed }) : "",
        })
      );
      const again = await api.verifyVersion(id);
      setReportId(id);
      setReport(again);
      refreshInstalled();
    } catch (e) {
      setMessage(t("versions.repairFailed", { id, error: String(e) }));
    } finally {
      setBusyId(null);
    }
  };

  const handleUninstall = async (id: string) => {
    if (busyId || verifyingId) return;
    if (confirmId !== id) {
      setConfirmId(id);
      return;
    }
    setConfirmId(null);
    setBusyId(id);
    setMessage("");
    try {
      await api.uninstallVersion(id);
      setMessage(t("versions.uninstalled", { id }));
      if (reportId === id) {
        setReportId(null);
        setReport(null);
      }
      refreshInstalled();
    } catch (e) {
      setMessage(t("versions.uninstallFailed", { id, error: String(e) }));
    } finally {
      setBusyId(null);
    }
  };

  // "已安装" tab 直接用已安装列表渲染，"全部" tab 分页展示完整版本清单
  const totalPages = Math.max(1, Math.ceil(versions.length / PAGE_SIZE));
  const safePage = Math.min(allPage, totalPages - 1);
  const shown = versions.slice(safePage * PAGE_SIZE, (safePage + 1) * PAGE_SIZE);

  return (
    <div className="page versions-page">
      <header className="page-header">
        <h1>{t("versions.title")}</h1>
        <p>
          {t("versions.summary", { installed: installed.length, total: versions.length })}
        </p>
      </header>

      <div className="version-toolbar">
        <div className="filter-tabs">
          <button className={filter === "installed" ? "active" : ""} onClick={() => setFilter("installed")}>
            {t("versions.tabInstalled")}
          </button>
          <button className={filter === "all" ? "active" : ""} onClick={() => setFilter("all")}>
            {t("versions.tabAll")}
          </button>
          <button className={filter === "running" ? "active" : ""} onClick={() => setFilter("running")}>
            {t("versions.tabRunning")}
            {running.length > 0 && <span className="tab-count">{running.length}</span>}
          </button>
        </div>
        <div className="toolbar-actions">
          <button
            className="btn btn-ghost btn-inline"
            onClick={() => setBuilderOpen((v) => !v)}
            disabled={busyId !== null}
          >
            <Icon name="mods" size={14} />
            {t("versions.newInstance")}
          </button>
          <button
            className="btn btn-ghost btn-inline"
            onClick={() => {
              const firstId = instances[0]?.version_id ?? installed[0];
              if (!firstId) {
                setMessage(t("versions.noInstanceToEdit"));
                return;
              }
              openEdit(firstId);
            }}
            disabled={busyId !== null}
          >
            <Icon name="settings" size={14} />
            {t("versions.editInstance")}
          </button>
          <button className="btn-icon" onClick={load} title={t("common.refresh")}>
            <Icon name="refresh" size={18} />
          </button>
        </div>
      </div>

      {builderOpen && (
        <div className="instance-builder">
          <div className="instance-builder-grid">
            <div className="field">
              <label className="field-label">{t("versions.instanceName")}</label>
              <input
                className="settings-input"
                value={instName}
                onChange={(e) => setInstName(e.target.value)}
                placeholder={t("versions.defaultName", { v: instMc || t("common.new") })}
              />
            </div>

            <div className="field">
              <label className="field-label">{t("versions.gameVersion")}</label>
              <select
                className="settings-input"
                value={instMc}
                onChange={(e) => setInstMc(e.target.value)}
              >
                <option value="">{t("versions.selectVersion")}</option>
                {versions
                  .filter((v) => v.version_type === "release")
                  .map((v) => (
                    <option key={v.id} value={v.id}>
                      {v.id}
                      {v.is_latest_release ? t("versions.latest") : ""}
                    </option>
                  ))}
              </select>
            </div>

            <div className="field">
              <label className="field-label">
                <span className="label-with-icon">
                  <MinecraftIcon id={loaderSpriteId(instLoader)} size={15} />
                  {t("versions.loader")}
                </span>
              </label>
              <select
                className="settings-input"
                value={instLoader}
                onChange={(e) =>
                  setInstLoader(
                    e.target.value as "vanilla" | "fabric" | "forge" | "neoforge"
                  )
                }
              >
                <option value="vanilla">{t("versions.vanilla")}</option>
                <option value="fabric">Fabric</option>
                <option value="forge">Forge</option>
                <option value="neoforge">NeoForge</option>
              </select>
            </div>
          </div>

          <div className="instance-builder-grid" style={{ marginTop: 12 }}>
            <div className="field">
              <label className="field-label">{t("versions.icon")}</label>
              <div className="icon-pick-row">
                <div className="icon-preview icon-preview-sm">
                  <MinecraftIcon id={instIcon} loader={instLoader} size={26} />
                </div>
                <div className="icon-pick-text">
                  <div>{instIcon ? iconName(instIcon) : t("versions.iconAuto")}</div>
                  <div className="settings-hint">{t("versions.iconHint")}</div>
                </div>
                <button
                  type="button"
                  className="btn btn-ghost btn-inline"
                  onClick={() => setBuilderPickerOpen(true)}
                >
                  <Icon name="search" size={14} />
                  {t("versions.iconPick")}
                </button>
              </div>
            </div>
            <div className="field">
              <label className="field-label">{t("versions.tags")}</label>
              <input
                className="settings-input"
                value={instTags}
                onChange={(e) => setInstTags(e.target.value)}
                placeholder={t("versions.tagsPlaceholder")}
              />
            </div>
            <div className="field">
              <label className="field-label">{t("versions.desc")}</label>
              <input
                className="settings-input"
                value={instDesc}
                onChange={(e) => setInstDesc(e.target.value)}
                placeholder={t("versions.descPlaceholder")}
              />
            </div>
          </div>

          <div className="instance-options">
            <label className={`check-chip ${instLoader !== "fabric" ? "check-chip-off" : ""}`}>
              <input
                type="checkbox"
                checked={withFabricApi && instLoader === "fabric"}
                disabled={instLoader !== "fabric"}
                onChange={(e) => setWithFabricApi(e.target.checked)}
              />
              Fabric API
            </label>
            <label className="check-chip">
              <input
                type="checkbox"
                checked={withOptifine}
                onChange={(e) => setWithOptifine(e.target.checked)}
              />
              OptiFine
            </label>
            <span className="text-tertiary instance-tip">{t("versions.instanceTip")}</span>
            <button
              className="btn btn-primary btn-inline"
              onClick={handleCreateInstance}
              disabled={busyId !== null || !instMc}
            >
              {busyId === "create" ? t("versions.creating") : t("versions.create")}
            </button>
          </div>
        </div>
      )}

      {message && (
        <div key={message} className="message-bar">
          {message}
        </div>
      )}

      {/* 安装进度：实时显示百分比 */}
      {progress && progress.stage === "task_done" && progress.total > 0 && (
        <div className="inline-progress">
          <div className="inline-progress-bar">
            <span
              style={{
                width: `${Math.round((progress.completed / progress.total) * 100)}%`,
              }}
            />
          </div>
          <span className="inline-progress-text">
            {Math.round((progress.completed / progress.total) * 100)}% （
            {progress.completed}/{progress.total}）
          </span>
        </div>
      )}

      {/* 已安装版本：可直接启动 / 编辑 / 批量编辑（仅"已安装" tab） */}
      {filter === "installed" && installed.length > 0 && (
        <section className="installed-versions">
          <div className="installed-versions-head">
            <span className="installed-versions-title">
              {t("versions.installedList", { n: installed.length })}
            </span>
            {selectedInstalled.length > 0 && (
              <button
                className="btn btn-ghost btn-inline"
                onClick={() => {
                  setBatchIcon("");
                  setBatchDesc("");
                  setBatchTagsInput("");
                  setBatchOpen(true);
                }}
              >
                {t("versions.batchEdit", { n: selectedInstalled.length })}
              </button>
            )}
          </div>

          <div className="installed-versions-list">
            {installed.map((id) => {
              const meta = instances.find((ins) => ins.version_id === id);
              return (
                <div key={id} className="installed-version-row">
                  <input
                    type="checkbox"
                    checked={selectedInstalled.includes(id)}
                    onChange={() => toggleInstalled(id)}
                    title={t("versions.selectForBatch")}
                  />
                  <span className="iv-icon">
                    <MinecraftIcon
                      id={meta?.icon}
                      loader={meta?.loader}
                      versionId={id}
                      size={24}
                    />
                  </span>
                  <div className="iv-main">
                    <div className="iv-name-line">
                      <span className="iv-name">{meta?.name || id}</span>
                      <LoaderBadge loader={meta?.loader} versionId={id} />
                    </div>
                    <div className="iv-meta text-tertiary">
                      {meta
                        ? `${meta.minecraft_version} · ${meta.loader}${
                            meta.loader_version ? ` ${meta.loader_version}` : ""
                          }`
                        : id}
                      {meta?.tags?.length ? ` · ${meta.tags.join(" / ")}` : ""}
                    </div>
                  </div>
                  <button
                    className="btn btn-primary btn-inline btn-play-sm"
                    onClick={() => handleLaunch(id)}
                    disabled={launchingId !== null}
                  >
                    <Icon name="play" size={13} />
                    {launchingId === id ? t("versions.launching") : t("versions.launch")}
                  </button>
                  <button
                    className="btn btn-ghost btn-inline btn-play-sm"
                    onClick={() => openEdit(id)}
                    title={t("versions.editRowBtn")}
                  >
                    <Icon name="settings" size={14} />
                    {t("versions.editRowBtn")}
                  </button>
                </div>
              );
            })}
          </div>
        </section>
      )}

      {filter === "running" && (
        <section className="running-section">
          <div className="running-toolbar">
            <div className="running-stats">
              <span className="running-count">
                <strong>{running.length}</strong> {t("versions.runningSuffix")}
              </span>
              <span className="text-tertiary">
                {t("versions.totalInstances", { n: allInstances.length })}
              </span>
            </div>
            <div className="toolbar-actions">
              <button className="btn btn-ghost btn-inline" onClick={handleAddGameRoot}>
                <Icon name="folder" size={14} />
                {t("versions.addGameRoot")}
              </button>
              <button
                className="btn btn-primary btn-inline"
                onClick={handleBatchStart}
                disabled={runBusy || runSelected.length === 0}
              >
                <Icon name="play" size={13} />
                {t("versions.startSelected")}
              </button>
              <button
                className="btn btn-ghost btn-inline"
                onClick={handleBatchStop}
                disabled={runBusy || runSelected.length === 0}
              >
                {t("versions.stopSelected")}
              </button>
              <button
                className="btn btn-danger btn-inline"
                onClick={handleStopAll}
                disabled={runBusy || running.length === 0}
              >
                {t("versions.stopAll")}
              </button>
              <button
                className="btn-icon"
                onClick={() => {
                  refreshRunning();
                  refreshAllInstances();
                }}
                title={t("common.refresh")}
              >
                <Icon name="refresh" size={17} />
              </button>
            </div>
          </div>

          <div className="game-roots">
            {gameRoots.map((root) => (
              <div className="game-root-chip" key={root.path} title={root.path}>
                <Icon name="folder" size={12} />
                <span className="game-root-name">
                  {root.is_primary ? t("versions.primaryRoot") : rootLabel(root.path)}
                </span>
                <span className="game-root-count">{root.instance_count}</span>
                {!root.is_primary && (
                  <button
                    className="game-root-remove"
                    onClick={() => handleRemoveGameRoot(root.path)}
                    title={t("versions.removeGameRoot")}
                  >
                    <Icon name="close" size={11} />
                  </button>
                )}
              </div>
            ))}
          </div>

          <div className="running-block">
            <div className="running-block-title">
              {t("versions.runningList", { n: running.length })}
            </div>
            {running.length === 0 ? (
              <div className="empty-inline">{t("versions.noRunning")}</div>
            ) : (
              running.map((run) => (
                <div className="running-row" key={run.run_id}>
                  <span className="running-dot" />
                  <div className="running-main">
                    <div className="running-name">{run.version_id}</div>
                    <div className="running-meta text-tertiary">
                      PID {run.pid} · {rootLabel(run.game_root)}
                    </div>
                  </div>
                  <button
                    className="btn btn-danger btn-inline"
                    onClick={() => handleStopRun(run)}
                  >
                    {t("versions.stop")}
                  </button>
                </div>
              ))
            )}
          </div>

          <div className="running-block">
            <div className="running-block-title">
              {t("versions.allInstances", { n: allInstances.length })}
            </div>
            <div className="all-instance-list">
              {allInstances.map((instance) => {
                const activeRuns = runsFor(instance);
                const key = instanceKey(instance);
                return (
                  <div
                    className={`all-instance-row ${activeRuns.length > 0 ? "all-instance-running" : ""}`}
                    key={key}
                  >
                    <input
                      type="checkbox"
                      checked={runSelected.includes(key)}
                      onChange={() => toggleRunSelected(instance)}
                      title={t("versions.selectForBatch")}
                    />
                    <MinecraftIcon
                      id={instance.icon}
                      loader={instance.loader}
                      versionId={instance.version_id}
                      size={24}
                    />
                    <div className="iv-main">
                      <div className="iv-name-line">
                        <span className="iv-name">{instance.name || instance.version_id}</span>
                        <LoaderBadge
                          loader={instance.loader}
                          versionId={instance.version_id}
                        />
                      </div>
                      <div className="iv-meta text-tertiary">
                        {instance.minecraft_version} · {rootLabel(instance.game_root || "")}
                      </div>
                    </div>
                    <span
                      className={`status-tag ${activeRuns.length > 0 ? "status-running" : ""}`}
                    >
                      {activeRuns.length > 0
                        ? `${t("versions.instanceRunning")}${
                            activeRuns.length > 1 ? ` ×${activeRuns.length}` : ""
                          }`
                        : t("versions.notRunning")}
                    </span>
                    <div className="all-instance-actions">
                      <button
                        className="btn btn-primary btn-inline"
                        onClick={() => launchOne(instance)}
                        disabled={launchingId !== null}
                      >
                        <Icon name="play" size={13} />
                        {launchingId === key ? t("versions.launching") : t("versions.launch")}
                      </button>
                      {activeRuns.length > 0 && (
                        <button
                          className="btn btn-ghost btn-inline"
                          onClick={() => handleStopInstance(instance)}
                        >
                          {t("versions.stop")}
                        </button>
                      )}
                      <button
                        className="btn btn-ghost btn-inline"
                        onClick={() => openCustomLaunch(instance)}
                      >
                        <Icon name="settings" size={13} />
                        {t("versions.customLaunch")}
                      </button>
                    </div>
                  </div>
                );
              })}
              {allInstances.length === 0 && (
                <div className="empty-state">{t("versions.noInstances")}</div>
              )}
            </div>
          </div>
        </section>
      )}

      {filter === "all" && (
        <>
          <div className="version-list">
            {loading && <div className="empty-state">{t("versions.loading")}</div>}
            {!loading && shown.map((v) => {
              const isInstalled = installed.includes(v.id);
              const isConfirming = confirmId === v.id;
              const isVerifying = verifyingId === v.id;
              const inferredLoader = inferLoaderFromId(v.id);
              return (
                <div key={v.id} className="version-item">
                  <div className="version-row">
                    <div className="version-row-icon">
                      {inferredLoader && !isInstalled ? (
                        <MinecraftIcon id={loaderSpriteId("", v.id)} size={18} />
                      ) : (
                        <Icon name={isInstalled ? "check" : "versions"} size={18} />
                      )}
                    </div>
                    <div className="version-row-main">
                      <div className="version-row-title">
                        {v.id}
                        {inferredLoader && <LoaderBadge versionId={v.id} />}
                        {v.is_latest_release && <span className="meta-chip">{t("versions.latestChip")}</span>}
                      </div>
                      <div className="version-row-sub text-tertiary">
                        {v.version_type} · {v.release_time.slice(0, 10)}
                      </div>
                    </div>
                    <span className={`status-tag ${isInstalled ? "status-installed" : ""}`}>
                      {isVerifying
                        ? t("versions.verifyingTag")
                        : isInstalled
                          ? t("common.installed")
                          : t("common.notInstalled")}
                    </span>
                    <div className="version-row-actions">
                      <button
                        className="btn-icon"
                        onClick={() => handleInstall(v.id, installed.includes(v.id))}
                        disabled={busyId !== null || verifyingId !== null}
                        title={isInstalled ? t("versions.reinstallTitle") : t("versions.installTitle")}
                      >
                        <Icon name="download" size={16} />
                      </button>
                      {isInstalled && (
                        <button
                          className="btn-icon"
                          onClick={() => handleVerify(v.id)}
                          disabled={busyId !== null || verifyingId !== null}
                          title={t("versions.verifyTitle")}
                        >
                          <Icon name="shield" size={16} />
                        </button>
                      )}
                      {isInstalled && (
                        <button
                          className={`btn-icon ${isConfirming ? "btn-confirm" : ""}`}
                          onClick={() => handleUninstall(v.id)}
                          disabled={busyId !== null || verifyingId !== null}
                          title={isConfirming ? t("versions.uninstallConfirmTitle") : t("versions.uninstallTitle")}
                        >
                          {isConfirming ? <span className="confirm-text">{t("common.confirm")}</span> : <Icon name="trash" size={16} />}
                        </button>
                      )}
                    </div>
                  </div>

                  {reportId === v.id && report && (
                    <div className={`verify-report ${report.missing === 0 && report.corrupted === 0 ? "verify-ok" : "verify-bad"}`}>
                      <div className="verify-report-head">
                        <span className="verify-report-title">
                          <Icon name="shield" size={14} />
                          {t("versions.report")}
                        </span>
                        <button
                          className="btn-icon btn-icon-sm"
                          onClick={() => { setReportId(null); setReport(null); }}
                          title={t("versions.closeReport")}
                        >
                          <Icon name="close" size={13} />
                        </button>
                      </div>
                      <div className="verify-report-stats">
                        <span>{t("versions.reportTotal", { n: report.total })}</span>
                        <span className="stat-ok">{t("versions.reportValid", { n: report.valid })}</span>
                        <span className={report.missing ? "stat-bad" : ""}>{t("versions.reportMissing", { n: report.missing })}</span>
                        <span className={report.corrupted ? "stat-bad" : ""}>{t("versions.reportCorrupted", { n: report.corrupted })}</span>
                      </div>
                      {report.bytes_to_download > 0 && (
                        <div className="verify-report-hint">
                          {t("versions.needDownload", { size: formatBytes(report.bytes_to_download) })}
                          <button
                            className="btn btn-ghost btn-inline"
                            onClick={() => handleRepair(v.id)}
                            disabled={busyId !== null}
                          >
                            <Icon name="download" size={14} />
                            {t("versions.repairNow")}
                          </button>
                        </div>
                      )}
                      {(report.corrupted_paths.length > 0 || report.missing_paths.length > 0) && (
                        <div className="verify-report-paths">
                          {[
                            ...report.corrupted_paths.map((p) => t("versions.corruptPath", { path: p })),
                            ...report.missing_paths.map((p) => t("versions.missingPath", { path: p })),
                          ]
                            .slice(0, 5)
                            .map((line) => (
                              <div key={line} className="verify-path">{line}</div>
                            ))}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
            {!loading && shown.length === 0 && (
              <div className="empty-state">{t("versions.none")}</div>
            )}
          </div>
          {!loading && versions.length > 0 && (
            <div className="pager">
              <button
                className="btn btn-ghost btn-inline"
                onClick={() => setAllPage((p) => Math.max(0, p - 1))}
                disabled={safePage === 0}
              >
                {t("common.prevPage")}
              </button>
              <span className="pager-info">
                {t("common.pageOf", { page: safePage + 1, total: totalPages })}
              </span>
              <button
                className="btn btn-ghost btn-inline"
                onClick={() => setAllPage((p) => Math.min(totalPages - 1, p + 1))}
                disabled={safePage >= totalPages - 1}
              >
                {t("common.nextPage")}
              </button>
            </div>
          )}
        </>
      )}

      {/* 已安装 tab 的提示 */}
      {filter === "installed" && installed.length === 0 && (
        <div className="empty-state">{t("versions.emptyInstalled")}</div>
      )}

      <button className="btn btn-ghost btn-refresh-installed" onClick={onInstalled}>
        {t("versions.refreshInstalled")}
      </button>

      {customOpen && customTarget && (
        <div className="meta-dialog-backdrop" onClick={() => setCustomOpen(false)}>
          <div className="meta-dialog custom-launch-dialog" onClick={(e) => e.stopPropagation()}>
            <h3>
              {t("versions.customLaunchTitle", {
                name: customTarget.name || customTarget.version_id,
              })}
            </h3>
            <p className="settings-hint">{t("versions.customLaunchHint")}</p>
            <div className="meta-field">
              <label>{t("versions.launchMemory")}</label>
              <select
                className="settings-input"
                value={customMemory}
                onChange={(e) => setCustomMemory(Number(e.target.value))}
              >
                <option value={0}>{t("settings.memoryAuto")}</option>
                <option value={1024}>1 GB</option>
                <option value={2048}>2 GB</option>
                <option value={4096}>4 GB</option>
                <option value={8192}>8 GB</option>
                <option value={16384}>16 GB</option>
              </select>
            </div>
            <div className="meta-field">
              <label>{t("versions.launchJvm")}</label>
              <input
                className="settings-input"
                value={customJvm}
                onChange={(e) => setCustomJvm(e.target.value)}
                placeholder="-XX:+UseG1GC"
              />
            </div>
            <div className="meta-field">
              <label>{t("versions.launchGameArgs")}</label>
              <input
                className="settings-input"
                value={customGameArgs}
                onChange={(e) => setCustomGameArgs(e.target.value)}
                placeholder="--fullscreen"
              />
            </div>
            <div className="settings-actions">
              <button className="btn btn-ghost" onClick={() => setCustomOpen(false)}>
                {t("common.cancel")}
              </button>
              <button
                className="btn btn-primary"
                onClick={handleCustomLaunch}
                disabled={customSaving}
              >
                <Icon name="play" size={15} />
                {customSaving ? t("versions.launching") : t("versions.saveAndLaunch")}
              </button>
            </div>
          </div>
        </div>
      )}

      {editOpen && (
        <div className="meta-dialog-backdrop" onClick={() => setEditOpen(false)}>
          <div className="meta-dialog edit-dialog-clean" onClick={(e) => e.stopPropagation()}>
            <h3>{t("versions.editDialog")} <span className="edit-dialog-sub">{editName || editId}</span></h3>

            {/* 基本信息区 */}
            <div className="edit-section">
              <div className="edit-section-title">{t("versions.basicInfo")}</div>
              <div className="meta-field">
                <label>{t("versions.name")}</label>
                <input
                  className="settings-input"
                  value={editName}
                  onChange={(e) => setEditName(e.target.value)}
                />
              </div>
              <div className="meta-field">
                <label>{t("versions.icon")}</label>
                <div className="icon-pick-row">
                  <div className="icon-preview">
                    <MinecraftIcon
                      id={editIcon}
                      loader={editLoader}
                      versionId={editId}
                      size={30}
                    />
                  </div>
                  <div className="icon-pick-text">
                    <div>{editIcon ? iconName(editIcon) : t("versions.iconAuto")}</div>
                    <div className="settings-hint">{t("versions.iconHint")}</div>
                  </div>
                  <button
                    className="btn btn-ghost btn-inline"
                    onClick={() => setEditPickerOpen(true)}
                    title={t("versions.iconHint")}
                  >
                    <Icon name="search" size={14} />
                    {t("versions.iconPick")}
                  </button>
                </div>
              </div>
              <div className="meta-field">
                <label>{t("versions.tags")}</label>
                <input
                  className="settings-input"
                  value={editTags}
                  onChange={(e) => setEditTags(e.target.value)}
                  placeholder={t("versions.tagsPlaceholder")}
                />
              </div>
              <div className="meta-field">
                <label>{t("versions.desc")}</label>
                <textarea
                  className="settings-input"
                  rows={2}
                  value={editDesc}
                  onChange={(e) => setEditDesc(e.target.value)}
                  placeholder={t("versions.descPlaceholder")}
                />
              </div>
            </div>

            {/* 启动验证区 */}
            <div className="edit-section">
              <div className="edit-section-title">{t("settings.verify")}</div>
              <div className="edit-verify-grid">
                <div className="meta-field">
                  <label>{t("versions.verifyGameSelf")}</label>
                  <select className="settings-input" value={editVerify}
                    onChange={(e) => setEditVerify(e.target.value as "inherit" | "on" | "off")}>
                    <option value="inherit">{t("versions.inherit")}</option>
                    <option value="on">{t("versions.alwaysOn")}</option>
                    <option value="off">{t("versions.skip")}</option>
                  </select>
                </div>
                <div className="meta-field">
                  <label>{t("versions.verifyResSelf")}</label>
                  <select className="settings-input" value={editVerifyRes}
                    onChange={(e) => setEditVerifyRes(e.target.value as "inherit" | "on" | "off")}>
                    <option value="inherit">{t("versions.inherit")}</option>
                    <option value="on">{t("versions.alwaysOn")}</option>
                    <option value="off">{t("versions.skip")}</option>
                  </select>
                </div>
                <div className="meta-field">
                  <label>{t("versions.checkDepsSelf")}</label>
                  <select className="settings-input" value={editCheckDeps}
                    onChange={(e) => setEditCheckDeps(e.target.value as "inherit" | "on" | "off")}>
                    <option value="inherit">{t("versions.inherit")}</option>
                    <option value="on">{t("versions.alwaysCheck")}</option>
                    <option value="off">{t("versions.skipCheck")}</option>
                  </select>
                </div>
              </div>
            </div>

            {/* 实例资源区 */}
            <div className="edit-section">
              <div className="edit-section-title">{t("versions.instanceResources")}（{editResourceList.length}）</div>
              {editResourceList.length === 0 ? (
                <div className="text-tertiary">{t("versions.instanceResourcesEmpty")}</div>
              ) : (
                <div className="edit-resource-list">
                  {editResourceList.map((r) => (
                    <div key={`${r.kind}/${r.file_name}`} className="edit-resource-row">
                      <span className="installed-kind">{r.kind}</span>
                      <span className="edit-resource-name" title={r.file_name}>{r.file_name}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>

            <div className="meta-dialog-actions">
              <button className="btn btn-danger" onClick={handleDeleteInstance}>
                <Icon name="trash" size={15} />
                {t("versions.deleteInstance")}
              </button>
              <button className="btn btn-ghost" onClick={handleOpenFolder}>
                <Icon name="folder" size={15} />
                {t("versions.openFolder")}
              </button>
              <button className="btn btn-ghost" onClick={() => setEditOpen(false)}>
                {t("common.cancel")}
              </button>
              <button className="btn btn-primary" onClick={handleSaveMeta}>
                {t("common.save")}
              </button>
            </div>
          </div>
        </div>
      )}

      {batchOpen && (
        <div className="meta-dialog-backdrop" onClick={() => setBatchOpen(false)}>
          <div className="meta-dialog" onClick={(e) => e.stopPropagation()}>
            <h3>{t("versions.batchDialog", { n: selectedInstalled.length })}</h3>
            <p className="settings-hint" style={{ marginBottom: 12 }}>
              {t("versions.batchHint")}
            </p>

            <div className="meta-field">
              <label>{t("versions.icon")}</label>
              <div className="icon-pick-row">
                <div className="icon-preview icon-preview-sm">
                  <MinecraftIcon
                    id={batchIcon}
                    size={26}
                    title={batchIcon ? iconName(batchIcon) : t("versions.iconAuto")}
                  />
                </div>
                <div className="icon-pick-text">
                  <div>{batchIcon ? iconName(batchIcon) : t("versions.keepEmpty")}</div>
                  <div className="settings-hint">{t("versions.iconHint")}</div>
                </div>
                <button
                  className="btn btn-ghost btn-inline"
                  onClick={() => setBatchPickerOpen(true)}
                >
                  <Icon name="search" size={14} />
                  {t("versions.iconPick")}
                </button>
              </div>
            </div>

            <div className="meta-field">
              <label>{t("versions.tags")}</label>
              <input
                className="settings-input"
                value={batchTagsInput}
                onChange={(e) => setBatchTagsInput(e.target.value)}
                placeholder={t("versions.keepEmpty")}
              />
            </div>

            <div className="meta-field">
              <label>{t("versions.desc")}</label>
              <input
                className="settings-input"
                value={batchDesc}
                onChange={(e) => setBatchDesc(e.target.value)}
                placeholder={t("versions.keepEmpty")}
              />
            </div>

            <div className="meta-dialog-actions">
              <button className="btn btn-ghost" onClick={() => setBatchOpen(false)}>
                {t("common.cancel")}
              </button>
              <button className="btn btn-primary" onClick={applyBatchEdit}>
                {t("versions.applySelected")}
              </button>
            </div>
          </div>
        </div>
      )}

      {builderPickerOpen && (
        <ItemIconPicker
          value={instIcon}
          onPick={setInstIcon}
          onClose={() => setBuilderPickerOpen(false)}
        />
      )}
      {editPickerOpen && (
        <ItemIconPicker
          value={editIcon}
          onPick={setEditIcon}
          onClose={() => setEditPickerOpen(false)}
        />
      )}
      {batchPickerOpen && (
        <ItemIconPicker
          value={batchIcon}
          onPick={setBatchIcon}
          onClose={() => setBatchPickerOpen(false)}
        />
      )}
    </div>
  );
}
