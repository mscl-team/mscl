# MSCL 跨端验证清单

阶段 7 端到端联调记录。目标是让每个平台从"能编译"到"能下载、能启动、能玩"。

## 验证策略评估（无 macOS/Linux 实机时）

由于当前只有 Windows 环境，对"如何验证 macOS/Linux"的评估结论：

| 方案 | 可行性 | 结论 |
|---|---|---|
| **Linux 虚拟机（VirtualBox/Hyper-V）** | 可行但重 | 不必要；WSLg 已能跑 Linux GUI 应用，成本更低 |
| **Linux WSLg** | ✅ 推荐 | Windows 11 原生支持，Linux 应用以独立窗口显示，支持 X11/Wayland + GPU 加速。可验证平台相关逻辑与启动，仅窗口管理器相关的透明效果覆盖不到 |
| **macOS 虚拟机（Windows 上）** | ❌ 不建议 | Apple 许可证限制 macOS 只能在 Apple 硬件上虚拟化；非官方方案有合规风险、无 GPU 加速、更新易崩、镜像来源风险 |
| **macOS 云主机 / 租借** | 合规但持续成本 | 短期验证可考虑，非长期方案 |
| **GitHub Actions CI 编译矩阵** | ✅ 推荐兜底 | 在真实 Linux / macOS（Apple 硬件）runner 上做编译 + 类型检查，覆盖平台相关分支，免费且合规 |
| **暂时跳过，仅记录** | 风险可控 | core 层平台无关逻辑已被 Windows 实测覆盖；仅"透明窗口"等 GUI 效果未验证 |

**采纳策略**：
1. **Linux** —— 用 WSLg 实机验证（脚本已备好：`scripts/verify_linux.sh`）
2. **macOS** —— 跳过本地虚拟机，用 CI 编译矩阵兜底（配置已备好：`.github/workflows/cross-platform.yml`）
3. 所有未实测项在本文件明确标注，待有真机时补测

### 兜底工具使用

**Linux 一键验证**（WSLg 或原生 Linux）：
```bash
bash scripts/verify_linux.sh            # 完整：编译 + 下载校验 + 启动 GUI
bash scripts/verify_linux.sh --no-gui   # 无显示环境：跳过 GUI 启动
```
脚本会依次检查系统依赖、打印环境信息（含 XDG 路径与 Java）、构建前端、`cargo check` + clippy、构建并运行 `e2e_live`（下载 588MB 并校验）与 `e2e_launch`（启动游戏观察 90 秒）。

**CI 编译矩阵**：推送到 main/master 或开 PR 时自动触发，在 Linux / macOS(ARM64) / Windows 三个 runner 上执行 `cargo check --all-targets` + clippy + 生产构建；另含 macOS x86_64 交叉编译检查（因 Intel runner 已弃用）。macOS runner 均为 Apple 硬件，符合 Apple 许可。

---

## 已实测通过：Windows 11（x86_64）

联调日期：2026-11-17

### 环境
- Java：Microsoft OpenJDK 25.0.3 LTS（满足最新版 MC 的 Java 25 要求）
- WebView2 Runtime：157.x
- GPU：NVIDIA RTX 5060 Laptop（OpenGL 3.3）

### 端到端链路（全部通过）
| 步骤 | 结果 | 关键数据 |
|---|---|---|
| 版本清单 | ✅ | 最新正式版 26.3 |
| 版本元数据 | ✅ | 114 个库，Java 25，主类 net.minecraft.client.main.Main |
| 资源索引 | ✅ | 5149 个资源对象 |
| 任务规划 | ✅ | 5225 个任务，588 MB |
| 并发下载 + SHA1 | ✅ | 下载 5225 / 失败 0，抽查 8 个文件哈希正确 |
| 资源路径规则 | ✅ | `assets/objects/{hash前2位}/{完整hash}` |
| 启动命令构建 | ✅ | 35 个参数，占位符全替换，classpath 75 项 0 缺失 |
| 实际启动游戏 | ✅ | 进入主菜单，LWJGL 3.4.3 / OpenGL / OpenAL / 纹理图集全部正常，稳定运行 90 秒无原生库错误 |

### 已知正常现象（非 BUG）
- 离线账号使用占位 access token，启动时日志会出现两类 ERROR：
  - `Failed to fetch user properties`（HTTP 401，取皮肤属性失败）
  - `Could not authorize you against Realms server`（JWT 解析失败）
  这些只影响皮肤与 Realms，**不影响单机游戏**。正版登录阶段接入微软认证后会消失。
