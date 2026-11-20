/** 备份与还原面板：按内容勾选创建快照，并在还原前自动保护当前数据。 */

import { useCallback, useEffect, useState } from "react";
import { useConfirm } from "../components/ConfirmDialog";
import { api, type BackupOptions, type BackupSummary } from "../lib/api";
import { useT } from "../lib/i18n";

const DEFAULT_OPTIONS: BackupOptions = {
  settings: true,
  plugins: false,
  instances: false,
  accounts: true,
  logs: false,
  extraPaths: [],
};

function formatSize(bytes: number): string {
  if (bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / Math.pow(1024, i)).toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

export function BackupPanel() {
  const t = useT();
  const dialogs = useConfirm();
  const [options, setOptions] = useState<BackupOptions>(DEFAULT_OPTIONS);
  const [note, setNote] = useState("");
  const [extraText, setExtraText] = useState("");
  const [backups, setBackups] = useState<BackupSummary[]>([]);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(() => {
    api.listLauncherBackups()
      .then(setBackups)
      .catch((e) => setMessage(String(e)));
  }, []);

  useEffect(refresh, [refresh]);

  const buildOptions = (): BackupOptions => ({
    ...options,
    extraPaths: extraText.split(/\r?\n/).map((line) => line.trim()).filter(Boolean),
  });

  const handleCreate = async () => {
    setBusy(true);
    setMessage("");
    try {
      const summary = await api.createLauncherBackup(buildOptions(), note.trim());
      setMessage(`${t("backup.created")} · ${summary.id}`);
      setNote("");
      refresh();
    } catch (e) {
      setMessage(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleRestore = async (backup: BackupSummary) => {
    const ok = await dialogs.confirm(t("backup.restoreWarn"), {
      title: t("backup.restore"),
      confirmText: t("backup.restore"),
      danger: true,
    });
    if (!ok) return;
    setBusy(true);
    try {
      const result = await api.restoreLauncherBackup(backup.path, buildOptions());
      setMessage(t("backup.restored", { msg: result }));
    } catch (e) {
      setMessage(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleDelete = async (backup: BackupSummary) => {
    const ok = await dialogs.confirm(`${t("backup.delete")} · ${backup.id}？`, {
      title: t("backup.delete"),
      confirmText: t("common.delete"),
      danger: true,
    });
    if (!ok) return;
    try {
      await api.deleteLauncherBackup(backup.path);
      setMessage(t("backup.deleted"));
      refresh();
    } catch (e) {
      setMessage(String(e));
    }
  };

  const toggle = (key: keyof Omit<BackupOptions, "extraPaths">) =>
    setOptions((prev) => ({ ...prev, [key]: !prev[key] }));

  return (
    <div className="backup-panel">
      <p className="settings-hint">{t("backup.hint")}</p>
      <div className="backup-options">
        {(["settings", "plugins", "instances", "accounts", "logs"] as const).map((key) => (
          <label key={key} className="checkbox-row">
            <input type="checkbox" checked={options[key]} onChange={() => toggle(key)} />
            <span>{t(`backup.${key}`)}</span>
          </label>
        ))}
      </div>
      <div className="backup-field">
        <label>{t("backup.extraPaths")}</label>
        <textarea
          className="plugin-config-editor"
          rows={3}
          value={extraText}
          onChange={(e) => setExtraText(e.target.value)}
          placeholder={t("backup.extraHint")}
          spellCheck={false}
        />
      </div>
      <div className="backup-field backup-create-row">
        <input
          className="settings-input"
          value={note}
          onChange={(e) => setNote(e.target.value)}
          placeholder={t("backup.notePlaceholder")}
        />
        <button className="btn btn-primary btn-inline" onClick={handleCreate} disabled={busy}>
          {t("backup.create")}
        </button>
      </div>
      {message && <div className="message-bar">{message}</div>}

      <h3 className="section-title">{t("backup.title")}（{backups.length}）</h3>
      {backups.length === 0 ? (
        <div className="empty-state">{t("backup.empty")}</div>
      ) : (
        <div className="backup-list">
          {backups.map((backup) => (
            <div key={backup.id} className="backup-item">
              <div className="backup-item-main">
                <div className="backup-item-title">
                  {backup.note || backup.id}
                  <span className="plugin-item-ver">v{backup.appVersion}</span>
                </div>
                <div className="backup-item-meta text-tertiary">
                  {new Date(backup.createdAt * 1000).toLocaleString()} ·{" "}
                  {t("backup.items", {
                    n: backup.items.length,
                    size: formatSize(backup.size),
                  })}
                </div>
              </div>
              <div className="backup-item-actions">
                <button className="btn btn-ghost btn-inline" onClick={() => handleRestore(backup)} disabled={busy}>
                  {t("backup.restore")}
                </button>
                <button className="btn btn-danger btn-inline" onClick={() => handleDelete(backup)} disabled={busy}>
                  {t("backup.delete")}
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
