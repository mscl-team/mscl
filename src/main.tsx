import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";
import "./styles/app.css";
import "./styles/installed.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

/**
 * 禁用 WebView 右键菜单（避免露出浏览器原生菜单，破坏启动器体验）。
 * 输入框 / 文本域保留右键，方便复制粘贴。
 */
document.addEventListener("contextmenu", (event) => {
  const target = event.target as HTMLElement | null;
  const editable =
    target?.tagName === "INPUT" ||
    target?.tagName === "TEXTAREA" ||
    target?.isContentEditable;
  if (!editable) event.preventDefault();
});

/**
 * 淡出启动画面。
 * 保证至少展示约 600ms，避免 React 挂载过快导致闪一下就消失；
 * 若 WebView 加载本身已超过该时长，则立即淡出。
 */
function dismissSplash() {
  const splash = document.getElementById("splash");
  if (!splash) return;

  const remain = Math.max(0, 600 - performance.now());
  window.setTimeout(() => {
    splash.classList.add("is-out");
    window.setTimeout(() => splash.remove(), 520);
  }, remain);
}

// 等首帧绘制完成再淡出，避免 UI 还没画出来就露出空白
requestAnimationFrame(() => requestAnimationFrame(dismissSplash));
