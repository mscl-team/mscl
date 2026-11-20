/**
 * 构建 MSCL 自绘安装程序（WebView 版）
 *
 * 流程：
 *   1. 构建启动器前端（npm run build）
 *   2. 编译启动器 exe（cargo tauri build --no-bundle）
 *   3. 把 exe / 许可证复制进 installer 的 resources
 *   4. 构建安装器前端（vite）
 *   5. 编译安装器 exe（cargo tauri build --no-bundle）
 *   6. 输出到 release/MSCL_Setup_<version>.exe
 */
import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const launcherCfg = JSON.parse(
  fs.readFileSync(path.join(root, "src-tauri/tauri.conf.json"), "utf8")
);
const version = launcherCfg.version;

const run = (cmd, cwd = root) => {
  console.log(`\n$ ${cmd}`);
  execSync(cmd, { cwd, stdio: "inherit", shell: true });
};

const copy = (from, to) => {
  fs.mkdirSync(path.dirname(to), { recursive: true });
  fs.copyFileSync(from, to);
  console.log(`copied ${path.relative(root, from)} -> ${path.relative(root, to)}`);
};

// 1. 启动器前端
run("npm run build");

// 2. 启动器 exe
run("cargo tauri build --no-bundle", path.join(root, "src-tauri"));

const launcherExe = path.join(root, "src-tauri/target/release/mscl-app.exe");
if (!fs.existsSync(launcherExe)) {
  throw new Error("未找到启动器 exe：" + launcherExe);
}

// 3. 资源准备
copy(launcherExe, path.join(root, "installer/src-tauri/resources/mscl-app.exe"));
copy(
  path.join(root, "src-tauri/windows/LICENSE.txt"),
  path.join(root, "installer/src-tauri/resources/LICENSE.txt")
);

// 4. 安装器前端（先清理旧产物，避免带 hash 的文件累积）
const installerDist = path.join(root, "installer/dist");
if (fs.existsSync(installerDist)) {
  const literal = installerDist.replace(/'/g, "''");
  execSync(
    `powershell -NoProfile -NonInteractive -Command "Remove-Item -Recurse -Force -LiteralPath '${literal}' -ErrorAction SilentlyContinue"`,
    { cwd: root, stdio: "ignore", shell: true }
  );
}
run("npx vite build --config installer/vite.config.ts");

// 5. 安装器 exe
run("cargo tauri build --no-bundle", path.join(root, "installer"));

const installerExe = path.join(root, "installer/src-tauri/target/release/mscl-installer.exe");
const outDir = path.join(root, "release");
fs.mkdirSync(outDir, { recursive: true });
const outFile = path.join(outDir, `MSCL_Setup_${version}.exe`);
fs.copyFileSync(installerExe, outFile);

// 独立卸载程序：与安装器同一可执行文件，带 --uninstall 参数时进入卸载流程
const uninstallerFile = path.join(outDir, `MSCL_Uninstall_${version}.exe`);
fs.copyFileSync(installerExe, uninstallerFile);

const mb = (fs.statSync(outFile).size / 1024 / 1024).toFixed(2);
console.log(`\n✔ 安装程序已生成：${path.relative(root, outFile)} (${mb} MB)`);
console.log(`✔ 卸载程序已生成：${path.relative(root, uninstallerFile)} (${mb} MB)`);
