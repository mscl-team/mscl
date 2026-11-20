/**
 * Mod 管理页：按已安装版本查看 / 启用 / 禁用 / 删除 Mod
 * 隔离模式读取 instances/{版本}/mods，否则读取共享 mods 目录
 */

import { useEffect, useState } from "react";
import { Icon } from "../components/Icon";
import { api, type ModInfo } from "../lib/api";

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / Math.pow(1024, i);
  return `${value.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

export function ModsPage() {
  const [versions, setVersions] = useState<string[]>([]);
  const [selected, setSelected] = useState("");
  const [mods, setMods] = useState<ModInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyFile, setBusyFile] = useState<string | null>(null);
  const [confirmFile, setConfirmFile] = useState<string | null>(null);
  const [message, setMessage] = useState("");

  useEffect(() => {
    api.getInstalledVersions()
      .then((list) => {
        setVersions(list);
        if (list.length > 0) setSelected(list[0]);
        else setLoading(false);
      })
      .catch(() => setLoading(false));
  }, []);

  useEffect(() => {
    if (!selected) {
      setMods([]);
      return;
    }
    setLoading(true);
    api.listMods(selected)
      .then(setMods)
      .catch(() => setMods([]))
      .finally(() => setLoading(false));
  }, [selected]);

  const refresh = () => {
    if (!selected) return;
    api.listMods(selected).then(setMods).catch(() => {});
  };

  const handleToggle = async (mod: ModInfo) => {
    if (busyFile) return;
    setBusyFile(mod.file_name);
    setMessage("");
    try {
      await api.toggleMod(selected, mod.file_name, !mod.enabled);
      refresh();
      setMessage(`${mod.file_name} 已${mod.enabled ? "禁用" : "启用"}`);
    } catch (e) {
      setMessage(`操作失败：${String(e)}`);
    } finally {
      setBusyFile(null);
    }
  };

  const handleDelete = async (mod: ModInfo) => {
    if (busyFile) return;
    if (confirmFile !== mod.file_name) {
      setConfirmFile(mod.file_name);
      return;
    }
    setConfirmFile(null);
    setBusyFile(mod.file_name);
    try {
      await api.deleteMod(selected, mod.file_name);
      refresh();
      setMessage(`已删除 ${mod.file_name}`);
    } catch (e) {
      setMessage(`删除失败：${String(e)}`);
    } finally {
      setBusyFile(null);
    }
  };

  const handleOpenDir = async () => {
    try {
      await api.openInstanceDir(selected);
      setMessage("已在文件管理器中打开 Mod 所在目录");
    } catch (e) {
      setMessage(`打开目录失败：${String(e)}`);
    }
  };

  return (
    <div className="page mods-page">
      <header className="page-header">
        <h1>Mod 管理</h1>
        <p>为每个版本启用或禁用 Mod（放入 mods 目录的 .jar）</p>
      </header>

      {versions.length === 0 ? (
        <div className="empty-state">还没有已安装的版本，请先到「版本」页安装</div>
      ) : (
        <>
          <div className="version-toolbar">
            <select
              className="settings-input"
              value={selected}
              onChange={(e) => setSelected(e.target.value)}
              title="选择版本"
            >
              {versions.map((v) => (
                <option key={v} value={v}>
                  {v}
                </option>
              ))}
            </select>
            <div className="toolbar-actions">
              <button className="btn-icon" onClick={refresh} title="刷新">
                <Icon name="refresh" size={18} />
              </button>
              <button className="btn btn-ghost btn-inline" onClick={handleOpenDir}>
                <Icon name="folder" size={15} />
                打开 Mod 目录
              </button>
            </div>
          </div>

          {message && (
            <div key={message} className="message-bar">
              {message}
            </div>
          )}

          <div className="mod-list">
            {loading && <div className="empty-state">正在读取 Mod…</div>}
            {!loading && mods.length === 0 && (
              <div className="empty-state">
                还没有 Mod。点击「打开 Mod 目录」，把 .jar 文件放进去即可。
              </div>
            )}
            {!loading &&
              mods.map((m) => (
                <div key={m.file_name} className={`mod-row ${m.enabled ? "" : "mod-disabled"}`}>
                  <div className="mod-icon">
                    <Icon name="mods" size={16} />
                  </div>
                  <div className="mod-main">
                    <div className="mod-name">{m.file_name}</div>
                    <div className="mod-sub text-tertiary">{formatBytes(m.size)}</div>
                  </div>
                  <span className={`status-tag ${m.enabled ? "status-installed" : ""}`}>
                    {m.enabled ? "已启用" : "已禁用"}
                  </span>
                  <label className="switch" title={m.enabled ? "禁用" : "启用"}>
                    <input
                      type="checkbox"
                      checked={m.enabled}
                      disabled={busyFile !== null}
                      onChange={() => handleToggle(m)}
                    />
                    <span className="switch-slider" />
                  </label>
                  <button
                    className={`btn-icon ${confirmFile === m.file_name ? "btn-confirm" : ""}`}
                    onClick={() => handleDelete(m)}
                    disabled={busyFile !== null}
                    title={confirmFile === m.file_name ? "再次点击确认删除" : "删除 Mod"}
                  >
                    {confirmFile === m.file_name ? (
                      <span className="confirm-text">确认</span>
                    ) : (
                      <Icon name="trash" size={16} />
                    )}
                  </button>
                </div>
              ))}
          </div>
        </>
      )}
    </div>
  );
}
