/**
 * 启动器插件管理面板（资源中心 → 启动器插件）
 *
 * 完整操作：启用/禁用、更新/取消更新、导入/导出、本地/GitHub 安装、卸载，
 * 以及按 configSchema 可视化编辑每个插件的 config.json。
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Icon } from "../components/Icon";
import { useConfirm } from "../components/ConfirmDialog";
import { useT } from "../lib/i18n";
import {
  api,
  type PluginEntry,
  type PluginManifest,
  type PluginRelease,
  type ResourceTarget,
} from "../lib/api";
import {
  BUILTIN_PLUGINS,
  PLUGIN_CATEGORY_LABELS,
  PLUGIN_LAYER_LABELS,
} from "../lib/plugins";

export function LauncherPluginPanel({ settingsRepo }: { settingsRepo: string }) {
  const t = useT();
  const dialogs = useConfirm();
  const [installed, setInstalled] = useState<PluginEntry[]>([]);
  const [enabledIds, setEnabledIds] = useState<string[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [message, setMessage] = useState("");
  const [repoInput, setRepoInput] = useState(settingsRepo);
  const [configTarget, setConfigTarget] = useState<PluginManifest | null>(null);
  const [configText, setConfigText] = useState("{}");
  const [configError, setConfigError] = useState("");
  const [pendingUpdates, setPendingUpdates] = useState<Record<string, PluginRelease | null>>({});
  const [installTargets, setInstallTargets] = useState<ResourceTarget[]>([]);
  const [installTarget, setInstallTarget] = useState("launcher:self");
  const [manualDir, setManualDir] = useState("");

  const resolveInstallDir = () =>
    installTarget === "__manual__"
      ? manualDir.trim() || null
      : installTargets.find((item) => item.id === installTarget)?.path ?? null;

  const refresh = useCallback(() => {
    setLoading(true);
    api.listInstalledPlugins()
      .then(setInstalled)
      .catch((e) => setMessage(String(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    refresh();
    api.getDefaultPluginConfig().then(setEnabledIds).catch(() => setEnabledIds(null));
    api.listResourceTargets()
      .then((list) => {
        const usable = list.filter((item) => item.kind === "launcher" || item.kind === "local");
        setInstallTargets(usable);
        if (usable.length && !usable.some((item) => item.id === installTarget)) {
          setInstallTarget(usable[0].id);
        }
      })
      .catch(() => {});
  }, [refresh]);

  useEffect(() => setRepoInput(settingsRepo), [settingsRepo]);

  const handleToggle = async (plugin: PluginManifest | PluginEntry) => {
    const isEntry = "path" in plugin;
    if (!isEntry) {
      // 内置插件未落盘：写入 config.json 的 enabled 列表后提示保存生效
      if (enabledIds === null) return;
      const next = plugin.enabled
        ? enabledIds.filter((id) => id !== plugin.id)
        : [...enabledIds, plugin.id];
      try {
        await api.writeDefaultPluginConfig(next);
        setEnabledIds(next);
        setMessage(t("plugins.toggleSaved"));
      } catch (e) {
        setMessage(String(e));
      }
      return;
    }
    try {
      await api.setPluginEnabled(plugin.id, !plugin.enabled);
      refresh();
    } catch (e) {
      setMessage(String(e));
    }
  };

  const handleUninstall = async (plugin: PluginEntry) => {
    const ok = await dialogs.confirm(t("plugins.uninstallWarn", { name: plugin.name }), {
      title: t("plugins.uninstall"), danger: true, confirmText: t("common.delete"),
    });
    if (!ok) return;
    try {
      await api.uninstallPlugin(plugin.id);
      setMessage(t("plugins.uninstalled", { name: plugin.name }));
      refresh();
    } catch (e) {
      setMessage(String(e));
    }
  };

  const handleLocalInstall = async () => {
    try {
      const selected = await open({ directory: true, multiple: false, title: t("plugins.pickDir") });
      if (typeof selected !== "string" || !selected) return;
      const raw = await api.getPluginManifestFromDir(selected).catch(() => null);
      if (raw && Object.keys(raw).length > 0) {
        const errors = await api.validatePluginManifest(raw).catch(() => [] as string[]);
        if (errors.length) {
          setMessage(errors.join("；"));
          return;
        }
      }
      const entry = await api.installLocalPlugin(selected, resolveInstallDir());
      setMessage(t("plugins.installed", { name: entry.name }));
      refresh();
    } catch (e) {
      setMessage(String(e));
    }
  };

  const handleGithubInstall = async () => {
    if (!repoInput.trim()) { setMessage(t("plugins.needRepo")); return; }
    try {
      const entry = await api.installGithubPlugin(repoInput.trim(), resolveInstallDir());
      setMessage(t("plugins.installed", { name: entry.name }));
      refresh();
    } catch (e) {
      setMessage(String(e));
    }
  };

  const handleInstallBuiltin = async (plugin: PluginManifest) => {
    try {
      await api.installBuiltinPlugin(plugin as unknown as Record<string, unknown>, resolveInstallDir());
      if (enabledIds && !enabledIds.includes(plugin.id)) {
        const next = [...enabledIds, plugin.id];
        await api.writeDefaultPluginConfig(next);
        setEnabledIds(next);
      }
      setMessage(t("plugins.installed", { name: plugin.name }));
      refresh();
    } catch (e) {
      setMessage(String(e));
    }
  };

  const handleCheckUpdate = async (plugin: PluginManifest | PluginEntry) => {
    try {
      const release = await api.checkPluginUpdate(plugin.id);
      if (!release) { setMessage(t("plugins.noUpdate")); return; }
      setPendingUpdates((prev) => ({ ...prev, [plugin.id]: release }));
      setMessage(t("plugins.updateFound", { version: release.tag }));
    } catch (e) {
      setMessage(String(e));
    }
  };

  const handleCancelUpdate = (plugin: PluginManifest | PluginEntry) => {
    setPendingUpdates((prev) => {
      const next = { ...prev };
      delete next[plugin.id];
      return next;
    });
    setMessage(t("plugins.updateCancelled"));
  };

  const handleExport = async () => {
    try {
      const payload = await api.exportPluginConfigs();
      const path = await save({
        defaultPath: "mscl-plugins-config.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) return;
      await api.writePluginExportFile(path, payload);
      setMessage(t("plugins.exported"));
    } catch (e) {
      setMessage(String(e));
    }
  };

  const handleImport = async () => {
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
        title: t("plugins.import"),
      });
      if (typeof path !== "string" || !path) return;
      const count = await api.importPluginConfigsFromFile(path);
      setMessage(t("plugins.imported", { n: count }));
      refresh();
    } catch (e) {
      setMessage(String(e));
    }
  };

  const openConfig = async (plugin: PluginManifest | PluginEntry) => {
    setConfigTarget(plugin);
    setConfigError("");
    try {
      const cfg = await api.getPluginConfig(plugin.id);
      // 合并 schema 默认值，让编辑器展示完整字段
      const merged: Record<string, unknown> = {};
      for (const f of plugin.configSchema ?? []) merged[f.key] = f.default;
      Object.assign(merged, cfg);
      setConfigText(JSON.stringify(merged, null, 2));
    } catch (e) {
      setConfigText("{}");
      setConfigError(String(e));
    }
  };

  const saveConfig = async () => {
    if (!configTarget) return;
    try {
      const parsed = JSON.parse(configText);
      await api.setPluginConfig(configTarget.id, parsed);
      setConfigTarget(null);
      setMessage(t("plugins.configSaved"));
    } catch (e) {
      setConfigError(String(e));
    }
  };

  const pluginById = useMemo(() => new Map(installed.map((p) => [p.id, p])), [installed]);
  const builtins: (PluginManifest | PluginEntry)[] = BUILTIN_PLUGINS
    .filter((b) => b.required || pluginById.has(b.id))
    .map((b) => pluginById.get(b.id) ?? b);
  const builtinIds = new Set(builtins.map((p) => p.id));
  const externals = installed.filter((p) => !p.builtin && !BUILTIN_PLUGINS.some((b) => b.id === p.id));
  const installable = BUILTIN_PLUGINS.filter(
    (p) => !p.required && !builtinIds.has(p.id)
  );

  const renderPlugin = (plugin: PluginManifest | PluginEntry, builtin: boolean) => {
    const isEntry = "path" in plugin;
    const release = pendingUpdates[plugin.id];
    const categoryLabel = PLUGIN_CATEGORY_LABELS[plugin.category] ?? plugin.category;
    const layer = plugin.layer ?? "feature";
    const layerLabel = PLUGIN_LAYER_LABELS[layer] ?? layer;
    return (
      <div className="plugin-item" key={plugin.id}>
        <div className="plugin-item-icon">{plugin.icon ?? "⊞"}</div>
        <div className="plugin-item-main">
          <div className="plugin-item-title">
            <span>{plugin.name}</span>
            <span className="plugin-item-ver">v{plugin.version}</span>
            <span className="plugin-item-tag">{categoryLabel}</span>
            <span className="plugin-item-tag">{layerLabel}</span>
            {!plugin.enabled && <span className="plugin-item-tag disabled">{t("plugins.disabled")}</span>}
            {release && <span className="plugin-item-tag update">{t("plugins.updateReady", { v: release.tag })}</span>}
          </div>
          <div className="plugin-item-desc">{plugin.description}</div>
          <div className="plugin-item-meta">
            {plugin.author || "MSCL"} · {plugin.id}
            {plugin.repo ? ` · ${plugin.repo}` : ""}
            {plugin.dependencies?.length ? ` · deps: ${plugin.dependencies.join(", ")}` : ""}
          </div>
        </div>
        <div className="plugin-item-actions">
          <button className="btn btn-ghost btn-inline" disabled={plugin.required} onClick={() => handleToggle(plugin)}>
            {plugin.enabled ? t("plugins.disable") : t("plugins.enable")}
          </button>
          <button className="btn btn-ghost btn-inline" onClick={() => openConfig(plugin)}>
            {t("plugins.editConfig")}
          </button>
          {release ? (
            <button className="btn btn-ghost btn-inline" onClick={() => handleCancelUpdate(plugin)}>
              {t("plugins.cancelUpdate")}
            </button>
          ) : (
            <button className="btn btn-ghost btn-inline" onClick={() => handleCheckUpdate(plugin)} disabled={!plugin.repo}>
              {t("plugins.checkUpdate")}
            </button>
          )}
          {!builtin && isEntry && (
            <button className="btn btn-danger btn-inline" onClick={() => handleUninstall(plugin as PluginEntry)}>
              {t("plugins.uninstall")}
            </button>
          )}
          {builtin && <span className="plugin-item-builtin">{t("plugins.builtin")}</span>}
        </div>
      </div>
    );
  };

  return (
    <div className="plugin-hub-panel">
      <div className="plugin-hub-toolbar">
        <div className="plugin-hub-install">
          <select
            className="settings-input"
            value={installTarget}
            onChange={(e) => setInstallTarget(e.target.value)}
            title={t("resources.installTo")}
          >
            {installTargets.map((item) => (
              <option key={item.id} value={item.id}>{item.label}</option>
            ))}
            <option value="__manual__">{t("resources.externalDir")}</option>
          </select>
          {installTarget === "__manual__" && (
            <input
              className="settings-input"
              value={manualDir}
              onChange={(e) => setManualDir(e.target.value)}
              placeholder={t("resources.externalDirPlaceholder")}
            />
          )}
          <input className="settings-input" placeholder={t("plugins.repoPlaceholder")} value={repoInput} onChange={(e) => setRepoInput(e.target.value)} />
          <button className="btn btn-primary btn-inline" onClick={handleGithubInstall}>
            <Icon name="download" size={15} />{t("plugins.installGithub")}
          </button>
          <button className="btn btn-ghost btn-inline" onClick={handleLocalInstall}>
            <Icon name="folder" size={15} />{t("plugins.installLocal")}
          </button>
          <button className="btn btn-ghost btn-inline" onClick={handleImport}>{t("plugins.import")}</button>
          <button className="btn btn-ghost btn-inline" onClick={handleExport}>{t("plugins.export")}</button>
        </div>
      </div>

      {message && <div className="plugin-hub-message">{message}</div>}

      <h2 className="section-title">{t("plugins.builtinSection")}（{builtins.length}）</h2>
      <div className="plugin-list">
        {loading ? t("common.loading") : builtins.map((p) => renderPlugin(p, true))}
      </div>

      {!loading && installable.length > 0 && (
        <>
          <h2 className="section-title" style={{ marginTop: 20 }}>{t("plugins.installableSection")}（{installable.length}）</h2>
          <div className="plugin-list">
            {installable.map((p) => (
              <div className="plugin-item" key={`install-${p.id}`}>
                <div className="plugin-item-icon">{p.icon ?? "⊞"}</div>
                <div className="plugin-item-main">
                  <div className="plugin-item-title">
                    <span>{p.name}</span>
                    <span className="plugin-item-ver">v{p.version}</span>
                    <span className="plugin-item-tag">{p.id}</span>
                  </div>
                  <div className="plugin-item-desc">{p.description}</div>
                </div>
                <div className="plugin-item-actions">
                  <button className="btn btn-ghost btn-inline" onClick={() => handleInstallBuiltin(p)}>
                    {t("plugins.installBuiltin")}
                  </button>
                </div>
              </div>
            ))}
          </div>
        </>
      )}

      {externals.length > 0 && (
        <>
          <h2 className="section-title" style={{ marginTop: 20 }}>{t("plugins.externalSection")}（{externals.length}）</h2>
          <div className="plugin-list">{externals.map((p) => renderPlugin(p, false))}</div>
        </>
      )}

      {configTarget && (
        <div className="plugin-config-backdrop" role="presentation">
          <div className="plugin-config-dialog" role="dialog" aria-modal="true" aria-label={t("plugins.editConfig")}>
            <div className="plugin-config-head">
              <div>
                <span className="update-dialog-badge">{configTarget.id}</span>
                <h3>{t("plugins.editConfig")} · {configTarget.name}</h3>
              </div>
              <button className="btn-icon" onClick={() => setConfigTarget(null)} title={t("common.close")}>
                <Icon name="close" size={15} />
              </button>
            </div>
            <div className="plugin-config-body">
              {(configTarget.configSchema ?? []).length > 0 && (
                <div className="plugin-config-schema">
                  {(configTarget.configSchema ?? []).map((f) => (
                    <div key={f.key}><code>{f.key}</code><span>{f.label} · {f.type}</span></div>
                  ))}
                </div>
              )}
              <textarea
                className="plugin-config-editor"
                value={configText}
                onChange={(e) => { setConfigText(e.target.value); setConfigError(""); }}
                spellCheck={false}
              />
              {configError && <div className="confirm-error">{configError}</div>}
            </div>
            <div className="plugin-config-actions">
              <button className="btn btn-ghost" onClick={() => setConfigTarget(null)}>{t("common.cancel")}</button>
              <button className="btn btn-primary" onClick={saveConfig}>{t("common.save")}</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
