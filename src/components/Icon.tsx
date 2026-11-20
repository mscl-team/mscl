/**
 * 图标组件：统一的线性 SVG 图标
 * 统一 currentColor，尺寸可控，风格一致（圆润、简洁）
 */

import type { CSSProperties, ReactElement } from "react";

export type IconName =
  | "home"
  | "versions"
  | "settings"
  | "play"
  | "download"
  | "moon"
  | "sun"
  | "minimize"
  | "maximize"
  | "close"
  | "check"
  | "arrow"
  | "refresh"
  | "stop"
  | "trash"
  | "info"
  | "shield"
  | "tools"
  | "mods"
  | "folder"
  | "search"
  | "users"
  | "chevron-down";

interface IconProps {
  name: IconName;
  size?: number;
  style?: CSSProperties;
  strokeWidth?: number;
}

// 各图标的 SVG path（viewBox 0 0 24 24）
const paths: Record<IconName, ReactElement> = {
  home: <path d="M4 10.5 12 4l8 6.5V19a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 19v-8.5Z M9.5 20.5v-6h5v6" />,
  versions: <><rect x="4" y="4" width="16" height="16" rx="3" /><path d="M8 9h8M8 13h8M8 17h5" /></>,
  settings: <><circle cx="12" cy="12" r="3" /><path d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M18.7 5.3l-1.4 1.4M6.7 17.3l-1.4 1.4" /></>,
  play: <path d="M8 5.5v13l11-6.5L8 5.5Z" />,
  download: <path d="M12 3v12M7.5 10.5 12 15l4.5-4.5M4 19.5h16" />,
  moon: <path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5Z" />,
  sun: <><circle cx="12" cy="12" r="4" /><path d="M12 2v2.5M12 19.5V22M2 12h2.5M19.5 12H22M4.9 4.9l1.8 1.8M17.3 17.3l1.8 1.8M19.1 4.9l-1.8 1.8M6.7 17.3l-1.8 1.8" /></>,
  minimize: <path d="M5 12h14" />,
  maximize: <rect x="5" y="5" width="14" height="14" rx="2" />,
  close: <path d="M6 6l12 12M18 6 6 18" />,
  check: <path d="M5 12.5 10 17l9-10" />,
  arrow: <path d="M5 12h14M13 6l6 6-6 6" />,
  refresh: <path d="M20 11a8 8 0 1 0-2.3 6.4M20 5v6h-6" />,
  stop: <rect x="6.5" y="6.5" width="11" height="11" rx="2.5" />,
  trash: <><path d="M4.5 7h15M9.5 7V4.5h5V7M6.5 7l.8 12.5h9.4L17.5 7M10 10.8v5.4M14 10.8v5.4" /></>,
  info: <><circle cx="12" cy="12" r="9" /><path d="M12 11.2v5.3" /><circle cx="12" cy="7.8" r="0.8" fill="currentColor" stroke="none" /></>,
  shield: <><path d="M12 3 5 5.8v6.1c0 4.4 2.9 7.4 7 9.1 4.1-1.7 7-4.7 7-9.1V5.8L12 3Z" /><path d="M9 11.8l2 2 4-4" /></>,
  tools: <><path d="M14.7 6.3a3.8 3.8 0 0 0-4.9 4.8L4 16.9V20h3.1l5.8-5.8a3.8 3.8 0 0 0 4.8-4.9l-2.4 2.4-2.1-.4-.4-2.1 2.4-2.4Z" /><path d="M5 19l4.5-4.5" /></>,
  mods: <><rect x="3.5" y="3.5" width="7" height="7" rx="2" /><rect x="13.5" y="3.5" width="7" height="7" rx="2" /><rect x="3.5" y="13.5" width="7" height="7" rx="2" /><path d="M17 13.5v6.5M13.75 16.75h6.5" /></>,
  folder: <path d="M3.5 7.5A2 2 0 0 1 5.5 5.5h3.2l1.6 2h8.2a2 2 0 0 1 2 2v7.5a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2V7.5Z" />,
  search: <><circle cx="11" cy="11" r="6.5" /><path d="M16 16l4.5 4.5" /></>,
  users: <><circle cx="9" cy="8" r="3.2" /><path d="M3.5 19c.5-3 2.5-4.6 5.5-4.6s5 1.6 5.5 4.6" /><path d="M16 5.4a3 3 0 0 1 0 5.8M17.5 14.8c2 .5 3 1.9 3.3 4.2" /></>,
  "chevron-down": <path d="M6 9.5l6 6 6-6" />,
};

export function Icon({ name, size = 20, style, strokeWidth = 1.8 }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      style={{ display: "block", flexShrink: 0, ...style }}
      aria-hidden="true"
    >
      {paths[name]}
    </svg>
  );
}
