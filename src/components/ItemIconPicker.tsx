/** Minecraft 方块 / 物品 / 加载器图标选择器（纯本地资源，无网络请求） */
import { useContext, useEffect, useMemo, useRef, useState, type ChangeEvent } from "react";
import { Icon } from "./Icon";
import { MinecraftIcon } from "./MinecraftIcon";
import {
  isCustomImageIcon,
  SPRITES,
  spriteLabel,
  type SpriteCategory,
} from "../lib/minecraftIcons";
import { LangContext, useT } from "../lib/i18n";

interface ItemIconPickerProps {
  value: string;
  onPick: (id: string) => void;
  onClose: () => void;
}

type Filter = "all" | SpriteCategory;

export function ItemIconPicker({ value, onPick, onClose }: ItemIconPickerProps) {
  const t = useT();
  const lang = useContext(LangContext);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const fileInputRef = useRef<HTMLInputElement>(null);

  const iconName = (id: string) =>
    isCustomImageIcon(id) ? t("versions.iconCustomImage") : spriteLabel(id, lang);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const list = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return SPRITES.filter((sprite) => {
      if (filter !== "all" && sprite.category !== filter) return false;
      if (!needle) return true;
      return (
        sprite.id.includes(needle) ||
        sprite.zh.toLowerCase().includes(needle) ||
        sprite.en.toLowerCase().includes(needle)
      );
    });
  }, [query, filter]);

  const filters: { key: Filter; label: string }[] = [
    { key: "all", label: t("versions.iconFilterAll") },
    { key: "block", label: t("versions.iconFilterBlock") },
    { key: "item", label: t("versions.iconFilterItem") },
    { key: "loader", label: t("versions.iconFilterLoader") },
  ];

  /** 本地图片：缩放到 128px 内再存，避免 instance.json 过大 */
  const handleLocalImage = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result !== "string") return;
      const img = new Image();
      img.onload = () => {
        const max = 128;
        const scale = Math.min(1, max / Math.max(img.width, img.height));
        const canvas = document.createElement("canvas");
        canvas.width = Math.max(1, Math.round(img.width * scale));
        canvas.height = Math.max(1, Math.round(img.height * scale));
        const ctx = canvas.getContext("2d");
        if (!ctx) {
          onPick(reader.result as string);
          onClose();
          return;
        }
        ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
        onPick(canvas.toDataURL("image/png"));
        onClose();
      };
      img.onerror = () => {
        onPick(reader.result as string);
        onClose();
      };
      img.src = reader.result as string;
    };
    reader.readAsDataURL(file);
    event.target.value = "";
  };

  return (
    <div className="item-icon-picker-backdrop" onMouseDown={onClose}>
      <div
        className="item-icon-picker"
        role="dialog"
        aria-modal="true"
        aria-label={t("versions.iconPickerTitle")}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="item-icon-picker-head">
          <div>
            <h3>{t("versions.iconPickerTitle")}</h3>
            <p className="settings-hint">{t("versions.iconPickerHint")}</p>
          </div>
          <button className="btn-icon" onClick={onClose} title={t("common.close")}>
            <Icon name="close" size={16} />
          </button>
        </div>

        <div className="item-icon-picker-toolbar">
          <div className="icon-picker-search">
            <Icon name="search" size={14} />
            <input
              className="settings-input"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("versions.iconSearchPlaceholder")}
              autoFocus
            />
          </div>
          <div className="icon-picker-filters">
            {filters.map((item) => (
              <button
                key={item.key}
                className={`chip ${filter === item.key ? "chip-active" : ""}`}
                onClick={() => setFilter(item.key)}
              >
                {item.label}
              </button>
            ))}
            <button
              className="chip icon-local-chip"
              onClick={() => fileInputRef.current?.click()}
              title={t("versions.iconLocalHint")}
            >
              <Icon name="folder" size={12} />
              {t("versions.iconLocalImage")}
            </button>
            <input
              ref={fileInputRef}
              type="file"
              accept="image/*"
              style={{ display: "none" }}
              onChange={handleLocalImage}
            />
          </div>
        </div>

        <div className="item-icon-grid">
          {list.map((sprite) => (
            <button
              key={sprite.id}
              className={`item-icon-card ${value === sprite.id ? "item-icon-card-active" : ""}`}
              onClick={() => {
                onPick(sprite.id);
                onClose();
              }}
              title={sprite.id}
            >
              <MinecraftIcon id={sprite.id} size={30} />
              <span>{spriteLabel(sprite.id, lang)}</span>
            </button>
          ))}
          {list.length === 0 && (
            <div className="item-icon-empty">{t("common.noResult")}</div>
          )}
        </div>

        <div className="item-icon-picker-foot">
          <button className="btn btn-ghost btn-inline" onClick={() => { onPick(""); onClose(); }}>
            <MinecraftIcon id="" loader="vanilla" size={18} />
            {t("versions.iconAuto")}
          </button>
          <span className="text-tertiary">
            {t("versions.iconSelected", { name: value ? iconName(value) : t("versions.iconAuto") })}
          </span>
        </div>
      </div>
    </div>
  );
}
