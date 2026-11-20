/**
 * 自定义标题栏：配合无边框窗口
 * 中间区域可拖拽窗口，右侧为最小化/最大化/关闭按钮
 */

import { Icon } from "./Icon";
import { useT } from "../lib/i18n";
import { api, type WindowAction } from "../lib/api";

export function TitleBar() {
  const t = useT();
  const control = (action: WindowAction) => {
    api.windowControl(action).catch(() => {
      // 浏览器预览环境下无 Tauri，忽略
    });
  };

  return (
    <div
      className="titlebar"
      data-tauri-drag-region
    >
      <div className="titlebar-brand" data-tauri-drag-region>
        <span className="titlebar-logo" data-tauri-drag-region>☾</span>
        <span data-tauri-drag-region>MSCL</span>
        <span className="titlebar-sub" data-tauri-drag-region>{t("home.title")} Launcher</span>
      </div>
      <div className="titlebar-controls">
        <button className="win-btn" onClick={() => control("minimize")} aria-label={t("titlebar.minimize")}>
          <Icon name="minimize" size={16} />
        </button>
        <button className="win-btn" onClick={() => control("maximize")} aria-label={t("titlebar.maximize")}>
          <Icon name="maximize" size={14} />
        </button>
        <button className="win-btn win-btn-close" onClick={() => control("close")} aria-label={t("common.close")}>
          <Icon name="close" size={16} />
        </button>
      </div>
    </div>
  );
}
