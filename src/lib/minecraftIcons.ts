/**
 * 内置 Minecraft 方块 / 物品 / 加载器像素图标。
 *
 * 设计约束：
 * - 完全离线：图标以 16x16 色块内置，不请求任何远程贴图。
 * - 统一渲染：所有图标走同一个 SVG 组件，尺寸可控、边缘保持像素风。
 * - 稳定标识：instance.json 里保存的是 id（如 diamond_sword / fabric_spool），
 *   不是 emoji，也不是 data URL，避免大字段与跨平台字体差异。
 */

type Rect = readonly [number, number, number, number, string];

export type SpriteCategory = "block" | "item" | "loader";

export interface MinecraftSprite {
  id: string;
  zh: string;
  en: string;
  category: SpriteCategory;
  rects: Rect[];
}

const r = (x: number, y: number, w: number, h: number, c: string): Rect => [
  x,
  y,
  w,
  h,
  c,
];

const C = {
  grass: "#5a9e3b",
  grassHi: "#7fc552",
  grassDark: "#3f7429",
  dirt: "#8b5a2b",
  dirtDark: "#6b4221",
  dirtHi: "#a06b36",
  stone: "#8e8e8e",
  stoneHi: "#b5b5b5",
  stoneDark: "#636363",
  wood: "#9a6a35",
  woodDark: "#6b4520",
  woodHi: "#c08b4b",
  metal: "#c9cdd4",
  metalHi: "#f0f2f5",
  metalDark: "#7c838e",
  diamond: "#4bd6d0",
  diamondHi: "#a8ffff",
  diamondDark: "#239c9b",
  emerald: "#39c95f",
  emeraldHi: "#8bff9f",
  emeraldDark: "#1d8b3b",
  gold: "#f5c542",
  goldHi: "#fff0a3",
  goldDark: "#bf8c18",
  red: "#c93b32",
  redHi: "#ff7165",
  redDark: "#8c241e",
  purple: "#8a4fc7",
  purpleHi: "#c492ff",
  purpleDark: "#5b2d8c",
  cyan: "#37c6d8",
  cyanHi: "#9af0fa",
  black: "#1d1d24",
  white: "#f5f5f5",
  orange: "#e8862c",
  orangeHi: "#ffbd63",
  brown: "#7a4a22",
  blue: "#3a6fc4",
  blueHi: "#7fb3ff",
};

