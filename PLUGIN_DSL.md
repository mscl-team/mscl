# MSCL 插件语法体系（Harness Plugin DSL v1）

MSCL 主程序 `mscl-app.exe` 是**宿主（Harness）**，只负责进程生命周期、插件扫描加载、能力总线、事件总线与配置持久化。所有业务功能（页面、资源分类、启动流程、下载源、主题、设置项……）都以插件为单位挂载。

本文档是 DSL 的权威说明，与以下文件保持同步：

- `plugins.schema.json` —— 机器可读的枚举与字段清单
- `src/lib/plugins.ts` —— 前端类型定义与内置插件清单
- `src-tauri/src/commands/plugin.rs` —— 后端 manifest 解析与校验规则

> DSL 版本：`schema: 1`。清单文件名固定为 `plugin.json`，安装根目录为游戏数据目录下的 `plugins/`。

---

## 1. 设计模型

```
宿主 Harness
 ├── 扫描 plugins/<id>/plugin.json
 ├── 校验 manifest（schema=1、id 规则、枚举合法性）
 ├── 按 order 排序、检查 dependencies
 ├── 注册 capabilities / hooks / extensionPoints
 └── 持久化配置到 plugins/config.json 与 <id>/config.json
```

三个核心概念：

| 概念 | 含义 |
|------|------|
| **category（分类）** | 插件归属的大板块，决定在 UI 的哪个资源大类下展示 |
| **layer（层级）** | 抽象层级：内核 / 功能模块 / 功能插件，决定加载优先级与能否被卸载 |
| **capability（能力）** | 插件向宿主声明"我能提供什么"，宿主据此注册对应通道 |
| **hook（钩子）** | 插件声明"我要在哪个生命周期点被通知" |
| **extensionPoint（扩展点）** | 插件声明"我把内容挂到宿主的哪个挂载位"，用 `point` + `key` 定位 |

---

## 2. 目录结构

```
<游戏数据目录>/plugins/
 ├── config.json                 # 全局：默认勾选的插件 ID 列表
 ├── core-harness/
 │    ├── plugin.json            # manifest（必需）
 │    ├── config.json            # 该插件的配置（可选，由 configSchema 决定）
 │    └── ...                    # 插件自有资源
 ├── res-mod/
 │    └── plugin.json
 └── my-custom-plugin/
      ├── plugin.json
      └── config.json
```

**全局 `config.json` 格式**（由安装器勾选生成）：

```json
{
  "schema": 1,
  "enabled": ["core-harness", "page-home", "res-mod"]
}
```

**单插件 `config.json`**：一个普通 JSON 对象，键为 `configSchema` 中声明的 `key`，值类型须匹配 `type`；未声明的键由宿主按配置校验规则处理。

---

## 3. manifest 字段（`plugin.json`）

### 3.1 字段总表

| 字段 | 类型 | 必需 | 默认 | 说明 |
|------|------|------|------|------|
| `schema` | number | 否 | `1` | DSL 版本，当前仅支持 1，大于 1 会被校验拒绝 |
| `id` | string | 是 | — | 唯一标识，小写字母/数字/短横线，1–64 位 |
| `name` | string | 是 | — | 显示名称，不能为空 |
| `version` | string | 否 | `0.1.0` | 语义化版本 |
| `category` | enum | 否 | `launcher` | `core` / `launcher` / `server` |
| `layer` | enum | 否 | `feature` | `harness` / `module` / `feature` |
| `description` | string | 否 | `""` | 插件说明 |
| `author` | string | 否 | `""` | 作者 |
| `entry` | string | 否 | 同 `id` | 扩展入口标识 |
| `enabled` | boolean | 否 | `true` | 是否启用；`false` 时不被加载 |
| `builtin` | boolean | 否 | `false` | 是否内置（内置不可卸载） |
| `required` | boolean | 否 | `false` | 是否必需（必需不可禁用） |
| `source` | enum | 否 | `local` | `builtin` / `local` / `github` |
| `repo` | string | 否 | — | GitHub 仓库地址，`source=github` 时使用 |
| `capabilities` | string[] | 否 | `[]` | 能力列表，见 §4 |
| `hooks` | string[] | 否 | `["onLoad"]`（前端默认） | 生命周期钩子，见 §5 |
| `extensionPoints` | object[] | 否 | `[]` | 扩展点绑定，见 §6 |
| `dependencies` | string[] | 否 | `[]` | 依赖的插件 ID，须同样满足 ID 规则 |
| `configSchema` | object[] | 否 | `[]` | 配置字段声明，见 §7 |
| `icon` | string | 否 | — | 图标字符（单字/符号） |
| `order` | number | 否 | `100`（前端）/ `0`（后端） | 加载顺序，越小越早 |

