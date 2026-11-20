import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type InstallInfo = {
  version: string;
  default_dir: string;
  license: string;
  uninstall: boolean;
  installed_dir: string | null;
  app_size: number;
};

type Progress = { percent: number; stage: string; message: string };
type LogEntry = { level: string; message: string; id: number };

/** 安装阶段可勾选的默认插件（与宿主 plugins.ts 的 OPTIONAL_DEFAULT_PLUGINS 对齐） */
const DEFAULT_PLUGINS: { id: string; name: string; icon: string; category: string }[] = [
  { id: "page-features", name: "功能介绍", icon: "✧", category: "core" },
  { id: "page-skin", name: "皮肤页", icon: "♟", category: "core" },
  { id: "res-mod", name: "Mod 资源", icon: "▦", category: "core" },
  { id: "res-shader", name: "光影资源", icon: "✺", category: "core" },
  { id: "res-resourcepack", name: "材质包资源", icon: "▧", category: "core" },
  { id: "res-datapack", name: "数据包资源", icon: "▩", category: "core" },
  { id: "res-modpack", name: "整合包资源", icon: "▥", category: "core" },
  { id: "res-server-plugin", name: "服务端插件资源", icon: "⬡", category: "server" },
  { id: "res-launcher-plugin", name: "启动器插件资源", icon: "⊞", category: "launcher" },
  { id: "res-skin", name: "皮肤资源", icon: "♟", category: "server" },
  { id: "launch-java", name: "Java 检测", icon: "☕", category: "core" },
  { id: "launch-memory", name: "内存分配", icon: "▤", category: "core" },
  { id: "launch-args", name: "JVM/游戏参数", icon: "⌘", category: "core" },
  { id: "launch-verify", name: "启动前校验", icon: "✓", category: "core" },
  { id: "launch-isolation", name: "版本隔离", icon: "▦", category: "core" },
  { id: "download-mirror", name: "镜像源管理", icon: "⇆", category: "core" },
  { id: "plugin-skin", name: "皮肤与披风", icon: "♟", category: "core" },
  { id: "plugin-plugin-hub", name: "插件市场", icon: "⊞", category: "launcher" },
  { id: "plugin-auto-update", name: "更新检测", icon: "↻", category: "launcher" },
  { id: "plugin-logger", name: "日志中心", icon: "≡", category: "core" },
  { id: "plugin-theme", name: "主题系统", icon: "◐", category: "core" },
  { id: "plugin-backup", name: "备份与还原", icon: "⇩", category: "core" },
];

/** 向导步骤 */
type WizardStep = 0 | 1 | 2 | 3;
const STEP_LABELS = ["许可协议", "安装位置", "插件与选项", "确认安装"];

type Stage =
  | "wizard"
  | "installing"
  | "done"
  | "uninstall-confirm"
  | "uninstalling"
  | "uninstalled"
  | "uninstall-failed";

function formatSize(bytes: number) {
  if (!bytes) return "—";
  const mb = bytes / 1024 / 1024;
  return mb >= 1024 ? `${(mb / 1024).toFixed(2)} GB` : `${mb.toFixed(1)} MB`;
}

/** 仅浏览器预览时使用的占位协议文本 */
const PREVIEW_LICENSE = `MSCL · 月之海启动器 用户使用协议
版本：1.0    生效日期：2026 年 11 月

在安装或使用 MSCL（以下简称"本软件"）之前，请仔细阅读本协议。
安装、复制或使用本软件，即表示你已阅读、理解并同意接受本协议全部条款。

一、软件性质
1. 本软件是一款第三方的 Minecraft Java 版启动器，用于管理游戏的下载、
   安装、启动、Mod 与整合包等本地资源。
2. 本软件不是 Minecraft 官方产品，与 Mojang Studios、Microsoft 及其关联
   公司没有任何隶属、赞助或背书关系。

二、使用许可
1. 在遵守本协议的前提下，你可以在自己的设备上免费安装和使用本软件。
2. 你可以自由传播本软件的原始安装包，但不得反编译、商业售卖或去除版权。

三、免责声明
本软件按现状提供，因使用或无法使用本软件造成的任何直接或间接损失，
作者不承担赔偿责任。

（预览模式仅显示节选，完整协议随安装包提供）`;

