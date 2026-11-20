import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);

// 禁用 WebView 原生右键菜单；输入框 / 文本域保留右键以便粘贴路径与仓库地址。
// Setup 与卸载器共用此前端，一处即可覆盖两者。
document.addEventListener("contextmenu", (event) => {
  const target = event.target as HTMLElement | null;
  const editable =
    target?.tagName === "INPUT" ||
    target?.tagName === "TEXTAREA" ||
    target?.isContentEditable;
  if (!editable) event.preventDefault();
});
