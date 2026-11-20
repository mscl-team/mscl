/**
 * 左侧导航栏：首页 / 版本管理 / Mod / 设置
 * 底部是主题切换按钮
 */

import { Icon, type IconName } from "./Icon";
import { useT } from "../lib/i18n";
import type { Page } from "../App";

interface SidebarProps {
  current: Page;
  onNavigate: (page: Page) => void;
  theme: "dark" | "light";
  onToggleTheme: () => void;
}

const items: { page: Page; icon: IconName; labelKey: string }[] = [
  { page: "home", icon: "home", labelKey: "nav.home" },
  { page: "users", icon: "users", labelKey: "nav.users" },
  { page: "versions", icon: "versions", labelKey: "nav.versions" },
  { page: "resources", icon: "search", labelKey: "nav.resources" },
  { page: "features", icon: "tools", labelKey: "nav.features" },
  { page: "settings", icon: "settings", labelKey: "nav.settings" },
];

export function Sidebar({
  current, onNavigate, theme, onToggleTheme }: SidebarProps) {
  const t = useT();
  return (
    <nav className="sidebar">
      <div className="sidebar-nav">
        {items.map((item) => (
          <button
            key={item.page}
            className={`nav-item ${current === item.page ? "nav-item-active" : ""}`}
            onClick={() => onNavigate(item.page)}
            title={t(item.labelKey)}
          >
            <Icon name={item.icon} size={21} />
            <span className="nav-label">{t(item.labelKey)}</span>
          </button>
        ))}
      </div>
      <div className="sidebar-bottom">
        <button className="nav-item" onClick={onToggleTheme} title={t("nav.toggleTheme")}>
          <Icon name={theme === "dark" ? "sun" : "moon"} size={21} />
          <span className="nav-label">{theme === "dark" ? t("nav.themeLight") : t("nav.themeDark")}</span>
        </button>
      </div>
    </nav>
  );
}