### 3.2 字段兼容写法

后端解析同时接受 camelCase 与 snake_case，两种写法等价：

- `extensionPoints` ≡ `extension_points`
- `configSchema` ≡ `config_schema`

前端清单统一使用 camelCase；从第三方仓库导入的 snake_case 清单也能被正确读取。

### 3.3 ID 规则（硬性校验）

```text
^[a-z0-9-]{1,64}$    且不允许空串
```

即：只能包含小写字母、数字、短横线，长度 1–64。`dependencies` 里每个 ID 也按此规则校验。ID 同时作为插件目录名，因此不要用 `..`、路径分隔符或大写字母。

---

## 4. 分类、层级与能力

### 4.1 category（分类）

| 值 | 含义 |
|----|------|
| `core` | 核心模块（宿主内核、页面、通用功能） |
| `launcher` | 启动器插件（插件市场、更新、安装目标等） |
| `server` | 服务端插件 / 服务端资源 |

### 4.2 layer（层级）

| 值 | 含义 | 是否可卸载 |
|----|------|-----------|
| `harness` | 宿主内核（IPC、事件总线、配置中心、安全中心等） | 不可（`required`） |
| `module` | 功能模块 | 视 `required` 而定 |
| `feature` | 功能插件 | 是 |

安装向导里可勾选的默认插件为：`!required && layer !== "harness"` 的插件。

### 4.3 capabilities（能力）

合法值（后端严格校验，未知值报错）：

```
page              提供页面
settings          提供设置分区
resourceCategory  提供资源分类
theme             提供主题
command           提供命令
launch            参与启动流程
download          提供下载能力
storage           提供存储能力
event             参与事件总线
i18n              提供国际化
config            参与配置系统
```

---

## 5. 生命周期钩子（hooks）

合法值（后端严格校验）：

| 钩子 | 触发时机 |
|------|---------|
| `onLoad` | 插件被加载时 |
| `onEnable` | 插件启用时 |
| `onDisable` | 插件禁用时 |
| `onBeforeLaunch` | 游戏启动前 |
| `onLaunch` | 游戏启动中 |
| `onAfterLaunch` | 游戏启动后 |
| `onBeforeResourceInstall` | 资源安装前 |
| `onResourceInstalled` | 资源安装后 |
| `onUpdateCheck` | 更新检查时 |
| `onSettingsSave` | 设置保存时 |

整体顺序：`onLoad → onEnable → run → onDisable`。多个插件间按 `order` 升序、同序按依赖拓扑执行。

---

## 6. 扩展点（extensionPoints）

每项是一个对象，含 `point`（扩展点名）、`key`（挂载位标识），可选 `label`：

```json
{
  "extensionPoints": [
    { "point": "resource.category", "key": "mod", "label": "Mod" },
    { "point": "settings", "key": "download" }
  ]
}
```

合法 `point`（后端严格校验；`key` 不得为空）：

| point | 用途 |
|-------|------|
| `page` | 挂载一个页面 |
| `settings` | 挂载一个设置分区 |
| `resource.category` | 挂载一个资源分类 |
| `launch.step` | 挂载一个启动步骤 |
| `download.source` | 挂载一个下载源 |
| `theme` | 挂载主题 |
| `command` | 挂载命令 |

---

## 7. 配置字段（configSchema）

每项字段：

| 键 | 类型 | 必需 | 说明 |
|----|------|------|------|
| `key` | string | 是 | 配置键，对应 `config.json` 里的键 |
| `label` | string | 建议 | 显示名称 |
| `type` | enum | 是 | `string` / `number` / `boolean` / `select` / `string[]` |
| `default` | 与 type 对应 | 是 | 默认值，**缺失会被校验拒绝** |
| `options` | `{value,label}[]` | 否 | `select` 类型的候选项 |
| `hint` | string | 否 | 提示文本 |
| `min` / `max` | number | 否 | `number` 类型的取值范围 |

示例：