export const SPRITES: MinecraftSprite[] = [
  {
    id: "grass_block",
    zh: "草方块",
    en: "Grass Block",
    category: "block",
    rects: [
      r(0, 4, 16, 12, C.dirt),
      r(0, 4, 3, 12, C.dirtDark),
      r(2, 7, 2, 1, C.dirtHi),
      r(8, 9, 3, 1, C.dirtHi),
      r(12, 12, 2, 1, C.dirtDark),
      r(5, 14, 3, 1, C.dirtDark),
      r(0, 0, 16, 4, C.grass),
      r(0, 0, 16, 1, C.grassHi),
      r(2, 4, 2, 2, C.grass),
      r(6, 4, 1, 3, C.grassDark),
      r(10, 4, 3, 2, C.grass),
      r(14, 4, 1, 2, C.grassDark),
    ],
  },
  {
    id: "dirt",
    zh: "泥土",
    en: "Dirt",
    category: "block",
    rects: [
      r(0, 0, 16, 16, C.dirt),
      r(0, 0, 3, 16, C.dirtDark),
      r(3, 3, 2, 1, C.dirtHi),
      r(8, 5, 3, 1, C.dirtHi),
      r(12, 10, 2, 1, C.dirtDark),
      r(5, 13, 3, 1, C.dirtDark),
    ],
  },
  {
    id: "stone",
    zh: "石头",
    en: "Stone",
    category: "block",
    rects: [
      r(0, 0, 16, 16, C.stone),
      r(0, 0, 16, 2, C.stoneHi),
      r(0, 0, 2, 16, C.stoneHi),
      r(0, 14, 16, 2, C.stoneDark),
      r(14, 0, 2, 16, C.stoneDark),
      r(4, 5, 3, 1, C.stoneDark),
      r(9, 10, 2, 1, C.stoneHi),
    ],
  },
  {
    id: "oak_log",
    zh: "橡木原木",
    en: "Oak Log",
    category: "block",
    rects: [
      r(0, 0, 16, 16, C.wood),
      r(0, 0, 2, 16, C.woodDark),
      r(14, 0, 2, 16, C.woodDark),
      r(2, 1, 1, 14, C.woodHi),
      r(11, 2, 1, 12, C.woodHi),
      r(5, 0, 6, 16, C.woodDark),
      r(6, 1, 4, 14, C.wood),
      r(7, 3, 2, 10, C.woodHi),
    ],
  },
  {
    id: "crafting_table",
    zh: "工作台",
    en: "Crafting Table",
    category: "block",
    rects: [
      r(0, 0, 16, 16, C.woodDark),
      r(1, 1, 14, 14, C.wood),
      r(1, 1, 14, 2, C.woodHi),
      r(2, 6, 12, 1, C.woodDark),
      r(2, 11, 12, 1, C.woodDark),
      r(7, 1, 1, 14, C.woodDark),
      r(3, 3, 3, 2, C.woodHi),
      r(10, 8, 3, 2, C.woodHi),
    ],
  },
  {
    id: "furnace",
    zh: "熔炉",
    en: "Furnace",
    category: "block",
    rects: [
      r(0, 0, 16, 16, C.stone),
      r(0, 0, 16, 2, C.stoneHi),
      r(0, 0, 2, 16, C.stoneHi),
      r(3, 6, 10, 8, C.black),
      r(4, 8, 8, 6, C.redDark),
      r(5, 10, 6, 4, C.orange),
      r(6, 11, 4, 3, C.gold),
      r(7, 9, 2, 2, C.orangeHi),
      r(3, 3, 4, 1, C.stoneDark),
    ],
  },
  {
    id: "chest",
    zh: "箱子",
    en: "Chest",
    category: "block",
    rects: [
      r(1, 3, 14, 12, C.woodDark),
      r(2, 4, 12, 10, C.wood),
      r(2, 4, 12, 2, C.woodHi),
      r(2, 8, 12, 1, C.woodDark),
      r(7, 7, 2, 4, C.gold),
      r(7, 8, 2, 2, C.goldHi),
      r(1, 2, 14, 2, C.woodHi),
    ],
  },
  {
    id: "ender_chest",
    zh: "末影箱",
    en: "Ender Chest",
    category: "block",
    rects: [
      r(0, 0, 16, 16, C.black),
      r(1, 1, 14, 14, C.purpleDark),
      r(2, 2, 12, 12, C.black),
      r(3, 3, 10, 10, C.purple),
      r(4, 4, 8, 8, C.purpleDark),
      r(7, 2, 2, 12, C.cyan),
      r(6, 7, 4, 2, C.cyanHi),
      r(3, 10, 2, 2, C.purpleHi),
      r(11, 4, 2, 2, C.purpleHi),
    ],
  },
  {
    id: "obsidian",
    zh: "黑曜石",
    en: "Obsidian",
    category: "block",
    rects: [
      r(0, 0, 16, 16, C.black),
      r(0, 0, 16, 2, C.purpleDark),
      r(0, 0, 2, 16, C.purpleDark),
      r(4, 5, 2, 1, C.purple),
      r(10, 3, 2, 1, C.purpleHi),
      r(7, 11, 2, 1, C.purple),
      r(12, 13, 2, 1, C.purpleDark),
    ],
  },
  {
    id: "beacon",
    zh: "信标",
    en: "Beacon",
    category: "block",
    rects: [
      r(5, 0, 6, 7, C.cyanHi),
      r(6, 1, 4, 6, C.white),
      r(1, 11, 14, 4, C.stoneDark),
      r(3, 9, 10, 3, C.stone),
      r(5, 7, 6, 3, C.stoneHi),
      r(7, 8, 2, 2, C.cyan),
      r(2, 12, 3, 1, C.stoneHi),
      r(11, 13, 3, 1, C.stoneHi),
    ],
  },
  {
    id: "tnt",
    zh: "TNT",
    en: "TNT",
    category: "block",
    rects: [
      r(3, 3, 10, 12, C.red),
      r(3, 3, 10, 2, C.redHi),
      r(3, 3, 2, 12, C.redDark),
      r(3, 7, 10, 3, C.white),
      r(4, 8, 3, 1, C.red),
      r(9, 8, 3, 1, C.red),
      r(7, 1, 2, 2, C.woodDark),
      r(9, 0, 2, 2, C.orangeHi),
    ],
  },
  {
    id: "enchanting_table",
    zh: "附魔台",
    en: "Enchanting Table",
    category: "block",
    rects: [
      r(0, 10, 16, 6, C.stoneDark),
      r(1, 12, 14, 3, C.stone),
      r(2, 6, 12, 5, C.purpleDark),
      r(3, 7, 10, 3, C.purple),
      r(4, 2, 8, 5, C.blue),
      r(5, 3, 6, 3, C.purpleHi),
      r(7, 4, 2, 2, C.white),
      r(2, 14, 3, 1, C.stoneHi),
      r(11, 11, 3, 1, C.stoneHi),
    ],
  },
  {
    id: "nether_brick",
    zh: "下界砖",
    en: "Nether Brick",
    category: "block",
    rects: [
      r(0, 0, 16, 16, C.redDark),
      r(1, 1, 14, 6, "#5d2320"),
      r(8, 1, 1, 6, C.black),
      r(1, 8, 6, 7, "#5d2320"),
      r(9, 8, 6, 7, "#5d2320"),
      r(1, 11, 6, 1, C.black),
      r(9, 11, 6, 1, C.black),
    ],
  },
  {
    id: "diamond",
    zh: "钻石",
    en: "Diamond",
    category: "item",
    rects: [
      r(5, 2, 6, 2, C.diamondHi),
      r(3, 4, 10, 2, C.diamond),
      r(2, 6, 12, 3, C.diamond),
      r(3, 9, 10, 3, C.diamondDark),
      r(5, 12, 6, 2, C.diamondDark),
      r(6, 4, 4, 4, C.diamondHi),
      r(4, 7, 2, 2, C.diamondHi),
    ],
  },
  {
    id: "emerald",
    zh: "绿宝石",
    en: "Emerald",
    category: "item",
    rects: [
      r(5, 2, 6, 2, C.emeraldHi),
      r(3, 4, 10, 2, C.emerald),
      r(2, 6, 12, 3, C.emerald),
      r(3, 9, 10, 3, C.emeraldDark),
      r(5, 12, 6, 2, C.emeraldDark),
      r(6, 4, 4, 4, C.emeraldHi),
      r(4, 7, 2, 2, C.emeraldHi),
    ],
  },
  {
    id: "gold_ingot",
    zh: "金锭",
    en: "Gold Ingot",
    category: "item",
    rects: [
      r(4, 7, 8, 5, C.gold),
      r(5, 6, 6, 1, C.goldHi),
      r(3, 9, 10, 3, C.goldDark),
      r(5, 8, 6, 1, C.goldHi),
      r(6, 11, 4, 1, C.goldDark),
    ],
  },
  {
    id: "iron_ingot",
    zh: "铁锭",
    en: "Iron Ingot",
    category: "item",
    rects: [
      r(4, 7, 8, 5, C.metal),
      r(5, 6, 6, 1, C.metalHi),
      r(3, 9, 10, 3, C.metalDark),
      r(5, 8, 6, 1, C.metalHi),
      r(6, 11, 4, 1, C.metalDark),
    ],
  },
  {
    id: "diamond_sword",
    zh: "钻石剑",
    en: "Diamond Sword",
    category: "item",
    rects: [
      r(10, 1, 3, 2, C.diamondHi),
      r(9, 3, 3, 2, C.diamond),
      r(8, 5, 3, 2, C.diamond),
      r(7, 7, 3, 2, C.diamondDark),
      r(6, 9, 3, 2, C.diamondDark),
      r(5, 10, 3, 2, C.gold),
      r(5, 11, 2, 3, C.woodDark),
      r(7, 8, 1, 2, C.diamondHi),
    ],
  },
  {
    id: "diamond_pickaxe",
    zh: "钻石镐",
    en: "Diamond Pickaxe",
    category: "item",
    rects: [
      r(2, 3, 5, 2, C.diamondDark),
      r(1, 5, 5, 2, C.diamond),
      r(3, 7, 4, 2, C.diamondDark),
      r(9, 3, 5, 2, C.diamondDark),
      r(10, 5, 5, 2, C.diamond),
      r(9, 7, 4, 2, C.diamondDark),
      r(7, 5, 2, 8, C.woodDark),
      r(7, 5, 1, 8, C.woodHi),
    ],
  },
  {
    id: "book",
    zh: "书",
    en: "Book",
    category: "item",
    rects: [
      r(3, 2, 10, 12, C.redDark),
      r(4, 3, 8, 10, C.red),
      r(5, 3, 6, 10, C.white),
      r(4, 5, 8, 1, C.gold),
      r(4, 10, 8, 1, C.gold),
      r(5, 4, 6, 1, C.white),
    ],
  },
  {
    id: "nether_star",
    zh: "下界之星",
    en: "Nether Star",
    category: "item",
    rects: [
      r(6, 1, 4, 3, C.white),
      r(1, 6, 14, 4, C.white),
      r(6, 12, 4, 3, C.white),
      r(4, 4, 8, 8, C.white),
      r(6, 6, 4, 4, C.cyan),
      r(7, 7, 2, 2, C.cyanHi),
      r(2, 7, 3, 2, C.stoneHi),
      r(11, 7, 3, 2, C.stoneHi),
    ],
  },
  {
    id: "compass",
    zh: "指南针",
    en: "Compass",
    category: "item",
    rects: [
      r(4, 3, 8, 10, C.goldDark),
      r(5, 4, 6, 8, C.gold),
      r(6, 5, 4, 6, C.white),
      r(7, 6, 2, 4, C.red),
      r(8, 6, 1, 2, C.white),
      r(3, 7, 2, 2, C.goldHi),
      r(11, 7, 2, 2, C.goldHi),
    ],
  },
  {
    id: "map",
    zh: "地图",
    en: "Map",
    category: "item",
    rects: [
      r(2, 2, 12, 12, C.wood),
      r(3, 3, 10, 10, "#d8c79a"),
      r(4, 5, 4, 2, C.grassDark),
      r(9, 4, 3, 3, C.stoneDark),
      r(5, 9, 6, 2, C.blue),
      r(10, 9, 2, 2, C.grass),
      r(7, 7, 2, 2, C.red),
    ],
  },
  {
    id: "torch",
    zh: "火把",
    en: "Torch",
    category: "item",
    rects: [
      r(7, 6, 2, 9, C.woodDark),
      r(7, 6, 1, 9, C.woodHi),
      r(6, 3, 4, 3, C.orange),
      r(7, 1, 2, 3, C.gold),
      r(7, 2, 1, 2, C.goldHi),
    ],
  },
  {
    id: "potion",
    zh: "药水",
    en: "Potion",
    category: "item",
    rects: [
      r(6, 1, 4, 2, C.woodDark),
      r(7, 3, 2, 2, C.white),
      r(5, 5, 6, 2, C.stoneHi),
      r(4, 7, 8, 7, C.blue),
      r(5, 8, 6, 5, C.purple),
      r(6, 9, 3, 2, C.purpleHi),
      r(5, 7, 1, 6, C.white),
    ],
  },
  {
    id: "cake",
    zh: "蛋糕",
    en: "Cake",
    category: "item",
    rects: [
      r(2, 7, 12, 6, C.white),
      r(2, 11, 12, 2, C.wood),
      r(3, 6, 10, 2, C.white),
      r(5, 4, 6, 3, C.red),
      r(7, 3, 2, 2, C.redHi),
      r(4, 8, 8, 1, C.red),
    ],
  },
  {
    id: "elytra",
    zh: "鞘翅",
    en: "Elytra",
    category: "item",
    rects: [
      r(6, 4, 4, 8, C.woodDark),
      r(1, 3, 5, 3, C.wood),
      r(0, 6, 6, 3, C.woodHi),
      r(2, 9, 4, 3, C.wood),
      r(10, 3, 5, 3, C.wood),
      r(10, 6, 6, 3, C.woodHi),
      r(10, 9, 4, 3, C.wood),
      r(7, 6, 2, 4, C.woodDark),
    ],
  },
  {
    id: "redstone",
    zh: "红石粉",
    en: "Redstone",
    category: "item",
    rects: [
      r(3, 4, 3, 3, C.red),
      r(8, 3, 3, 3, C.redHi),
      r(5, 8, 3, 3, C.red),
      r(10, 9, 3, 3, C.redDark),
      r(2, 10, 2, 2, C.redDark),
      r(12, 5, 2, 2, C.red),
    ],
  },
  {
    id: "diamond_helmet",
    zh: "钻石头盔",
    en: "Diamond Helmet",
    category: "item",
    rects: [
      r(4, 2, 8, 3, C.diamondHi),
      r(3, 5, 10, 5, C.diamond),
      r(4, 10, 8, 3, C.diamondDark),
      r(3, 8, 2, 5, C.diamondDark),
      r(11, 8, 2, 5, C.diamondDark),
      r(6, 6, 4, 3, C.diamondHi),
      r(7, 7, 2, 2, C.diamondDark),
    ],
  },
  {
    id: "lava_bucket",
    zh: "岩浆桶",
    en: "Lava Bucket",
    category: "item",
    rects: [
      r(4, 3, 8, 2, C.metalDark),
      r(3, 5, 10, 9, C.metal),
      r(4, 6, 8, 6, C.orange),
      r(5, 7, 6, 4, C.gold),
      r(6, 8, 4, 2, C.goldHi),
      r(3, 5, 2, 9, C.metalHi),
      r(11, 5, 2, 9, C.metalDark),
      r(5, 13, 6, 1, C.metalDark),
    ],
  },
  {
    id: "anvil",
    zh: "铁砧（Forge）",
    en: "Anvil (Forge)",
    category: "loader",
    rects: [
      r(2, 2, 12, 3, C.metal),
      r(3, 5, 10, 3, C.metalHi),
      r(5, 8, 6, 3, C.metalDark),
      r(3, 11, 10, 3, C.metal),
      r(4, 2, 8, 1, C.metalHi),
      r(2, 12, 12, 1, C.metalDark),
      r(7, 6, 2, 2, C.stoneDark),
    ],
  },
  {
    id: "fabric_spool",
    zh: "线轴（Fabric）",
    en: "Spool (Fabric)",
    category: "loader",
    rects: [
      r(3, 2, 10, 2, C.woodHi),
      r(4, 4, 8, 8, C.red),
      r(2, 4, 2, 8, C.woodDark),
      r(12, 4, 2, 8, C.woodDark),
      r(3, 12, 10, 2, C.woodHi),
      r(5, 5, 6, 1, C.redHi),
      r(6, 7, 4, 1, C.redDark),
      r(5, 9, 6, 1, C.redHi),
      r(7, 4, 2, 8, C.woodDark),
    ],
  },
  {
    id: "fox",
    zh: "狐狸（NeoForge）",
    en: "Fox (NeoForge)",
    category: "loader",
    rects: [
      r(3, 2, 4, 3, C.orange),
      r(9, 2, 4, 3, C.orange),
      r(2, 5, 12, 5, C.orangeHi),
      r(3, 10, 10, 4, C.orange),
      r(5, 6, 2, 2, C.black),
      r(9, 6, 2, 2, C.black),
      r(7, 9, 2, 2, C.black),
      r(4, 12, 3, 2, C.white),
      r(9, 12, 3, 2, C.white),
      r(3, 3, 2, 1, C.redDark),
      r(11, 3, 2, 1, C.redDark),
    ],
  },
  {
    id: "quilt",
    zh: "拼布（Quilt）",
    en: "Quilt",
    category: "loader",
    rects: [
      r(1, 1, 6, 6, C.blue),
      r(9, 1, 6, 6, C.red),
      r(1, 9, 6, 6, C.gold),
      r(9, 9, 6, 6, C.emerald),
      r(7, 1, 2, 14, C.white),
      r(1, 7, 14, 2, C.white),
      r(3, 3, 2, 2, C.blueHi),
      r(11, 11, 2, 2, C.emeraldHi),
    ],
  },
];