- 最新版 26.3 的原生库走 classpath 自动解压，`plan_native_jars` 返回 0 属正常；老版本（如 1.16.5）才会解压 native jar。

### 复现命令
```bash
cd src-tauri
cargo build --example e2e_live --offline
./target/debug/examples/e2e_live.exe     # 全量下载 + 校验 + 命令构建
cargo build --example e2e_launch --offline
./target/debug/examples/e2e_launch.exe   # 实际启动，观察 90 秒
```

---

## 待验证：macOS（x86_64 / Apple Silicon）

当前开发环境是 Windows，无法交叉编译或运行 macOS 版本。需在 macOS 机器上完成：

### 编译准备
- [ ] 安装 Xcode Command Line Tools：`xcode-select --install`
- [ ] 安装 Rust target（如在 Apple Silicon 上编译 x86_64）：`rustup target add x86_64-apple-darwin`
- [ ] 安装前置：`npm install`（前端）、Homebrew 可选

### 验证步骤
- [ ] `cargo check --workspace` 通过（core/platform 为纯 Rust，预期无平台差异）
- [ ] `cargo build --features custom-protocol` 生成 .app
- [ ] `cargo tauri build --features custom-protocol` 生成 .dmg / .app
- [ ] 启动 GUI，确认透明圆角窗口在 macOS 上正常（Tauri macOS 用 WKWebView，透明表现与 Windows 不同）
- [ ] 下载最新版，确认 rules 过滤选中 `natives-macos`（core 逻辑已覆盖，见 `native_library_extensions` 的 `dylib`+`so`）
- [ ] 实际启动游戏，确认 LWJGL macOS 原生库（`.dylib`）加载正常
- [ ] 数据目录：`~/Library/Application Support/MSCL`（`default_data_dir` 已实现）

### 已知风险点
- **透明窗口**：macOS 上 `transparent: true` 需要窗口支持，若出现渲染异常，参考 Windows 的经验先关闭透明再定位
- **Java 检测**：已实现扫描 `/Library/Java/JavaVirtualMachines/<jdk>/Contents/Home/bin/java` 及 Homebrew openjdk 布局，并回退 `/usr/bin/java`（阶段 7 加固时补齐）

---

## 待验证：Linux（x86_64，仅 AppImage）

当前在 Windows 上无法交叉编译 Linux 包。需在 Linux 机器上完成：

### 系统依赖（Debian/Ubuntu 示例）
```bash
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl wget file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

### 验证步骤
- [ ] `cargo check --workspace` 通过
- [ ] `cargo build --features custom-protocol` 生成可执行文件
- [ ] `cargo tauri build --features custom-protocol` 生成 AppImage（tauri-cli 会自动下载 linuxdeploy/appimagekit）
- [ ] 启动 GUI，确认透明圆角窗口（Linux 合成器环境下表现依赖 WM）
- [ ] 确认数据目录遵循 XDG：`$XDG_DATA_HOME/mscl` 或 `~/.local/share/mscl`
- [ ] 确认 Java 检测能扫到 `/usr/lib/jvm/*/bin/java`（已实现）
- [ ] 下载最新版，确认 rules 过滤选中 `natives-linux`（已实现，见 Linux classifier 单测）
- [ ] 实际启动游戏，确认 LWJGL `.so` 加载正常、无 `UnsatisfiedLinkError`

### 已知风险点
- **透明窗口**：Linux WebKit2GTK 的透明窗口依赖合成器，部分 WM 下可能失效，必要时关闭 `transparent`
- **GLIBC 版本**：AppImage 在过旧的发行版（如 Ubuntu 18.04）上可能因 GLIBC 过老无法运行，建议在较新系统上构建
- **架构**：当前仅支持 x86_64，ARM64 不在范围内（Mojang 原生库支持不足）

---

## 分层验证策略（为何 Windows 实测能覆盖大部分逻辑）

本项目 core 层是纯逻辑，平台信息通过 `TargetPlatform` 参数传入，因此：
- rules 过滤、classpath 分隔符、占位符替换、路径规则、SHA1 校验、任务规划 —— 这些逻辑平台无关，Windows 实测通过的结论对 macOS/Linux 同样成立
- 真正需要逐平台验证的只有**平台相关**部分：Java 检测路径、默认数据目录、 natives 解压扩展名、进程启动、Webview 透明窗口

这些平台相关点已在代码中分平台实现，并在上文各平台清单中逐项列出验证方法。
