/**
 * 资源中心：搜索并安装 Mod / 光影 / 材质 / 数据包 / 整合包，支持本地与外链导入整合包。
 * 数据来自可配置的 Modrinth / CurseForge / MineBBS 兼容 API。
 */

import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Icon } from "../components/Icon";
import { useConfirm } from "../components/ConfirmDialog";
import { inferLoaderFromId } from "../lib/minecraftIcons";
import { useT } from "../lib/i18n";
import { LauncherPluginPanel } from "../components/LauncherPluginPanel";
import {
  api,
  sourceSupports,
  type DetectedResource,
  type InstanceInfo,
  type InstanceResource,
  type ResourceDetail,
  type ResourceHit,
  type ResourceSource,
  type ResourceTarget,
  type ResourceVersionInfo,
} from "../lib/api";

type ResourceType =
  | "mod"
  | "shader"
  | "resourcepack"
  | "datapack"
  | "modpack"
  | "plugin"
  | "launcherPlugin"
  | "skin";

const TABS: { key: ResourceType; labelKey: string }[] = [
  { key: "mod", labelKey: "resources.tab.mod" },
  { key: "shader", labelKey: "resources.tab.shader" },
  { key: "resourcepack", labelKey: "resources.tab.resourcepack" },
  { key: "datapack", labelKey: "resources.tab.datapack" },
  { key: "modpack", labelKey: "resources.tab.modpack" },
  { key: "plugin", labelKey: "resources.tab.serverPlugin" },
  { key: "launcherPlugin", labelKey: "resources.tab.launcherPlugin" },
  { key: "skin", labelKey: "resources.tab.skin" },
];

const PAGE_SIZE = 20;

