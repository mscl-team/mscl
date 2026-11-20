"""
MSCL 图标生成器 —— 仅使用 Python 标准库，无第三方依赖。
生成一套"月之海"主题占位图标：深蓝圆角底 + 银色月牙。
阶段 5 会用正式设计稿替换。

输出：
  icons/32x32.png
  icons/128x128.png
  icons/128x128@2x.png   (256x256)
  icons/icon.ico         (内嵌 32x32 与 256x256 PNG)
  icons/icon.icns        (占位：单尺寸 icns，macOS 构建可后续替换)
"""

import os
import struct
import zlib

OUT_DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "src-tauri", "icons")

# 配色（月之海）
BG = (26, 46, 78, 255)       # 深海蓝
MOON = (226, 234, 248, 255)  # 银白月牙
STAR = (170, 196, 235, 255)  # 星点


def in_rounded_rect(x, y, size, radius):
    """判断像素是否位于圆角矩形内"""
    cx = min(max(x, radius), size - radius)
    cy = min(max(y, radius), size - radius)
    dx, dy = x - cx, y - cy
    return dx * dx + dy * dy <= radius * radius


def pixel_color(x, y, size):
    """计算单个像素 RGBA"""
    radius = size * 0.22
    if not in_rounded_rect(x + 0.5, y + 0.5, size, radius):
        return (0, 0, 0, 0)  # 圆角外透明

    # 月牙：外圆减去偏移的内圆
    ox, oy, orad = size * 0.50, size * 0.50, size * 0.28
    ix, iy, irad = size * 0.60, size * 0.42, size * 0.25
    px, py = x + 0.5, y + 0.5
    d_outer = (px - ox) ** 2 + (py - oy) ** 2
    d_inner = (px - ix) ** 2 + (py - iy) ** 2
    if d_outer <= orad ** 2 and d_inner > irad ** 2:
        return MOON

    # 星点（右上、右下各一）
    for sx, sy, sr in [(0.74, 0.30, 0.025), (0.70, 0.70, 0.018)]:
        if (px - size * sx) ** 2 + (py - size * sy) ** 2 <= (size * sr) ** 2:
            return STAR

    return BG


def make_png(size):
    """生成 PNG 字节串（RGBA, 8bit, 无交错）"""
    raw = bytearray()
    for y in range(size):
        raw.append(0)  # filter type: None
        for x in range(size):
            raw.extend(pixel_color(x, y, size))

    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        c += struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        return c

    sig = b"\x89PNG\r\n\x1a\n"
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    idat = zlib.compress(bytes(raw), 9)
    return sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")


def make_ico(entries):
    """entries: list of (size, png_bytes)。构造 PNG 压缩的 ICO。"""
    header = struct.pack("<HHH", 0, 1, len(entries))
    offset = 6 + 16 * len(entries)
    dirs = b""
    data = b""
    for size, png in entries:
        w = 0 if size >= 256 else size
        h = 0 if size >= 256 else size
        dirs += struct.pack("<BBBBHHII", w, h, 0, 0, 1, 32, len(png), offset)
        offset += len(png)
        data += png
    return header + dirs + data


def make_icns(png512):
    """占位 icns：单个 ic09 (512x512) 类型条目"""
    icon_data = struct.pack(">4sI", b"ic09", 8 + len(png512)) + png512
    return struct.pack(">4sI", b"icns", 8 + len(icon_data)) + icon_data


def main():
    os.makedirs(OUT_DIR, exist_ok=True)

    png32 = make_png(32)
    png128 = make_png(128)
    png256 = make_png(256)
    png512 = make_png(512)

    files = {
        "32x32.png": png32,
        "128x128.png": png128,
        "128x128@2x.png": png256,
        "icon.ico": make_ico([(32, png32), (256, png256)]),
        "icon.icns": make_icns(png512),
    }
    for name, data in files.items():
        path = os.path.join(OUT_DIR, name)
        with open(path, "wb") as f:
            f.write(data)
        print(f"generated: {path}  ({len(data)} bytes)")


if __name__ == "__main__":
    main()
