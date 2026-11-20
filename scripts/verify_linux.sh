#!/usr/bin/env bash
# Linux / WSLg 端到端验证脚本（阶段 7）
#
# 用法：
#   bash scripts/verify_linux.sh            # 完整验证（含下载 + 启动 GUI）
#   bash scripts/verify_linux.sh --no-gui   # 只验证编译/下载/命令，不启动 GUI（CI/无显示环境用）
#
# 前置：
#   - WSL2 + WSLg（Windows 11）或原生 Linux 桌面
#   - Rust / Node 已安装

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NO_GUI=0
for arg in "$@"; do
  case "$arg" in
    --no-gui) NO_GUI=1 ;;
  esac
done

echo "===== MSCL Linux 验证 (项目根: $ROOT) ====="

# ---- 1. 系统依赖检查 ----
echo "[1/6] 检查系统依赖..."
MISSING=0
for pkg in pkg-config; do
  command -v "$pkg" >/dev/null 2>&1 || { echo "  缺少 $pkg"; MISSING=1; }
done
# WebKit2GTK（Tauri Linux 必须）
if ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
  echo "  缺少 WebKit2GTK 4.1 开发库，请先执行："
  echo "  sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file \\"
  echo "    libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev"
  MISSING=1
fi
if [ "$MISSING" -ne 0 ]; then
  echo "系统依赖未满足，中止。"; exit 1
fi
echo "  系统依赖 OK"

# ---- 2. 环境信息 ----
echo "[2/6] 环境信息..."
echo "  系统: $(uname -srm)"
echo "  Rust: $(rustc --version)"
if [ -n "${XDG_DATA_HOME:-}" ]; then
  echo "  XDG_DATA_HOME: $XDG_DATA_HOME -> 数据目录将是 \$XDG_DATA_HOME/mscl"
else
  echo "  XDG_DATA_HOME 未设置 -> 数据目录将是 ~/.local/share/mscl"
fi
echo "  DISPLAY: ${DISPLAY:-未设置}"
java -version 2>&1 | head -1 || echo "  警告: 未检测到 Java（下载阶段不需要，启动需要）"

# ---- 3. 前端构建 ----
echo "[3/6] 构建前端..."
cd "$ROOT"
npm ci
npm run build

# ---- 4. 类型检查 + Clippy ----
echo "[4/6] 类型检查与 Clippy..."
cd "$ROOT/src-tauri"
cargo check --workspace --all-targets --locked
cargo clippy -p mscl-core -p mscl-platform -- -D warnings

# ---- 5. 构建验证程序 ----
echo "[5/6] 构建端到端验证程序..."
cargo build --example e2e_live --example e2e_launch --locked

# ---- 6. 运行下载 + 命令构建验证 ----
echo "[6/6] 运行端到端下载验证（约 588MB，视网速数分钟）..."
./target/debug/examples/e2e_live

if [ "$NO_GUI" -eq 1 ]; then
  echo "===== --no-gui：跳过实际启动，验证完成 ====="
  exit 0
fi

# ---- 7. 实际启动游戏（可选） ----
if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
  echo "未检测到显示环境（DISPLAY/WAYLAND_DISPLAY），跳过 GUI 启动。"
  echo "如需验证 GUI，请在 WSLg 或 Linux 桌面环境下运行。"
  exit 0
fi
echo "[7/7] 实际启动游戏（观察 90 秒）..."
echo "提示：WSLg 下 Tauri 建议设置 GDK_BACKEND=x11，透明窗口效果以实际为准。"
export GDK_BACKEND="${GDK_BACKEND:-x11}"
./target/debug/examples/e2e_launch

echo "===== Linux 验证全部完成 ====="