export default function App() {
  const [info, setInfo] = useState<InstallInfo | null>(null);
  const [stage, setStage] = useState<Stage>("wizard");
  const [step, setStep] = useState<WizardStep>(0);
  const [agreed, setAgreed] = useState(false);
  const [dir, setDir] = useState("");
  const [desktop, setDesktop] = useState(true);
  const [startMenu, setStartMenu] = useState(true);
  const [selectedPlugins, setSelectedPlugins] = useState<string[]>(
    DEFAULT_PLUGINS.map((p) => p.id)
  );
  const [progress, setProgress] = useState<Progress>({
    percent: 0,
    stage: "准备安装",
    message: "",
  });
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [doneMessage, setDoneMessage] = useState("");
  const logEl = useRef<HTMLDivElement | null>(null);
  const seq = useRef(0);

  useEffect(() => {
    const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
    if (!inTauri) {
      const previewUninstall =
        typeof window !== "undefined" &&
        new URLSearchParams(window.location.search).get("preview") === "uninstall";
      const previewStep = Number(
        new URLSearchParams(window.location.search).get("step") ?? "0"
      );
      const base = {
        version: "0.1.0",
        default_dir: "C:\\Users\\Player\\AppData\\Local\\MSCL",
        license: PREVIEW_LICENSE,
        uninstall: previewUninstall,
        installed_dir: previewUninstall ? "C:\\Users\\Player\\AppData\\Local\\MSCL" : null,
        app_size: 20971520,
      };
      setInfo(base);
      setDir(base.installed_dir || base.default_dir);
      if (base.uninstall) setStage("uninstall-confirm");
      else if (previewStep >= 0 && previewStep <= 3) {
        setStep(previewStep as WizardStep);
        if (previewStep > 0) setAgreed(true);
      }
      fetch("license.txt")
        .then((r) => (r.ok ? r.text() : Promise.reject()))
        .then((text) => text && setInfo({ ...base, license: text }))
        .catch(() => {});
      return;
    }
    invoke<InstallInfo>("get_info")
      .then((data) => {
        setInfo(data);
        setDir(data.installed_dir || data.default_dir);
        if (data.uninstall) setStage("uninstall-confirm");
      })
      .catch((e) => setDoneMessage(String(e)));
  }, []);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    listen<Progress>("installer://progress", (e) => setProgress(e.payload)).then((fn) =>
      unlisteners.push(fn)
    );
    listen<{ level: string; message: string }>("installer://log", (e) => {
      seq.current += 1;
      setLogs((prev) => [...prev.slice(-200), { ...e.payload, id: seq.current }]);
    }).then((fn) => unlisteners.push(fn));
    listen<{ ok: boolean; message: string }>("installer://done", (e) => {
      setDoneMessage(e.payload.message);
      setStage((prev) => {
        if (prev === "uninstalling") return e.payload.ok ? "uninstalled" : "uninstall-failed";
        return e.payload.ok ? "done" : "installing";
      });
    }).then((fn) => unlisteners.push(fn));
    return () => unlisteners.forEach((fn) => fn());
  }, []);

  useEffect(() => {
    if (logEl.current) logEl.current.scrollTop = logEl.current.scrollHeight;
  }, [logs]);

  const percentText = useMemo(
    () => Math.min(100, Math.max(0, progress.percent)).toFixed(2),
    [progress.percent]
  );

  const pickDir = useCallback(async () => {
    try {
      const picked = await invoke<string | null>("pick_dir", { current: dir });
      if (picked) setDir(picked);
    } catch {
      /* 用户取消或选择器不可用 */
    }
  }, [dir]);

  const nextStep = useCallback(() => {
    if (step === 0 && !agreed) return;
    if (step === 1 && !dir.trim()) return;
    setStep((s) => Math.min(3, s + 1) as WizardStep);
  }, [step, agreed, dir]);

  const prevStep = useCallback(() => setStep((s) => Math.max(0, s - 1) as WizardStep), []);

  const startInstall = useCallback(() => {
    setLogs([]);
    setProgress({ percent: 0, stage: "准备安装", message: "" });
    setStage("installing");
    invoke("start_install", { dir, desktop, startMenu, plugins: selectedPlugins }).catch((e) =>
      setDoneMessage(String(e))
    );
  }, [dir, desktop, startMenu, selectedPlugins]);

  const startUninstall = useCallback(() => {
    setLogs([]);
    setProgress({ percent: 0, stage: "准备卸载", message: "正在关闭 MSCL…" });
    setDoneMessage("");
    setStage("uninstalling");
    invoke("start_uninstall", { dir }).catch((e) => setDoneMessage(String(e)));
  }, [dir]);

  const quit = useCallback(() => {
    invoke("exit_app").catch(() => {});
  }, []);

  if (!info) {
    return (
      <div className="frame">
        <div className="loading">正在初始化安装程序…</div>
      </div>
    );
  }

  const isUninstall = info.uninstall;

  const wizardNav =
    stage === "wizard" ? (
      <div className="wizard-steps">
        {STEP_LABELS.map((label, i) => (
          <div key={label} className={`wizard-step ${i === step ? "active" : ""} ${i < step ? "done" : ""}`}>
            <span className="wizard-step-num">{i < step ? "✓" : i + 1}</span>
            <span>{label}</span>
          </div>
        ))}
      </div>
    ) : null;

  return (
    <div className="frame">
      <header className="titlebar" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region>
          <span className="brand-mark">☾</span>
          <span className="brand-name">MSCL</span>
          <span className="brand-sub">{isUninstall ? "卸载程序" : "安装程序"}</span>
        </div>
        <div className="tb-spacer" data-tauri-drag-region />
        <button className="tb-close" onClick={quit} title="关闭">✕</button>
      </header>

      {wizardNav}

      <main className="body">
        {stage === "wizard" && step === 0 && (
          <>
            <section className="hero">
              <h1>安装 MSCL 月之海启动器</h1>
              <p className="hero-sub">
                版本 v{info.version} · 安装包 {formatSize(info.app_size)} · 仅安装到当前用户，无需管理员权限
              </p>
            </section>
            <section className="card license-card">
              <div className="card-head">
                <span>用户使用协议</span>
                <span className="hint">请向下滚动阅读完整内容</span>
              </div>
              <pre className="license">{info.license}</pre>
            </section>
            <label className={`agree ${agreed ? "checked" : ""}`}>
              <input type="checkbox" checked={agreed} onChange={(e) => setAgreed(e.target.checked)} />
              <span>我已阅读并同意《MSCL 用户使用协议》</span>
            </label>
          </>
        )}

        {stage === "wizard" && step === 1 && (
          <>
            <section className="hero">
              <h1>选择安装位置</h1>
              <p className="hero-sub">MSCL 仅安装到当前用户目录，不需要管理员权限。</p>
            </section>
            <section className="card path-card">
              <div className="card-head"><span>安装目录</span><span className="hint">可直接使用默认路径</span></div>
              <div className="path-row">
                <span className="field-label">安装到</span>
                <input className="path-input" value={dir} onChange={(e) => setDir(e.target.value)} />
                <button className="btn ghost small" onClick={pickDir}>更改</button>
              </div>
            </section>
            <div className="options">
              <label>
                <input type="checkbox" checked={desktop} onChange={(e) => setDesktop(e.target.checked)} />
                创建桌面快捷方式
              </label>
              <label>
                <input type="checkbox" checked={startMenu} onChange={(e) => setStartMenu(e.target.checked)} />
                创建开始菜单快捷方式
              </label>
            </div>
          </>
        )}

        {stage === "wizard" && step === 2 && (
          <>
            <section className="hero">
              <h1>选择默认插件</h1>
              <p className="hero-sub">启动器按插件组织功能，可按需取消不需要的插件；核心内核始终启用。</p>
            </section>
            <div className="plugin-pick plugin-pick-wizard">
              <div className="plugin-pick-head">
                <span>可选插件（{selectedPlugins.length} / {DEFAULT_PLUGINS.length}）</span>
                <div className="plugin-pick-actions">
                  <button type="button" onClick={() => setSelectedPlugins(DEFAULT_PLUGINS.map((p) => p.id))}>全选</button>
                  <button type="button" onClick={() => setSelectedPlugins([])}>全不选</button>
                </div>
              </div>
              <div className="plugin-pick-list">
                {DEFAULT_PLUGINS.map((plugin) => (
                  <label key={plugin.id} className={selectedPlugins.includes(plugin.id) ? "checked" : ""}>
                    <input
                      type="checkbox"
                      checked={selectedPlugins.includes(plugin.id)}
                      onChange={(e) =>
                        setSelectedPlugins((prev) =>
                          e.target.checked ? [...prev, plugin.id] : prev.filter((id) => id !== plugin.id)
                        )
                      }
                    />
                    <span className="plugin-pick-name">{plugin.icon} {plugin.name}</span>
                    <span className="plugin-pick-cat">{plugin.category === "core" ? "核心" : plugin.category === "launcher" ? "启动器" : "服务端"}</span>
                  </label>
                ))}
              </div>
              <div className="plugin-pick-hint">宿主内核与必需模块始终启用；这里只影响可选默认插件。</div>
            </div>
          </>
        )}

        {stage === "wizard" && step === 3 && (
          <>
            <section className="hero">
              <h1>确认安装</h1>
              <p className="hero-sub">请确认以下设置，点击"开始安装"后将按真实进度执行。</p>
            </section>
            <section className="card confirm-card">
              <div className="confirm-row"><span>版本</span><b>v{info.version}</b></div>
              <div className="confirm-row"><span>安装目录</span><b className="confirm-path">{dir}</b></div>
              <div className="confirm-row"><span>快捷方式</span><b>{desktop ? "桌面" : "无桌面"}{startMenu ? " · 开始菜单" : ""}</b></div>
              <div className="confirm-row"><span>默认插件</span><b>已选 {selectedPlugins.length} / {DEFAULT_PLUGINS.length} 项</b></div>
            </section>
          </>
        )}

        {stage === "wizard" && (
          <footer className="actions wizard-actions">
            <button className="btn ghost" onClick={quit}>取消</button>
            <div className="wizard-nav-buttons">
              {step > 0 && <button className="btn ghost" onClick={prevStep}>上一步</button>}
              {step < 3 ? (
                <button className="btn primary" onClick={nextStep} disabled={(step === 0 && !agreed) || (step === 1 && !dir.trim())}>
                  下一步
                </button>
              ) : (
                <button className="btn primary" onClick={startInstall}>开始安装</button>
              )}
            </div>
          </footer>
        )}

        {stage === "installing" && (
          <>
            <section className="hero compact">
              <h1>正在安装</h1>
              <p className="hero-sub">{progress.message || progress.stage}</p>
            </section>
            <section className="progress-card">
              <div className="progress-head">
                <span className="stage">{progress.stage}</span>
                <span className="percent">{percentText}<i>%</i></span>
              </div>
              <div className="bar"><span style={{ width: `${Math.min(100, progress.percent)}%` }} /></div>
              <div className="logs" ref={logEl}>
                {logs.length === 0 ? (
                  <div className="log muted">等待安装开始…</div>
                ) : (
                  logs.map((l) => <div key={l.id} className={`log ${l.level}`}>{l.message}</div>)
                )}
              </div>
            </section>
          </>
        )}

        {stage === "done" && (
          <section className="result">
            <div className="result-icon ok">✓</div>
            <h2>安装完成</h2>
            <p className="result-text">{doneMessage || "MSCL 已安装到你的电脑"}</p>
            <code className="result-path">{dir}</code>
            <div className="actions center">
              <button className="btn ghost" onClick={() => invoke("open_dir", { dir })}>打开目录</button>
              <button className="btn ghost" onClick={quit}>完成</button>
              <button className="btn primary" onClick={() => invoke("launch_app", { dir }).catch((e) => setDoneMessage(String(e)))}>启动 MSCL</button>
            </div>
          </section>
        )}

        {stage === "uninstall-confirm" && (
          <section className="result">
            <div className="result-icon warn">!</div>
            <h2>卸载 MSCL</h2>
            <p className="result-text">
              请先保存并关闭正在运行的游戏。卸载程序会自动关闭仍在运行的启动器。
              <br />
              将删除程序文件、快捷方式与注册表信息。你的游戏目录、存档与 Mod 不受影响。
            </p>
            <code className="result-path">{dir}</code>
            <div className="actions center">
              <button className="btn ghost" onClick={quit}>取消</button>
              <button className="btn danger" onClick={startUninstall}>卸载</button>
            </div>
          </section>
        )}

        {stage === "uninstalling" && (
          <>
            <section className="hero compact">
              <h1>正在卸载</h1>
              <p className="hero-sub">{progress.message || progress.stage}</p>
            </section>
            <section className="progress-card">
              <div className="progress-head">
                <span className="stage">{progress.stage}</span>
                <span className="percent">{percentText}<i>%</i></span>
              </div>
              <div className="bar"><span style={{ width: `${Math.min(100, progress.percent)}%` }} /></div>
            </section>
          </>
        )}

        {stage === "uninstalled" && (
          <section className="result">
            <div className="result-icon ok">✓</div>
            <h2>已卸载</h2>
            <p className="result-text">{doneMessage || "MSCL 已从这台电脑移除"}</p>
            <div className="actions center">
              <button className="btn primary" onClick={quit}>关闭</button>
            </div>
          </section>
        )}

        {stage === "uninstall-failed" && (
          <section className="result">
            <div className="result-icon warn">!</div>
            <h2>卸载未完成</h2>
            <p className="result-text">{doneMessage || "卸载过程遇到问题，未继续删除文件"}</p>
            <div className="actions center">
              <button className="btn ghost" onClick={quit}>关闭</button>
              <button className="btn danger" onClick={startUninstall}>重试卸载</button>
            </div>
          </section>
        )}
      </main>
    </div>
  );
}