```json
{
  "configSchema": [
    { "key": "autoDetect", "label": "自动检测 Java", "type": "boolean", "default": true },
    { "key": "maxMemoryMb", "label": "最大内存（MB）", "type": "number", "default": 2048, "min": 512, "max": 65536 },
    {
      "key": "source",
      "label": "下载源",
      "type": "select",
      "default": "official",
      "options": [
        { "value": "official", "label": "官方" },
        { "value": "bmclapi", "label": "BMCLAPI" }
      ]
    },
    { "key": "isolateTypes", "label": "隔离类型", "type": "string[]", "default": ["saves", "mods"] }
  ]
}
```

---

## 8. 完整 manifest 示例

```json
{
  "schema": 1,
  "id": "my-feature",
  "name": "我的功能插件",
  "version": "1.0.0",
  "category": "launcher",
  "layer": "feature",
  "description": "演示插件",
  "author": "YourName",
  "entry": "index",
  "enabled": true,
  "builtin": false,
  "required": false,
  "source": "local",
  "capabilities": ["settings", "command"],
  "hooks": ["onLoad", "onSettingsSave"],
  "extensionPoints": [
    { "point": "settings", "key": "myFeature", "label": "我的设置" },
    { "point": "command", "key": "my-feature" }
  ],
  "dependencies": ["core-config"],
  "configSchema": [
    { "key": "enableFeature", "label": "启用功能", "type": "boolean", "default": true }
  ],
  "icon": "★",
  "order": 100
}
```

---

## 9. 安装来源

`source` 决定插件从哪来，三种来源在 UI 与后端均有对应入口：

| source | 说明 | 安装命令 |
|--------|------|---------|
| `builtin` | 内置插件，随程序发布；未勾选的默认插件可在"启动器插件资源"里补装 | `install_builtin_plugin` |
| `local` | 本地插件目录安装 | `install_local_plugin` |
| `github` | 从 GitHub 仓库安装 / 更新 | `install_github_plugin` / `update_plugin_repo_safe` |

### 安装目标目录（`targetDir`）

安装目标受白名单校验，防止任意路径写入。合法目标包括：

- 游戏数据目录（实例 / 本地资源）
- 启动器配置目录
- 系统临时目录
- 启动器 exe 所在目录
- 设置中已注册的自定义资源目标目录

路径中含 `..`（上级目录）会被直接拒绝。

### 插件仓库约定

- 默认插件仓库：`https://github.com/mscl-team/mscl-plugins`
- 仓库地址格式：`https://github.com/owner/name`（后端严格校验）
- 仓库 Release 的 tag 作为版本号；`assets` 中的 `.zip` 作为更新包，缺失时回退到 `archive/refs/tags/<tag>.zip`
- 更新流程：下载校验 → 备份旧目录 → 独立助手在启动器关闭后替换 → 失败自动回滚

---

## 10. 校验规则速查

`validate_plugin_manifest` 会返回错误数组（空数组表示合法），逐项检查：

1. `id` 满足 ID 规则
2. `name` 非空
3. `schema <= 1`
4. `category ∈ {core, launcher, server}`
5. `layer ∈ {harness, module, feature}`
6. `capabilities` 每项在合法集合内
7. `hooks` 每项在合法集合内
8. `extensionPoints` 每项 `point` 合法且 `key` 非空
9. `dependencies` 每项 ID 合法
10. `configSchema` 每项有 `key`、合法 `type`、有 `default`

导入第三方插件前，可在 UI 里先调用"校验 manifest"确认无误。

---

## 11. 开发一个新插件（推荐流程）

1. 在 `plugins/` 下新建目录，目录名 = 插件 `id`（如 `my-feature/`）
2. 编写 `plugin.json`，按 §3–§7 填字段；至少给出 `id`、`name`、`category`、`layer`、`capabilities`
3. 如需配置，声明 `configSchema`，宿主会自动生成设置 UI 并读写 `<id>/config.json`
4. 用"校验 manifest"确认清单合法
5. 本地安装验证：设置 → 启动器插件 → 本地安装，选择该目录
6. 需要分发时，推到 `mscl-plugins` 仓库（或自己的 GitHub 仓库），通过"从 GitHub 安装"或仓库 Release 更新

### 拆分原则

遵循单一职责：新增功能时，优先判断能否并入现有插件；只有当职责明显独立（新的页面、新的资源大类、新的启动步骤、新的下载源）时才新建插件。内置清单已按 `宿主内核 / 页面模块 / 资源子模块 / 启动与下载 / 功能插件` 五组拆分，可作为粒度参考。
