import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Icon } from "../components/Icon";
import { SkinViewer3D, type SkinPartKey, type SkinPose } from "../components/SkinViewer3D";
import { useT } from "../lib/i18n";
import { api, type SkinAsset, type SkinProfile } from "../lib/api";

type SkinSource = "mojang" | "namemc";

const POSES: { key: SkinPose; labelKey: string }[] = [
  { key: "idle", labelKey: "skin.poseIdle" },
  { key: "wave", labelKey: "skin.poseWave" },
  { key: "run", labelKey: "skin.poseRun" },
  { key: "sit", labelKey: "skin.poseSit" },
  { key: "tpose", labelKey: "skin.poseTpose" },
];

interface EditorState {
  name: string;
  dataUrl: string;
}

export function SkinPage() {
  const t = useT();
  const [name, setName] = useState("Player");
  const [source, setSource] = useState<SkinSource>("mojang");
  const [profile, setProfile] = useState<SkinProfile | null>(null);
  const [localSkins, setLocalSkins] = useState<SkinAsset[]>([]);
  const [previewName, setPreviewName] = useState("");
  const [previewData, setPreviewData] = useState("");
  const [capePreview, setCapePreview] = useState("");
  const [localKind, setLocalKind] = useState<"skin" | "cape">("skin");
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState("");
  const [showCape, setShowCape] = useState(true);
  const [capeFailed, setCapeFailed] = useState(false);
  const [showOverlay, setShowOverlay] = useState(true);
  const [solid, setSolid] = useState(true);
  const [hiddenParts, setHiddenParts] = useState<SkinPartKey[]>([]);
  const [pose, setPose] = useState<SkinPose>("idle");
  const [slim, setSlim] = useState(false);
  const [autoRotate, setAutoRotate] = useState(false);
  const [editor, setEditor] = useState<EditorState | null>(null);

  const refreshLocal = () =>
    api.listLocalSkins().then(setLocalSkins).catch(() => setLocalSkins([]));

  useEffect(() => {
    refreshLocal();
  }, []);

  const fetchProfile = async () => {
    if (!name.trim()) {
      setMessage(t("skin.needName"));
      return;
    }
    setLoading(true);
    setMessage("");
    setCapeFailed(false);
    try {
      const result = await api.fetchSkinProfile(name.trim(), source);
      setProfile(result);
      setPreviewName(result.name);
      setPreviewData(result.skin_data_url || result.skin_url);
      setCapePreview(result.cape_data_url || "");
      setSlim(result.model === "slim");
    } catch (e) {
      setProfile(null);
      setMessage(String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleDownload = async () => {
    if (!profile) return;
    try {
      const asset = await api.downloadSkin(profile.skin_url, profile.name);
      setMessage(t("skin.downloaded", { name: asset.name }));
      refreshLocal();
    } catch (e) {
      setMessage(t("skin.downloadFailed", { error: String(e) }));
    }
  };

  const handleDownloadCape = async () => {
    if (!profile?.cape_url) return;
    try {
      const asset = profile.cape_data_url
        ? await api.saveCapeData(`${profile.name}-cape`, profile.cape_data_url)
        : await api.downloadCape(profile.cape_url, `${profile.name}-cape`);
      setMessage(t("skin.capeDownloaded", { name: asset.name }));
      setLocalKind("cape");
      refreshLocal();
    } catch (e) {
      setMessage(t("skin.downloadFailed", { error: String(e) }));
    }
  };

  const handleImport = async (kind: "skin" | "cape" = "skin") => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "PNG", extensions: ["png"] }],
        title: kind === "cape" ? t("skin.importCape") : t("skin.importSkin"),
      });
      if (typeof selected !== "string") return;
      const asset =
        kind === "cape"
          ? await api.importCapeFile(selected)
          : await api.importSkinFile(selected);
      setMessage(t("skin.imported", { name: asset.name }));
      setLocalKind(kind);
      refreshLocal();
      if (kind === "cape") {
        setCapePreview(asset.data_url);
        setShowCape(true);
      } else {
        setPreviewName(asset.name);
        setPreviewData(asset.data_url);
        setPose("idle");
      }
    } catch (e) {
      setMessage(t("skin.importFailed", { error: String(e) }));
    }
  };

  const selectLocal = (asset: SkinAsset) => {
    const kind = asset.kind === "cape" ? "cape" : "skin";
    if (kind === "cape") {
      setCapePreview(asset.data_url);
      setShowCape(true);
      setCapeFailed(false);
      setMessage(t("skin.capeApplied", { name: asset.name }));
      return;
    }
    setProfile(null);
    setPreviewName(asset.name);
    setPreviewData(asset.data_url);
    setPose("idle");
    setSlim(false);
    setCapeFailed(false);
    setMessage("");
  };

  const togglePart = (key: SkinPartKey) => {
    setHiddenParts((prev) =>
      prev.includes(key) ? prev.filter((item) => item !== key) : [...prev, key]
    );
  };

  const openEditor = (asset: SkinAsset) => {
    setEditor({ name: asset.name, dataUrl: asset.data_url });
  };

  const visibleLocal = localSkins.filter(
    (asset) => (asset.kind ?? "skin") === localKind
  );

  return (
    <div className="page skin-page">
      <header className="page-header">
        <h1>{t("skin.title")}</h1>
        <p>{t("skin.subtitle")}</p>
      </header>

      <div className="skin-search">
        <input
          className="settings-input"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void fetchProfile();
          }}
          placeholder={t("skin.namePlaceholder")}
        />
        <select
          className="settings-input skin-source-select"
          value={source}
          onChange={(e) => setSource(e.target.value as SkinSource)}
          title={t("skin.source")}
        >
          <option value="mojang">{t("skin.sourceMojang")}</option>
          <option value="namemc">{t("skin.sourceNamemc")}</option>
        </select>
        <button
          className="btn btn-primary btn-inline"
          onClick={fetchProfile}
          disabled={loading}
        >
          <Icon name="search" size={14} />
          {loading ? t("skin.querying") : t("skin.query")}
        </button>
      </div>

      {message && (
        <div key={message} className="message-bar">
          {message}
        </div>
      )}

      <div className="skin-workbench">
        <div className="skin-viewer-panel">
          {previewData ? (
            <SkinViewer3D
              dataUrl={previewData}
              capeDataUrl={capePreview}
              showCape={showCape}
              showOverlay={showOverlay}
              solid={solid}
              hiddenParts={hiddenParts}
              slim={slim}
              pose={pose}
              autoRotate={autoRotate}
            />
          ) : (
            <div className="skin-viewer-empty">{t("skin.empty")}</div>
          )}
          <div className="skin-pose-bar">
            {POSES.map((item) => (
              <button
                key={item.key}
                className={`pose-chip ${pose === item.key ? "pose-chip-active" : ""}`}
                onClick={() => setPose(item.key)}
              >
                {t(item.labelKey)}
              </button>
            ))}
          </div>
          <div className="skin-view-options">
            <label className="check-chip">
              <input
                type="checkbox"
                checked={showOverlay}
                onChange={(e) => setShowOverlay(e.target.checked)}
              />
              {t("skin.showOverlay")}
            </label>
            <label className="check-chip">
              <input
                type="checkbox"
                checked={solid}
                onChange={(e) => setSolid(e.target.checked)}
              />
              {t("skin.solid")}
            </label>
            <label className="check-chip">
              <input
                type="checkbox"
                checked={showCape}
                onChange={(e) => setShowCape(e.target.checked)}
              />
              {t("skin.showCape")}
            </label>
            <label className="check-chip">
              <input
                type="checkbox"
                checked={slim}
                onChange={(e) => setSlim(e.target.checked)}
              />
              {t("skin.slim")}
            </label>
            <label className="check-chip">
              <input
                type="checkbox"
                checked={autoRotate}
                onChange={(e) => setAutoRotate(e.target.checked)}
              />
              {t("skin.autoRotate")}
            </label>
            {capePreview && (
              <button
                className="btn btn-ghost btn-inline"
                onClick={() => {
                  setCapePreview("");
                  setShowCape(false);
                }}
              >
                {t("skin.clearCape")}
              </button>
            )}
            <div className="skin-part-row">
              <span className="text-tertiary">{t("skin.parts")}</span>
              {(
                [
                  ["head", "skin.partHead"],
                  ["body", "skin.partBody"],
                  ["arms", "skin.partArms"],
                  ["legs", "skin.partLegs"],
                ] as [SkinPartKey, string][]
              ).map(([key, labelKey]) => (
                <button
                  key={key}
                  className={`pose-chip ${hiddenParts.includes(key) ? "pose-chip-off" : ""}`}
                  onClick={() => togglePart(key)}
                  title={
                    hiddenParts.includes(key)
                      ? t("skin.partShow")
                      : t("skin.partHide")
                  }
                >
                  {t(labelKey)}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="skin-detail-panel">
          {profile ? (
            <>
              <div className="skin-name">{profile.name}</div>
              <div className="text-tertiary skin-uuid">{profile.uuid}</div>
              <div className="skin-detail-tags">
                <span className="meta-chip">{profile.source === "namemc" ? "NameMC" : "Mojang"}</span>
                <span className="meta-chip">{profile.model === "slim" ? t("skin.slim") : t("skin.classic")}</span>
              </div>
              <div className="skin-detail-actions">
                <button className="btn btn-primary btn-inline" onClick={handleDownload}>
                  <Icon name="download" size={14} />
                  {t("skin.download")}
                </button>
                {profile.cape_url && (
                  <button className="btn btn-ghost btn-inline" onClick={handleDownloadCape}>
                    <Icon name="download" size={14} />
                    {t("skin.downloadCape")}
                  </button>
                )}
                {profile.skin_data_url && (
                  <button
                    className="btn btn-ghost btn-inline"
                    onClick={() =>
                      setEditor({ name: profile.name, dataUrl: profile.skin_data_url ?? "" })
                    }
                  >
                    <Icon name="settings" size={14} />
                    {t("skin.edit")}
                  </button>
                )}
              </div>
            </>
          ) : (
            <div className="skin-detail-placeholder">
              <Icon name="users" size={28} />
              {previewName && <div className="skin-name">{previewName}</div>}
              <p>{t("skin.previewHint")}</p>
            </div>
          )}

          {(previewData || capePreview) && (
            <>
              <div className="skin-texture-panel">
                <div className="skin-texture-head">
                  <span>{t("skin.textureTitle")}</span>
                  <span className="text-tertiary">{previewData ? "64 × 64" : "64 × 32"}</span>
                </div>
                <div className="skin-texture-grid">
                  {previewData && (
                    <figure className="skin-texture-item">
                      <img src={previewData} alt={t("skin.skin")} />
                      <figcaption>{t("skin.skin")}</figcaption>
                    </figure>
                  )}
                  {showCape && capePreview && (
                    <figure className="skin-texture-item">
                      {capeFailed ? (
                        <div className="skin-cape-empty">{t("skin.noCape")}</div>
                      ) : (
                        <img
                          src={capePreview}
                          alt={t("skin.cape")}
                          onError={() => setCapeFailed(true)}
                        />
                      )}
                      <figcaption>{t("skin.cape")}</figcaption>
                    </figure>
                  )}
                </div>
              </div>
              <p className="settings-hint skin-usage-hint">{t("skin.usageHint")}</p>
            </>
          )}
        </div>
      </div>

      <section className="skin-local-section">
        <div className="skin-local-head">
          <div>
            <h2>{t("skin.localTitle")}</h2>
            <p className="settings-hint">{t("skin.localHint")}</p>
          </div>
          <div className="skin-local-head-actions">
            <div className="skin-local-tabs">
              <button
                className={localKind === "skin" ? "active" : ""}
                onClick={() => setLocalKind("skin")}
              >
                {t("skin.tabSkins")}
              </button>
              <button
                className={localKind === "cape" ? "active" : ""}
                onClick={() => setLocalKind("cape")}
              >
                {t("skin.tabCapes")}
              </button>
            </div>
            <button
              className="btn btn-ghost btn-inline"
              onClick={() => handleImport(localKind)}
            >
              <Icon name="folder" size={14} />
              {localKind === "cape" ? t("skin.importCape") : t("skin.importSkin")}
            </button>
            <button className="btn btn-ghost btn-inline" onClick={refreshLocal}>
              <Icon name="refresh" size={14} />
              {t("common.refresh")}
            </button>
          </div>
        </div>

        {visibleLocal.length === 0 ? (
          <div className="empty-inline">
            {localKind === "cape" ? t("skin.localCapeEmpty") : t("skin.localEmpty")}
          </div>
        ) : (
          <div className="skin-local-grid">
            {visibleLocal.map((asset) => (
              <div className="skin-local-card" key={asset.path}>
                <button
                  className="skin-local-preview"
                  onClick={() => selectLocal(asset)}
                  title={localKind === "cape" ? t("skin.useCape") : t("skin.preview3d")}
                >
                  <img src={asset.data_url} alt={asset.name} />
                </button>
                <div className="skin-local-info">
                  <div className="skin-local-name">{asset.name}</div>
                  <div className="skin-local-source text-tertiary">
                    {asset.source === "instance"
                      ? t("skin.sourceInstance")
                      : asset.source === "resourcepack"
                        ? t("skin.sourceResourcepack")
                        : asset.source === "import"
                          ? t("skin.sourceImport")
                          : t("skin.sourceDownload")}
                  </div>
                </div>
                <div className="skin-local-actions">
                  <button className="btn btn-ghost btn-inline" onClick={() => selectLocal(asset)}>
                    {localKind === "cape" ? t("skin.useCape") : t("skin.preview3d")}
                  </button>
                  {localKind === "skin" && (
                    <button className="btn btn-ghost btn-inline" onClick={() => openEditor(asset)}>
                      {t("skin.edit")}
                    </button>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </section>

      {editor && (
        <SkinEditor
          name={editor.name}
          dataUrl={editor.dataUrl}
          onClose={() => setEditor(null)}
          onSaved={(asset) => {
            setEditor(null);
            setMessage(t("skin.saved", { name: asset.name }));
            refreshLocal();
            setPreviewName(asset.name);
            setPreviewData(asset.data_url);
          }}
        />
      )}
    </div>
  );
}

function SkinEditor({
  name,
  dataUrl,
  onClose,
  onSaved,
}: {
  name: string;
  dataUrl: string;
  onClose: () => void;
  onSaved: (asset: SkinAsset) => void;
}) {
  const t = useT();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const drawingRef = useRef(false);
  const [color, setColor] = useState("#3fa9f5");
  const [erase, setErase] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("");

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const image = new Image();
    image.onload = () => {
      ctx.imageSmoothingEnabled = false;
      ctx.clearRect(0, 0, 64, 64);
      ctx.drawImage(image, 0, 0, 64, 64);
    };
    image.src = dataUrl;
  }, [dataUrl]);

  const paint = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const x = Math.floor(((event.clientX - rect.left) / rect.width) * 64);
    const y = Math.floor(((event.clientY - rect.top) / rect.height) * 64);
    if (x < 0 || y < 0 || x >= 64 || y >= 64) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.imageSmoothingEnabled = false;
    if (erase) {
      ctx.clearRect(x, y, 1, 1);
    } else {
      ctx.fillStyle = color;
      ctx.fillRect(x, y, 1, 1);
    }
  };

  const handleSave = async () => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    setSaving(true);
    setMessage("");
    try {
      const asset = await api.saveSkinData(name, canvas.toDataURL("image/png"));
      onSaved(asset);
    } catch (e) {
      setMessage(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="meta-dialog-backdrop" onClick={onClose}>
      <div className="meta-dialog skin-editor-dialog" onClick={(e) => e.stopPropagation()}>
        <h3>{t("skin.editorTitle", { name })}</h3>
        <p className="settings-hint">{t("skin.editorHint")}</p>
        <div className="skin-editor-tools">
          <input
            type="color"
            value={color}
            onChange={(e) => {
              setColor(e.target.value);
              setErase(false);
            }}
          />
          <label className="check-chip">
            <input
              type="checkbox"
              checked={erase}
              onChange={(e) => setErase(e.target.checked)}
            />
            {t("skin.eraser")}
          </label>
        </div>
        <div className="skin-editor-canvas-wrap">
          <canvas
            ref={canvasRef}
            width={64}
            height={64}
            className="skin-editor-canvas"
            onPointerDown={(e) => {
              drawingRef.current = true;
              e.currentTarget.setPointerCapture(e.pointerId);
              paint(e);
            }}
            onPointerMove={(e) => {
              if (drawingRef.current) paint(e);
            }}
            onPointerUp={() => {
              drawingRef.current = false;
            }}
            onPointerLeave={() => {
              drawingRef.current = false;
            }}
          />
        </div>
        {message && <div className="message-bar">{message}</div>}
        <div className="settings-actions">
          <button className="btn btn-ghost" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button className="btn btn-primary" onClick={handleSave} disabled={saving}>
            <Icon name="check" size={15} />
            {saving ? t("settings.saving") : t("skin.saveSkin")}
          </button>
        </div>
      </div>
    </div>
  );
}
