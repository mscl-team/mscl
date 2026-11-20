/** 用户中心：多账户管理（离线 / 外置登录 / 正版模板） */
import { useEffect, useMemo, useRef, useState, type ChangeEvent } from "react";
import { Icon } from "../components/Icon";
import { UserAvatar } from "../components/UserAvatar";
import { useConfirm } from "../components/ConfirmDialog";
import { useT } from "../lib/i18n";
import { api, type AccountInfo } from "../lib/api";

type LoginTab = "offline" | "external" | "premium";

const PAGE_SIZE = 6;

interface UsersPageProps {
  /** 账户变化后通知外层刷新首页用户信息 */
  onAccountsChanged: () => void;
  /** 当前账户用户名同步回应用设置 */
  onUsernameChanged: (username: string) => void;
}

export function UsersPage({
  onAccountsChanged,
  onUsernameChanged,
}: UsersPageProps) {
  const t = useT();
  const dialogs = useConfirm();
  const avatarInputRef = useRef<HTMLInputElement>(null);

  const [accounts, setAccounts] = useState<AccountInfo[]>([]);
  const [tab, setTab] = useState<LoginTab>("offline");
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");

  const [editingId, setEditingId] = useState<string | null>(null);
  const [offlineName, setOfflineName] = useState("");
  const [offlineAvatar, setOfflineAvatar] = useState("");

  const [apiBase, setApiBase] = useState("https://littleskin.cn/api/yggdrasil");
  const [externalUser, setExternalUser] = useState("");
  const [externalPass, setExternalPass] = useState("");

  const load = () => {
    api.listAccounts()
      .then(setAccounts)
      .catch(() => setAccounts([]));
  };

  useEffect(load, []);

  const applyAccounts = (infos: AccountInfo[]) => {
    setAccounts(infos);
    const active = infos.find((account) => account.is_active);
    if (active) onUsernameChanged(active.username);
    onAccountsChanged();
    setPage(0);
  };

  const methodLabel = (kind: AccountInfo["kind"]) =>
    kind === "offline"
      ? t("users.methodOffline")
      : kind === "external"
        ? t("users.methodExternal")
        : t("users.methodPremium");

  const active = accounts.find((account) => account.is_active) ?? null;

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return accounts;
    return accounts.filter((account) =>
      [account.username, account.uuid, account.api_base ?? "", methodLabel(account.kind)]
        .some((value) => value.toLowerCase().includes(needle))
    );
  }, [accounts, query]);

  const totalPages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const safePage = Math.min(page, totalPages - 1);
  const shown = filtered.slice(safePage * PAGE_SIZE, (safePage + 1) * PAGE_SIZE);

  const resetOfflineForm = () => {
    setEditingId(null);
    setOfflineName("");
    setOfflineAvatar("");
  };

  const handleAvatarPick = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result !== "string") return;
      const img = new Image();
      img.onload = () => {
        const max = 96;
        const scale = Math.min(1, max / Math.max(img.width, img.height));
        const canvas = document.createElement("canvas");
        canvas.width = Math.max(1, Math.round(img.width * scale));
        canvas.height = Math.max(1, Math.round(img.height * scale));
        const ctx = canvas.getContext("2d");
        setOfflineAvatar(
          ctx ? (ctx.drawImage(img, 0, 0, canvas.width, canvas.height), canvas.toDataURL("image/png")) : (reader.result as string)
        );
      };
      img.onerror = () => setOfflineAvatar(reader.result as string);
      img.src = reader.result as string;
    };
    reader.readAsDataURL(file);
    event.target.value = "";
  };

  const handleOfflineSubmit = async () => {
    if (busy) return;
    if (!offlineName.trim()) {
      setMessage(t("users.needName"));
      return;
    }
    setBusy(true);
    setMessage("");
    try {
      await api.upsertOfflineAccount({
        id: editingId,
        username: offlineName.trim(),
        avatar_url: offlineAvatar || null,
      });
      applyAccounts(await api.listAccounts());
      setMessage(t(editingId ? "users.offlineUpdated" : "users.offlineCreated"));
      resetOfflineForm();
    } catch (e) {
      setMessage(t("users.saveFailed", { error: String(e) }));
    } finally {
      setBusy(false);
    }
  };

  const handleExternalLogin = async () => {
    if (busy) return;
    if (!apiBase.trim() || !externalUser.trim() || !externalPass) {
      setMessage(t("users.externalNeedFields"));
      return;
    }
    setBusy(true);
    setMessage("");
    try {
      await api.authenticateExternal({
        api_base: apiBase.trim(),
        username: externalUser.trim(),
        password: externalPass,
      });
      applyAccounts(await api.listAccounts());
      setExternalPass("");
      setMessage(t("users.externalDone"));
    } catch (e) {
      setMessage(t("users.externalFailed", { error: String(e) }));
    } finally {
      setBusy(false);
    }
  };

  const handleUse = async (account: AccountInfo) => {
    try {
      applyAccounts(await api.setActiveAccount(account.id));
      setMessage(t("users.switched", { name: account.username }));
    } catch (e) {
      setMessage(t("users.switchFailed", { error: String(e) }));
    }
  };

  const handleEdit = (account: AccountInfo) => {
    if (account.kind !== "offline") {
      setMessage(t("users.onlyOfflineEdit"));
      return;
    }
    setTab("offline");
    setEditingId(account.id);
    setOfflineName(account.username);
    setOfflineAvatar(account.avatar_url ?? "");
  };

  const handleDelete = async (account: AccountInfo) => {
    const ok = await dialogs.confirm(
      t("users.deleteConfirm", { name: account.username }),
      { title: t("users.deleteAccount"), confirmText: t("common.delete"), danger: true }
    );
    if (!ok) return;
    try {
      applyAccounts(await api.deleteAccount(account.id));
      if (editingId === account.id) resetOfflineForm();
      setMessage(t("users.deleted", { name: account.username }));
    } catch (e) {
      setMessage(t("users.deleteFailed", { error: String(e) }));
    }
  };

  return (
    <div className="page users-page">
      <header className="page-header">
        <h1>{t("users.title")}</h1>
        <p>{t("users.subtitle")}</p>
      </header>

      <section className="users-summary">
        {active ? (
          <>
            <UserAvatar
              src={active.avatar_url}
              name={active.username}
              size={52}
              title={t("users.currentAccount")}
            />
            <div className="users-summary-main">
              <div className="users-summary-name">{active.username}</div>
              <div className="users-summary-meta">
                <span className={`auth-badge auth-${active.kind}`}>{methodLabel(active.kind)}</span>
                <span className="text-tertiary">{active.uuid}</span>
              </div>
            </div>
          </>
        ) : (
          <>
            <UserAvatar name="Player" size={52} />
            <div className="users-summary-main">
              <div className="users-summary-name">{t("users.noAccount")}</div>
              <div className="settings-hint">{t("users.noAccountHint")}</div>
            </div>
          </>
        )}
        <span className="users-summary-count">
          {t("users.accountCount", { n: accounts.length })}
        </span>
      </section>

      <div className="users-tabs">
        {([
          ["offline", "users.methodOffline"],
          ["external", "users.methodExternal"],
          ["premium", "users.methodPremium"],
        ] as const).map(([key, label]) => (
          <button
            key={key}
            className={`chip ${tab === key ? "chip-active" : ""}`}
            onClick={() => setTab(key)}
          >
            {t(label)}
          </button>
        ))}
      </div>

      {tab === "offline" && (
        <section className="users-form-card">
          <h3>{editingId ? t("users.editOffline") : t("users.addOffline")}</h3>
          <p className="settings-hint">{t("users.offlineHint")}</p>
          <div className="users-avatar-row">
            <UserAvatar
              src={offlineAvatar}
              name={offlineName || "Player"}
              size={58}
              onClick={() => avatarInputRef.current?.click()}
              title={t("users.clickAvatarEdit")}
            />
            <div className="users-avatar-tip text-tertiary">{t("users.clickAvatarEdit")}</div>
            <input
              ref={avatarInputRef}
              type="file"
              accept="image/*"
              style={{ display: "none" }}
              onChange={handleAvatarPick}
            />
          </div>
          <div className="users-form-row">
            <label className="settings-label">{t("users.username")}</label>
            <input
              className="settings-input"
              value={offlineName}
              maxLength={16}
              onChange={(e) => setOfflineName(e.target.value)}
              placeholder="Player"
            />
          </div>
          <div className="users-form-actions">
            <button className="btn btn-primary btn-inline" onClick={handleOfflineSubmit} disabled={busy}>
              {t(editingId ? "users.save" : "users.add")}
            </button>
            {editingId && (
              <button className="btn btn-ghost btn-inline" onClick={resetOfflineForm}>
                {t("common.cancel")}
              </button>
            )}
          </div>
        </section>
      )}

      {tab === "external" && (
        <section className="users-form-card">
          <h3>{t("users.addExternal")}</h3>
          <p className="settings-hint">{t("users.externalHint")}</p>
          <div className="users-form-row">
            <label className="settings-label">{t("users.authServer")}</label>
            <input
              className="settings-input"
              value={apiBase}
              onChange={(e) => setApiBase(e.target.value)}
              placeholder="https://littleskin.cn/api/yggdrasil"
            />
          </div>
          <div className="users-form-grid">
            <div className="users-form-row">
              <label className="settings-label">{t("users.account")}</label>
              <input
                className="settings-input"
                value={externalUser}
                onChange={(e) => setExternalUser(e.target.value)}
                placeholder={t("users.accountPlaceholder")}
              />
            </div>
            <div className="users-form-row">
              <label className="settings-label">{t("users.password")}</label>
              <input
                className="settings-input"
                type="password"
                value={externalPass}
                onChange={(e) => setExternalPass(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") void handleExternalLogin(); }}
                placeholder="••••••••"
              />
            </div>
          </div>
          <div className="users-form-actions">
            <button className="btn btn-primary btn-inline" onClick={handleExternalLogin} disabled={busy}>
              <Icon name="arrow" size={14} />
              {busy ? t("users.loggingIn") : t("users.login")}
            </button>
          </div>
        </section>
      )}

      {tab === "premium" && (
        <section className="users-form-card users-premium">
          <Icon name="shield" size={30} />
          <h3>{t("users.premiumTitle")}</h3>
          <p className="settings-hint">{t("users.premiumHint")}</p>
          <button className="btn btn-primary btn-inline" disabled>
            {t("users.premiumLogin")}
          </button>
          <p className="settings-hint">{t("users.premiumTemplate")}</p>
        </section>
      )}

      <section className="users-list-section">
        <div className="users-list-head">
          <h3>{t("users.accountList", { n: accounts.length })}</h3>
          <div className="users-search">
            <Icon name="search" size={14} />
            <input
              className="settings-input"
              value={query}
              onChange={(e) => { setQuery(e.target.value); setPage(0); }}
              placeholder={t("users.searchPlaceholder")}
            />
          </div>
        </div>

        <div className="account-grid">
          {shown.map((account) => (
            <div key={account.id} className={`account-card ${account.is_active ? "account-card-active" : ""}`}>
              <UserAvatar
                src={account.avatar_url}
                name={account.username}
                size={44}
                onClick={account.kind === "offline" ? () => handleEdit(account) : undefined}
                title={account.kind === "offline" ? t("users.clickAvatarEdit") : account.username}
              />
              <div className="account-main">
                <div className="account-name-row">
                  <strong>{account.username}</strong>
                  {account.is_active && <span className="meta-chip">{t("users.current")}</span>}
                </div>
                <div className="account-meta text-tertiary">
                  <span className={`auth-badge auth-${account.kind}`}>{methodLabel(account.kind)}</span>
                  <span title={account.uuid}>{account.uuid}</span>
                </div>
              </div>
              <div className="account-actions">
                {!account.is_active && (
                  <button className="btn btn-ghost btn-inline" onClick={() => handleUse(account)}>
                    {t("users.use")}
                  </button>
                )}
                {account.kind === "offline" && (
                  <button className="btn btn-ghost btn-inline" onClick={() => handleEdit(account)}>
                    {t("users.edit")}
                  </button>
                )}
                <button className="btn btn-danger btn-inline" onClick={() => handleDelete(account)}>
                  {t("common.delete")}
                </button>
              </div>
            </div>
          ))}
          {shown.length === 0 && <div className="empty-state">{t("users.noMatch")}</div>}
        </div>

        {filtered.length > 0 && (
          <div className="pager">
            <button
              className="btn btn-ghost btn-inline"
              onClick={() => setPage((p) => Math.max(0, p - 1))}
              disabled={safePage === 0}
            >
              {t("common.prevPage")}
            </button>
            <span className="pager-info">
              {t("common.pageOf", { page: safePage + 1, total: totalPages })}
            </span>
            <button
              className="btn btn-ghost btn-inline"
              onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
              disabled={safePage >= totalPages - 1}
            >
              {t("common.nextPage")}
            </button>
          </div>
        )}
      </section>

      {message && <div className="message-bar">{message}</div>}
    </div>
  );
}