function formatDownloads(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(0)}K`;
  return String(n);
}

/** 依据当前实例的加载器与游戏版本，给版本排序：匹配项优先 */
function rankVersions(
  versions: ResourceVersionInfo[],
  loader: string | null,
  gameVersion: string | null
): { matched: ResourceVersionInfo[]; others: ResourceVersionInfo[] } {
  const matched: ResourceVersionInfo[] = [];
  const others: ResourceVersionInfo[] = [];
  for (const v of versions) {
    const loaderOk = !loader || v.loaders.some((l) => l.toLowerCase() === loader.toLowerCase());
    const gameOk = !gameVersion || v.game_versions.includes(gameVersion);
    (loaderOk && gameOk ? matched : others).push(v);
  }
  return { matched, others };
}

export function ResourcesPage() {
  const t = useT();
  const dialogs = useConfirm();
  const [type, setType] = useState<ResourceType>("mod");
  const [pluginRepo, setPluginRepo] = useState("https://github.com/mscl-team/mscl-plugins");
  const [query, setQuery] = useState("");
  const [gameVersion, setGameVersion] = useState("");
  const [loader, setLoader] = useState("");
  const [hits, setHits] = useState<ResourceHit[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [hasSearched, setHasSearched] = useState(false);
  const [message, setMessage] = useState("");
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [packUrl, setPackUrl] = useState("");
  const [importing, setImporting] = useState(false);
  const [resources, setResources] = useState<InstanceResource[]>([]);
  const [moveTarget, setMoveTarget] = useState<Record<string, string>>({});
  const [sources, setSources] = useState<ResourceSource[]>([]);
  const [sourceId, setSourceId] = useState("");
  const [customDir, setCustomDir] = useState("");
  const [targets, setTargets] = useState<ResourceTarget[]>([]);
  const [targetId, setTargetId] = useState("");
  const [page, setPage] = useState(0);
  const [category, setCategory] = useState("");
  const [installedVersions, setInstalledVersions] = useState<string[]>([]);
  const [gameVersions, setGameVersions] = useState<string[]>([]);
  // 版本/详情弹窗：坐标定位 + 详情 + 版本分组
  const [versionModal, setVersionModal] = useState<{
    hit: ResourceHit;
    versions: ResourceVersionInfo[];
    loading: boolean;
    detail: ResourceDetail | null;
    pos: { left: number; top: number };
  } | null>(null);
  // 拖拽识别弹窗
  const [dropFile, setDropFile] = useState<DetectedResource | null>(null);
  const [dropTarget, setDropTarget] = useState("");
  const [dropDir, setDropDir] = useState("");
  const [dragOver, setDragOver] = useState(false);

  const refreshInstances = () =>
    api
      .listInstances()
      .then((list) => setInstances(list))
      .catch(() => {});

  useEffect(() => {
    refreshInstances();
    api.listResourceTargets()
      .then((list) => {
        setTargets(list);
        setTargetId((prev) => {
          if (prev && list.some((t) => t.id === prev)) return prev;
          return list.find((t) => t.kind === "instance")?.id ?? list[0]?.id ?? "";
        });
      })
      .catch(() => {});
    api.getSettings().then((s) => {
      if (s.plugin_repo_url) setPluginRepo(s.plugin_repo_url);
    }).catch(() => {});
    api.getInstalledVersions().then(setInstalledVersions).catch(() => {});
    api
      .getVersionList()
      .then((list) =>
        setGameVersions(
          list.versions
            .filter((v) => v.version_type === "release")
            .slice(0, 50)
            .map((v) => v.id)
        )
      )
      .catch(() => {});
    api
      .getSettings()
      .then((s) => {
        const list = s.resource_sources?.length ? s.resource_sources : [];
        setSources(list);
        setSourceId((prev) => prev || (list[0]?.id ?? ""));
      })
      .catch(() => {});
  }, []);

  const allowedTargetKinds: Record<ResourceType, string[]> = {
    mod: ["instance", "local"],
    shader: ["instance", "local"],
    resourcepack: ["instance", "local"],
    datapack: ["instance", "local"],
    modpack: ["instance", "local"],
    plugin: ["server", "multiplayer"],
    launcherPlugin: ["launcher", "local"],
    skin: ["instance", "local"],
  };

  /** 供实例资源列表使用的版本 ID；非实例目标统一标记为 __custom__ */
  const targetEntry = targets.find(
    (item) => item.id === targetId && allowedTargetKinds[type].includes(item.kind)
  ) ?? null;
  const target = targetEntry?.kind === "instance"
    ? targetEntry.id.replace(/^instance:/, "")
    : targetEntry
      ? "__custom__"
      : "";

  const refreshResources = () => {
    if (!target || target === "__custom__") {
      setResources([]);
      return;
    }
    api.listInstanceResources(target).then(setResources).catch(() => setResources([]));
  };

  useEffect(refreshResources, [target]);

  const availableSources = sources.filter((source) => sourceSupports(source, type));
  const currentSource = availableSources.find((s) => s.id === sourceId);
  const isLinkOnly =
    currentSource?.kind === "minebbs" || currentSource?.kind === "mcmod";

  useEffect(() => {
    if (availableSources.some((source) => source.id === sourceId)) return;
    const preferred =
      type === "plugin"
        ? availableSources.find((source) => source.kind === "spigot")
        : undefined;
    setSourceId((preferred ?? availableSources[0])?.id ?? "");
  }, [type, sources, sourceId]);

  // 当前目标实例及其加载器信息
  const targetMeta = instances.find((i) => i.version_id === target);
  // 优先用实例元数据；没有 instance.json 的直装版本（如 xxx-forge-xxx）从版本 ID 推断
  const targetLoader =
    (targetMeta?.loader?.toLowerCase() && targetMeta.loader.toLowerCase() !== "vanilla"
      ? targetMeta.loader.toLowerCase()
      : "") || inferLoaderFromId(target);
  const hasLoader = targetLoader !== "" && target !== "__custom__";
  const needsLoader = type === "mod" || type === "plugin";

  const kindOfType: Record<ResourceType, string> = {
    mod: "mods",
    shader: "shaderpacks",
    resourcepack: "resourcepacks",
    datapack: "datapacks",
    modpack: "mods",
    plugin: "plugins",
    launcherPlugin: "plugins",
    skin: "resourcepacks",
  };

  const availableTargets = targets.filter((item) =>
    allowedTargetKinds[type].includes(item.kind)
  );
  const currentTarget = availableTargets.find((item) => item.id === targetId) ?? null;

  useEffect(() => {
    if (availableTargets.some((item) => item.id === targetId)) return;
    setTargetId(availableTargets[0]?.id ?? "");
  }, [type, targets, targetId]);

  /** 统一解析安装目标：实例用 version_id，其他目录用 target_dir */
  const resolveTarget = () => {
    if (!currentTarget) return { instanceId: "", dir: customDir.trim(), kind: "local" };
    if (currentTarget.kind === "instance") {
      return {
        instanceId: currentTarget.id.replace(/^instance:/, ""),
        dir: "",
        kind: "instance",
      };
    }
    return { instanceId: "", dir: currentTarget.path || customDir.trim(), kind: currentTarget.kind };
  };

  const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE));

  const handleLocalImport = async () => {
    const { instanceId, dir } = resolveTarget();
    if (!instanceId && !dir) {
      setMessage(t("resources.selectFirst"));
      return;
    }
    if (type === "modpack") {
      setMessage(t("resources.packUseImport"));
      return;
    }
    const selected = await open({
      multiple: true,
      filters: [{ name: t("resources.resourceFiles"), extensions: ["jar", "zip"] }],
    });
    const files = Array.isArray(selected) ? selected : selected ? [selected] : [];
    if (files.length === 0) return;

    setImporting(true);
    let ok = 0;
    for (const file of files) {
      try {
        await api.importLocalResource({
          version_id: instanceId,
          kind: kindOfType[type],
          path: file,
          target_dir: dir || null,
        });
        ok += 1;
      } catch (e) {
        setMessage(t("resources.importFailed", { error: String(e) }));
      }
    }
    setImporting(false);
    if (ok > 0) {
      setMessage(t("resources.imported", { n: ok, dir: kindOfType[type] }));
    }
    refreshResources();
  };

  const handleDeleteResource = async (res: InstanceResource) => {
    const ok = await dialogs.confirm(
      t("resources.deleteConfirm", { name: res.file_name }),
      {
        title: t("resources.deleteResourceTitle"),
        confirmText: t("common.delete"),
        danger: true,
      }
    );
    if (!ok) return;
    try {
      await api.deleteInstanceResource(target, res.kind, res.file_name);
      setMessage(t("resources.deleted", { name: res.file_name }));
      refreshResources();
    } catch (e) {
      setMessage(t("resources.deleteFailed", { error: String(e) }));
    }
  };

  const handleMoveResource = async (res: InstanceResource) => {
    const key = `${res.kind}/${res.file_name}`;
    const to = moveTarget[key];
    if (!to) {
      setMessage(t("resources.moveToSelect"));
      return;
    }
    try {
      await api.moveInstanceResource({
        from_version: target,
        to_version: to,
        kind: res.kind,
        file_name: res.file_name,
      });
      setMessage(t("resources.moved", { name: res.file_name, target: to }));
      setMoveTarget((prev) => ({ ...prev, [key]: "" }));
      refreshResources();
    } catch (e) {
      setMessage(t("resources.moveFailed", { error: String(e) }));
    }
  };

  const formatSize = (bytes: number) => {
    if (bytes <= 0) return "";
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  };

  const search = async (
    nextType: ResourceType = type,
    keyword: string = query,
    pageOverride?: number,
    sourceOverride?: string
  ) => {
    const targetPage = pageOverride ?? page;
    const searchSourceId = sourceOverride ?? sourceId;
    if (!searchSourceId) {
      setLoading(false);
      setHasSearched(true);
      setHits([]);
      setTotal(0);
      setMessage(t("resources.noSourceHint"));
      return;
    }
    setLoading(true);
    setHasSearched(true);
    setMessage("");
    try {
      const result = await api.searchResources({
        query: keyword,
        project_type: nextType,
        game_version: gameVersion || null,
        loader:
          nextType === "mod" || nextType === "modpack" ? loader || null : null,
        source_id: searchSourceId,
        offset: targetPage * PAGE_SIZE,
        category: category || null,
      });
      setHits(result.hits);
      setTotal(result.total);
      if (result.hits.length === 0) {
        setMessage(t("resources.noResult"));
      }
    } catch (e) {
      setHits([]);
      setTotal(0);
      setMessage(t("resources.searchFailed", { error: String(e) }));
    } finally {
      setLoading(false);
    }
  };

  const switchType = (next: ResourceType) => {
    const candidates = sources.filter((source) => sourceSupports(source, next));
    const preferred =
      next === "plugin"
        ? candidates.find((source) => source.kind === "spigot")
        : undefined;
    const nextSourceId = (preferred ?? candidates[0])?.id ?? "";
    setType(next);
    setSourceId(nextSourceId);
    setHits([]);
    setTotal(0);
    setPage(0);
    setLoader("");
    setCategory("");
    void search(next, query, 0, nextSourceId);
  };

  /** 打开版本/详情弹窗，基于触发元素定位 */
  const openVersions = async (hit: ResourceHit, anchor?: HTMLElement | null) => {
    if (!currentTarget) {
      setMessage(t("resources.target.noTarget"));
      return;
    }
    // 计算弹窗位置：靠近按钮，clamp 到视口内
    const rect = anchor?.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const modalW = 560;
    const modalH = 520;
    const anchorX = rect ? rect.left : vw / 2 - modalW / 2;
    const anchorY = rect ? rect.bottom : vh / 2 - modalH / 2;
    const left = Math.max(12, Math.min(anchorX, vw - modalW - 12));
    const top = Math.max(12, Math.min(anchorY + 8, vh - modalH - 12));
    setVersionModal({ hit, versions: [], loading: true, detail: null, pos: { left, top } });
    // 并行拉取版本与详情
    void api
      .listResourceVersions(hit.project_id, type, sourceId || undefined)
      .then((list) =>
        setVersionModal((prev) => (prev ? { ...prev, versions: list, loading: false } : prev))
      )
      .catch((e) => {
        setVersionModal(null);
        setMessage(t("resources.versionLoadFailed", { error: String(e) }));
      });
    void api
      .getResourceDetail(hit.project_id, sourceId || undefined)
      .then((detail) =>
        setVersionModal((prev) => (prev ? { ...prev, detail } : prev))
      )
      .catch(() => {});
  };

  const install = async (hit: ResourceHit, versionId?: string) => {
    const { instanceId, dir, kind } = resolveTarget();
    if (!instanceId && !dir) {
      setMessage(t("resources.target.noTarget"));
      return;
    }
    setVersionModal(null);
    setBusyId(hit.project_id);
    setMessage("");
    try {
      const check = await api.checkResourceInstall({
        instance_version_id: instanceId,
        project_id: hit.project_id,
        project_type: type,
        source_id: sourceId || null,
        version_id: versionId ?? null,
        target_dir: dir || null,
        target_kind: kind,
      });

      if (check.already_installed) {
        const ok = await dialogs.confirm(
          t("resources.duplicateTitle", {
            title: hit.title,
            file: check.existing_file
              ? t("resources.duplicateFile", { file: check.existing_file })
              : "",
          }),
          {
            title: t("resources.duplicateConfirmTitle"),
            confirmText: t("resources.duplicateContinue"),
          }
        );
        if (!ok) {
          setMessage(t("resources.cancelled"));
          return;
        }
      } else if (check.missing_dependencies.length > 0) {
        const list = check.missing_dependencies.join("\n· ");
        const goInstall = await dialogs.confirm(
          t("resources.depsTitle", { title: hit.title, list }),
          {
            title: t("resources.depsDialogTitle"),
            confirmText: t("resources.installDepFirst"),
          }
        );
        if (goInstall) {
          setQuery(check.missing_dependencies[0]);
          setMessage(t("resources.foundDep", { dep: check.missing_dependencies[0] }));
          await search(type, check.missing_dependencies[0]);
          return;
        }
      }

      const result = await api.installResource({
        instance_version_id: instanceId,
        project_id: hit.project_id,
        project_type: type,
        source_id: sourceId || null,
        version_id: versionId ?? null,
        target_dir: dir || null,
        target_kind: kind,
      });
      setMessage(
        t("resources.installDone", {
          title: hit.title,
          version: result.version_id,
          failed: result.failed > 0 ? t("resources.installFailedPart", { n: result.failed }) : "",
        })
      );
      refreshInstances();
    } catch (e) {
      setMessage(t("resources.installFailed", { error: String(e) }));
    } finally {
      setBusyId(null);
    }
  };

  const importLocal = async () => {
    if (importing) return;
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "Modrinth Modpack", extensions: ["mrpack"] }],
      });
      if (!selected || typeof selected !== "string") return;
      setImporting(true);
      setMessage(t("resources.importingLocal"));
      const result = await api.importMrpack(selected, resolveTarget().dir || null);
      setMessage(
        t("resources.importDone", {
          name: result.pack_name,
          target: result.target_version,
          downloaded: result.downloaded,
          overrides: result.overrides,
        })
      );
      refreshInstances();
    } catch (e) {
      setMessage(t("resources.importFailed", { error: String(e) }));
    } finally {
      setImporting(false);
    }
  };

  const importRemote = async () => {
    if (importing || !packUrl.trim()) return;
    setImporting(true);
    setMessage(t("resources.importingRemote"));
    try {
      const result = await api.importMrpackUrl(packUrl.trim(), resolveTarget().dir || null);
      setMessage(
        t("resources.importDone", {
          name: result.pack_name,
          target: result.target_version,
          downloaded: result.downloaded,
          overrides: result.overrides,
        })
      );
      setPackUrl("");
      refreshInstances();
    } catch (e) {
      setMessage(t("resources.importFailed", { error: String(e) }));
    } finally {
      setImporting(false);
    }
  };

  // ===== 拖拽本地资源：自动识别类型并选择安装路径 =====
  const detectAndShow = async (path: string) => {
    try {
      const detected = await api.detectResourceFile(path);
      setDropFile(detected);
      setDropTarget(target && target !== "__custom__" ? target : "");
      setDropDir(customDir);
      setMessage(
        detected.project_type === "unknown"
          ? t("resources.dropUnknown")
          : t("resources.dropDetected", {
              type: t(`resources.dropType.${detected.project_type}`),
              name: detected.file_name,
            })
      );
    } catch (e) {
      setMessage(t("resources.importFailed", { error: String(e) }));
    }
  };

  const handleDroppedFiles = async (files: FileList | null) => {
    if (!files || files.length === 0) return;
    const file = files[0];
    // Tauri Webview 里 File 带 path；浏览器预览下退回文件名
    const path = (file as File & { path?: string }).path ?? file.name;
    await detectAndShow(path);
  };

  // 通过系统对话框选择文件后识别（Tauri 下能拿到真实路径）
  const pickAndDetectFile = async () => {
    const selected = await open({
      multiple: false,
      filters: [
        {
          name: t("resources.resourceFiles"),
          extensions: ["jar", "zip", "mrpack"],
        },
      ],
    });
    if (!selected || typeof selected !== "string") return;
    await detectAndShow(selected);
  };

  const confirmDropInstall = async () => {
    if (!dropFile) return;
    if (!dropTarget && !dropDir.trim()) {
      setMessage(t("resources.selectFirst"));
      return;
    }
    try {
      await api.importLocalResource({
        version_id: dropTarget && dropTarget !== "__custom__" ? dropTarget : "",
        kind: dropFile.kind,
        path: dropFile.path,
        target_dir: dropTarget === "__custom__" ? dropDir.trim() || null : null,
      });
      setMessage(t("resources.dropDone", { name: dropFile.file_name, target: dropTarget || dropDir }));
      setDropFile(null);
      refreshResources();
      refreshInstances();
    } catch (e) {
      setMessage(t("resources.importFailed", { error: String(e) }));
    }
  };

  // 版本弹窗内的分组
  const versionGroups = versionModal
    ? rankVersions(versionModal.versions, hasLoader ? targetLoader : null, gameVersion || null)
    : null;

  const renderVersionPick = (v: ResourceVersionInfo) => (
    <button
      key={v.id}
      className="version-pick"
      onClick={() => versionModal && install(versionModal.hit, v.id)}
    >
      <span className="version-pick-name">
        {v.version_number || v.name || v.id}
      </span>
      <span className="version-pick-meta text-tertiary">
        {v.game_versions.slice(0, 3).join(", ")}
        {v.loaders.length > 0 ? ` · ${v.loaders.join("/")}` : ""}
      </span>
      <span className="version-pick-date text-tertiary">
        {v.published.slice(0, 10)}
      </span>
    </button>
  );

  return (
    <div
      className="page resources-page"
      onDragOver={(e) => {
        e.preventDefault();
        setDragOver(true);
      }}
      onDragLeave={(e) => {
        if (e.currentTarget === e.target) setDragOver(false);
      }}
      onDrop={(e) => {
        e.preventDefault();
        setDragOver(false);
        void handleDroppedFiles(e.dataTransfer.files);
      }}
    >
      <header className="page-header">
        <h1>{t("resources.title")}</h1>
        <p>{t("resources.subtitle")}</p>
      </header>

      <div className="filter-tabs">
        {TABS.map((tab) => (
          <button
            key={tab.key}
            className={`filter-tab ${type === tab.key ? "filter-tab-active" : ""}`}
            onClick={() => switchType(tab.key)}
          >
            {t(tab.labelKey)}
          </button>
        ))}
      </div>

      {type === "launcherPlugin" ? (
        <LauncherPluginPanel settingsRepo={pluginRepo} />
      ) : (
      <>
      <div className="resource-search-main">
        <input
          className="settings-input"
          placeholder={isLinkOnly ? t("resources.searchMinebbs") : t("resources.searchPlaceholder")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void search();
          }}
        />
        <button className="btn btn-primary btn-inline" onClick={() => search()} disabled={loading}>
          <Icon name="search" size={15} />
          {loading ? t("common.searching") : t("common.search")}
        </button>
      </div>

      <div className="resource-filters">
        <select
          className="settings-input"
          value={gameVersion}
          onChange={(e) => {
            setGameVersion(e.target.value);
            setPage(0);
            void search(type, query, 0);
          }}
        >
          <option value="">{t("resources.allGameVersions")}</option>
          {[...new Set([...installedVersions, ...gameVersions])].map((v) => (
            <option key={v} value={v}>
              {v}
            </option>
          ))}
        </select>

        {(type === "mod" || type === "modpack") && (
          <select
            className="settings-input"
            value={loader}
            onChange={(e) => {
              setLoader(e.target.value);
              setPage(0);
              void search(type, query, 0);
            }}
          >
            <option value="">{t("resources.allLoaders")}</option>
            <option value="fabric">Fabric</option>
            <option value="forge">Forge</option>
            <option value="neoforge">NeoForge</option>
            <option value="quilt">Quilt</option>
          </select>
        )}

        {type === "shader" && (
          <select
            className="settings-input"
            value={loader}
            onChange={(e) => {
              setLoader(e.target.value);
              setPage(0);
              void search(type, query, 0);
            }}
          >
            <option value="">{t("resources.allShaderLoaders")}</option>
            <option value="iris">{t("resources.shaderIris")}</option>
            <option value="optifine">{t("resources.shaderOptifine")}</option>
            <option value="canvas">Canvas</option>
          </select>
        )}

        {type === "mod" && (
          <select
            className="settings-input"
            value={category}
            onChange={(e) => {
              setCategory(e.target.value);
              setPage(0);
              void search(type, query, 0);
            }}
          >
            <option value="">{t("resources.allModTypes")}</option>
            <option value="optimization">{t("resources.cat.optimization")}</option>
            <option value="decoration">{t("resources.cat.decoration")}</option>
            <option value="library">{t("resources.cat.library")}</option>
            <option value="adventure">{t("resources.cat.adventure")}</option>
            <option value="utility">{t("resources.cat.utility")}</option>
            <option value="worldgen">{t("resources.cat.worldgen")}</option>
          </select>
        )}
        <select
          className="settings-input"
          value={sourceId}
          onChange={(e) => setSourceId(e.target.value)}
          title={t("resources.sourceTitle")}
          disabled={availableSources.length === 0}
        >
          {availableSources.length === 0 ? (
            <option value="">{t("resources.noSourceForType")}</option>
          ) : (
            availableSources.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))
          )}
        </select>
        {availableSources.length === 0 && (
          <span className="source-empty-hint">{t("resources.noSourceHint")}</span>
        )}
        <button
          className="btn btn-ghost btn-inline"
          onClick={handleLocalImport}
          disabled={!currentTarget || importing || type === "modpack"}
          title={t("resources.localImportTitle")}
        >
          <Icon name="folder" size={14} />
          {t("resources.localImport")}
        </button>
      </div>

      <div className="resource-target">
        <Icon name="mods" size={15} />
        <span className="text-tertiary">{t("resources.installTo")}</span>
        <select
          className="version-select"
          value={targetId}
          onChange={(e) => setTargetId(e.target.value)}
        >
          <option value="">{t("resources.target.choose")}</option>
          {["instance", "local", "server", "multiplayer", "launcher"].map((kind) => {
            const group = availableTargets.filter((item) => item.kind === kind);
            if (group.length === 0) return null;
            return (
              <optgroup key={kind} label={t(`resources.target.${kind}`)}>
                {group.map((item) => (
                  <option key={item.id} value={item.id} title={item.path}>
                    {item.label}
                  </option>
                ))}
              </optgroup>
            );
          })}
        </select>

        {currentTarget?.kind !== "instance" && (
          <input
            className="settings-input"
            style={{ maxWidth: 280 }}
            value={customDir}
            onChange={(e) => setCustomDir(e.target.value)}
            placeholder={t("resources.externalDirPlaceholder")}
          />
        )}

        <button
          className="btn-icon"
          onClick={() => {
            refreshInstances();
            api.listResourceTargets().then(setTargets).catch(() => {});
          }}
          title={t("resources.refreshInstances")}
        >          <Icon name="refresh" size={16} />
        </button>
        <button
          className="btn btn-ghost btn-inline"
          onClick={() => void pickAndDetectFile()}
          title={t("resources.dropHint")}
        >
          <Icon name="folder" size={14} />
          {t("resources.autoDetect")}
        </button>
      </div>

      {/* 无加载器提示：Mod/插件不可安装 */}
      {needsLoader && target && target !== "__custom__" && !hasLoader && (
        <div className="message-bar message-warn">{t("resources.needLoader")}</div>
      )}

      {type === "modpack" && (
        <div className="pack-import">
          <div className="pack-import-row">
            <button
              className="btn btn-ghost btn-inline"
              onClick={importLocal}
              disabled={importing}
            >
              <Icon name="folder" size={14} />
              {t("resources.packLocal")}
            </button>
            <span className="text-tertiary">{t("resources.or")}</span>
            <input
              className="settings-input"
              placeholder={t("resources.packUrlPlaceholder")}
              value={packUrl}
              onChange={(e) => setPackUrl(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void importRemote();
              }}
            />
            <button
              className="btn btn-primary btn-inline"
              onClick={importRemote}
              disabled={importing || !packUrl.trim()}
            >
              {t("resources.importRemote")}
            </button>
          </div>
        </div>
      )}

      {message && (
        <div key={message} className="message-bar">
          {message}
        </div>
      )}

      <div className="resource-list">
        {loading && <div className="empty-state">{t("resources.searching")}</div>}
        {!loading && !hasSearched && (
          <div className="resource-empty-guide">
            <div className="guide-icon"><Icon name="search" size={32} /></div>
            <div className="guide-title">{t("resources.pickToInstall")}</div>
            <div className="guide-hint text-tertiary">{t("resources.pickHint")}</div>
          </div>
        )}
        {!loading && hasSearched &&
          hits.map((hit) => (
            <div key={hit.project_id} className="resource-row">
              <div className="resource-icon">
                {hit.icon_url ? (
                  <img src={hit.icon_url} alt="" loading="lazy" />
                ) : (
                  <Icon name="mods" size={18} />
                )}
              </div>
              <div className="resource-main">
                <div className="resource-title-row">
                  <span className="resource-title">{hit.title}</span>
                  <span className="resource-downloads text-tertiary">
                    ↓ {formatDownloads(hit.downloads)}
                  </span>
                </div>
                <div className="resource-desc text-tertiary">{hit.description}</div>
              </div>
              {isLinkOnly ? (
                <button
                  className="btn btn-ghost btn-inline"
                  onClick={() =>
                    api
                      .openExternalUrl(hit.slug)
                      .catch((e) => setMessage(t("resources.openLinkFailed", { error: String(e) })))
                  }
                  disabled={!hit.slug}
                  title={t("resources.openWebTitle")}
                >
                  {t("resources.openWeb")}
                </button>
              ) : (
                <button
                  className="btn btn-ghost btn-inline"
                  onClick={(e) => openVersions(hit, e.currentTarget)}
                  disabled={busyId !== null || !target || (needsLoader && !hasLoader)}
                  title={needsLoader && !hasLoader ? t("resources.needLoaderShort") : undefined}
                >
                  {busyId === hit.project_id ? t("resources.installing") : t("resources.chooseVersion")}
                </button>
              )}
            </div>
          ))}
      </div>

      {total > 0 && (
        <div className="pager">
          <button
            className="btn btn-ghost btn-inline"
            onClick={() => {
              const next = Math.max(0, page - 1);
              setPage(next);
              void search(type, query, next);
            }}
            disabled={page === 0 || loading}
          >
            {t("common.prevPage")}
          </button>
          <span className="pager-info">
            {t("resources.pageOfTotal", { page: page + 1, total: totalPages })}
          </span>
          <button
            className="btn btn-ghost btn-inline"
            onClick={() => {
              const next = page + 1;
              setPage(next);
              void search(type, query, next);
            }}
            disabled={page + 1 >= totalPages || loading}
          >
            {t("common.nextPage")}
          </button>
        </div>
      )}

      {target && target !== "__custom__" && (
        <section className="installed-resources">
          <div className="installed-head">
            <span className="installed-title">{t("resources.installedResources", { n: resources.length })}</span>
            <button className="btn-icon" onClick={refreshResources} title={t("common.refresh")}>
              <Icon name="refresh" size={15} />
            </button>
          </div>

          {resources.length === 0 ? (
            <div className="text-tertiary installed-empty">{t("resources.installedEmpty")}</div>
          ) : (
            <div className="installed-list">
              {resources.map((res) => {
                // 无加载器实例不展示 mod 类资源列表
                if (!hasLoader && res.kind === "mods") return null;
                const key = `${res.kind}/${res.file_name}`;
                return (
                  <div key={key} className="installed-row">
                    <span className="installed-kind">{res.kind}</span>
                    <span className="installed-name" title={res.file_name}>
                      {res.file_name}
                    </span>
                    <span className="text-tertiary installed-size">
                      {formatSize(res.size)}
                      {!res.enabled ? t("resources.disabled") : ""}
                    </span>
                    <select
                      className="version-select installed-move"
                      value={moveTarget[key] ?? ""}
                      onChange={(e) =>
                        setMoveTarget((prev) => ({ ...prev, [key]: e.target.value }))
                      }
                    >
                      <option value="">{t("resources.moveTo")}</option>
                      {instances
                        .filter((ins) => ins.version_id !== target)
                        .map((ins) => (
                          <option key={ins.version_id} value={ins.version_id}>
                            {ins.name}
                          </option>
                        ))}
                    </select>
                    <button
                      className="btn-icon"
                      onClick={() => handleMoveResource(res)}
                      title={t("resources.move")}
                    >
                      <Icon name="arrow" size={14} />
                    </button>
                    <button
                      className="btn-icon"
                      onClick={() => handleDeleteResource(res)}
                      title={t("common.delete")}
                    >
                      <Icon name="trash" size={14} />
                    </button>
                  </div>
                );
              })}
            </div>
          )}
        </section>
      )}

      {/* 版本/详情弹窗：跟随触发按钮定位 */}
      {versionModal && versionGroups && (
        <div className="meta-dialog-backdrop" onClick={() => setVersionModal(null)}>
          <div
            className="meta-dialog version-dialog version-dialog-float"
            style={{ left: versionModal.pos.left, top: versionModal.pos.top }}
            onClick={(e) => e.stopPropagation()}
          >
            <h3>{t("resources.versionDialog", { title: versionModal.hit.title })}</h3>

            {/* 详情区：描述 / 下载量 / 前置依赖 */}
            <div className="resource-detail">
              {versionModal.detail ? (
                <>
                  <div className="detail-downloads text-tertiary">
                    {t("resources.detailDownloads")}: {formatDownloads(versionModal.detail.downloads)}
                  </div>
                  <div className="detail-section-title">{t("resources.detailDeps")}</div>
                  {versionModal.detail.dependencies.length === 0 ? (
                    <div className="text-tertiary detail-deps-empty">{t("resources.detailNoDeps")}</div>
                  ) : (
                    <div className="detail-deps">
                      {versionModal.detail.dependencies.map((d, i) => (
                        <span key={`${d}-${i}`} className="detail-dep-chip">{d}</span>
                      ))}
                    </div>
                  )}
                  {versionModal.detail.description && (
                    <>
                      <div className="detail-section-title">{t("resources.detailDesc")}</div>
                      <div className="detail-desc">{versionModal.detail.description}</div>
                    </>
                  )}
                </>
              ) : (
                <div className="text-tertiary">{t("resources.detailLoading")}</div>
              )}
            </div>

            {/* 版本块：匹配优先，然后全部 */}
            {versionModal.loading ? (
              <div className="text-tertiary" style={{ padding: "12px 0" }}>
                {t("resources.versionLoading")}
              </div>
            ) : (
              <div className="version-groups">
                {versionGroups.matched.length > 0 && (
                  <div className="version-group">
                    <div className="version-group-title">{t("resources.matchVersions")}</div>
                    <div className="version-pick-list">
                      {versionGroups.matched.map(renderVersionPick)}
                    </div>
                  </div>
                )}
                <div className="version-group">
                  <div className="version-group-title">{t("resources.allVersions")}</div>
                  {versionGroups.others.length === 0 && versionGroups.matched.length === 0 ? (
                    <div className="text-tertiary" style={{ padding: "8px 0" }}>{t("resources.noVersion")}</div>
                  ) : (
                    <div className="version-pick-list">
                      {versionModal.versions.map(renderVersionPick)}
                    </div>
                  )}
                </div>
              </div>
            )}

            <div className="meta-dialog-actions">
              <button
                className="btn btn-ghost"
                onClick={() => install(versionModal.hit)}
                disabled={versionModal.loading}
              >
                {t("resources.installLatest")}
              </button>
              <button className="btn btn-ghost" onClick={() => setVersionModal(null)}>
                {t("common.cancel")}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 拖拽识别确认弹窗 */}
      {dropFile && (
        <div className="meta-dialog-backdrop" onClick={() => setDropFile(null)}>
          <div className="meta-dialog drop-dialog" onClick={(e) => e.stopPropagation()}>
            <h3>{t("resources.dropHint")}</h3>
            <div className="meta-field">
              <label>{t("resources.detailTitle")}</label>
              <div className="drop-detected">
                {t("resources.dropDetected", {
                  type: t(`resources.dropType.${dropFile.project_type}`),
                  name: dropFile.file_name,
                })}
              </div>
            </div>
            <div className="meta-field">
              <label>{t("resources.dropPickInstance")}</label>
              <select className="settings-input" value={dropTarget} onChange={(e) => setDropTarget(e.target.value)}>
                <option value="">—</option>
                {instances.map((ins) => (
                  <option key={ins.version_id} value={ins.version_id}>
                    {ins.name}（{ins.minecraft_version} / {ins.loader}）
                  </option>
                ))}
                <option value="__custom__">{t("resources.externalDir")}</option>
              </select>
            </div>
            {dropTarget === "__custom__" && (
              <div className="meta-field">
                <label>{t("resources.dropPickDir")}</label>
                <input
                  className="settings-input"
                  value={dropDir}
                  onChange={(e) => setDropDir(e.target.value)}
                  placeholder="D:\\MC\\myinstance"
                />
              </div>
            )}
            <div className="meta-dialog-actions">
              <button className="btn btn-ghost" onClick={() => setDropFile(null)}>
                {t("common.cancel")}
              </button>
              <button className="btn btn-primary" onClick={confirmDropInstall}>
                {t("resources.dropConfirm")}
              </button>
            </div>
          </div>
        </div>
      )}

      </>
      )}

      {dragOver && <div className="drop-overlay">{t("resources.dropHint")}</div>}
    </div>
  );
}
