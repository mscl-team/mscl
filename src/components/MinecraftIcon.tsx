/** 内置像素图标渲染器：方块 / 物品 / 加载器 / 旧版自定义图片 */
import type { CSSProperties } from "react";
import { effectiveSpriteId, inferLoaderFromId, normalizeLoader, SPRITE_MAP } from "../lib/minecraftIcons";

interface MinecraftIconProps {
  id?: string | null;
  /** 没有自定义图标时，用加载器推断默认图标 */
  loader?: string | null;
  versionId?: string;
  size?: number;
  className?: string;
  style?: CSSProperties;
  title?: string;
}

export function MinecraftIcon({
  id,
  loader,
  versionId,
  size = 20,
  className = "",
  style,
  title,
}: MinecraftIconProps) {
  const resolved = effectiveSpriteId(id, loader, versionId);

  if (resolved.startsWith("data:image")) {
    return (
      <img
        src={resolved}
        alt={title ?? "icon"}
        title={title}
        width={size}
        height={size}
        className={`mc-icon-img ${className}`.trim()}
        style={{ display: "block", flexShrink: 0, objectFit: "contain", ...style }}
      />
    );
  }

  const sprite = SPRITE_MAP.get(resolved) ?? SPRITE_MAP.get("grass_block");
  if (!sprite) return null;

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      shapeRendering="crispEdges"
      className={`mc-icon ${className}`.trim()}
      style={{ display: "block", flexShrink: 0, ...style }}
      role="img"
      aria-label={title ?? sprite.en}
    >
      {title && <title>{title}</title>}
      {sprite.rects.map(([x, y, w, h, color], index) => (
        <rect key={index} x={x} y={y} width={w} height={h} fill={color} />
      ))}
    </svg>
  );
}

interface LoaderBadgeProps {
  loader?: string | null;
  versionId?: string;
  size?: number;
}

/** 版本行里的加载器小徽章：铁砧=Forge，线轴=Fabric，狐狸=NeoForge，拼布=Quilt */
export function LoaderBadge({ loader, versionId, size = 14 }: LoaderBadgeProps) {
  const normalized =
    normalizeLoader(loader && loader.toLowerCase() !== "vanilla" ? loader : "") ||
    inferLoaderFromId(versionId ?? "");
  if (!normalized || normalized === "vanilla") return null;

  const label = normalized.includes("neoforge")
    ? "NeoForge"
    : normalized.includes("fabric")
      ? "Fabric"
      : normalized.includes("quilt")
        ? "Quilt"
        : normalized.includes("forge")
          ? "Forge"
          : "";
  if (!label) return null;

  return (
    <span className="loader-badge" title={label}>
      <MinecraftIcon
        id={
          normalized.includes("neoforge")
            ? "fox"
            : normalized.includes("fabric")
              ? "fabric_spool"
              : normalized.includes("quilt")
                ? "quilt"
                : "anvil"
        }
        size={size}
      />
      <span>{label}</span>
    </span>
  );
}