export const SPRITE_MAP = new Map(SPRITES.map((s) => [s.id, s]));

export function spriteLabel(id: string, lang: "zh-CN" | "en-US"): string {
  const sprite = SPRITE_MAP.get(id);
  if (!sprite) return id || (lang === "zh-CN" ? "自动" : "Auto");
  return lang === "en-US" ? sprite.en : sprite.zh;
}

/** 按版本 ID 推断加载器，供没有 instance.json 的直装版本使用 */
export function inferLoaderFromId(versionId: string): string {
  const id = versionId.toLowerCase();
  if (id.includes("neoforge")) return "neoforge";
  if (id.includes("fabric")) return "fabric";
  if (id.includes("quilt")) return "quilt";
  if (id.includes("forge")) return "forge";
  return "";
}

/** 规范化加载器名称；vanilla 返回 vanilla，未知返回空串 */
export function normalizeLoader(input?: string | null): string {
  const value = (input ?? "").trim().toLowerCase();
  if (!value || value === "vanilla" || value === "none") return "vanilla";
  if (value.includes("neoforge")) return "neoforge";
  if (value.includes("fabric")) return "fabric";
  if (value.includes("quilt")) return "quilt";
  if (value.includes("forge")) return "forge";
  return "";
}

/** 根据实例元数据与版本 ID 得到应该展示的图标 id */
export function loaderSpriteId(loader?: string | null, versionId?: string): string {
  const normalized = normalizeLoader(loader);
  const inferred = inferLoaderFromId(versionId ?? "");
  const key = normalized !== "vanilla" ? normalized : inferred;
  switch (key) {
    case "fabric":
      return "fabric_spool";
    case "forge":
      return "anvil";
    case "neoforge":
      return "fox";
    case "quilt":
      return "quilt";
    default:
      return "grass_block";
  }
}

/** 合实例图标：自定义 item id 优先；空值时按加载器自动选择 */
export function effectiveSpriteId(
  icon?: string | null,
  loader?: string | null,
  versionId?: string
): string {
  if (icon?.startsWith("data:image")) return icon;
  if (icon && SPRITE_MAP.has(icon)) return icon;
  return loaderSpriteId(loader, versionId);
}

export function isLoaderKey(value: string): boolean {
  return normalizeLoader(value) !== "vanilla" && normalizeLoader(value) !== "";
}

/** 自定义本地图片以 data URL 保存，展示时不能把整串数据当名称 */
export function isCustomImageIcon(id?: string | null): boolean {
  return Boolean(id && id.startsWith("data:image"));
}
