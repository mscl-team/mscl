# MSCL Plugins

`mscl-team/mscl-plugins` 是 [MSCL 启动器](https://github.com/mscl-team/mscl) 的插件仓库，用于分发默认插件库与第三方插件。插件遵循 **Harness Plugin DSL v1**，主程序 `mscl-app.exe` 是宿主，插件通过 `plugin.json` 声明能力、钩子与扩展点。

## 仓库内容

每个插件一个独立目录，目录名与 manifest 的 `id` 一致：

```
mscl-plugins/
├── README.md
├── my-plugin/
│   ├── plugin.json      # 必需：插件清单
│   ├── config.json      # 可选：默认配置（键对应 configSchema）
│   └── ...              # 插件自有资源
└── another-plugin/
    └── plugin.json
```

`plugin.json` 的 `id` 规则：`^[a-z0-9-]{1,64}$`，只能使用小写字母、数字和短横线，长度 1–64。该 ID 同时作为安装后的目录名。

## 最小示例

```json
{
  "schema": 1,
  "id": "my-plugin",
  "name": "我的插件",
  "version": "1.0.0",
  "category": "launcher",
  "layer": "feature",
  "description": "插件说明",
  "author": "MSCL Team",
  "source": "github",
  "repo": "https://github.com/mscl-team/mscl-plugins",
  "capabilities": ["settings", "command"],
  "hooks": ["onLoad"],
  "extensionPoints": [
    { "point": "settings", "key": "my-plugin", "label": "我的设置" }
  ],
  "dependencies": [],
  "configSchema": [
    { "key": "enabledFeature", "label": "启用功能", "type": "boolean", "default": true }
  ],
  "order": 100
}
```

字段说明、枚举取值、校验规则和完整字段表见主仓库的 [PLUGIN_DSL.md](https://github.com/mscl-team/mscl/blob/main/PLUGIN_DSL.md) 与 [plugins.schema.json](https://github.com/mscl-team/mscl/blob/main/plugins.schema.json)。后端同时接受 camelCase 与 snake_case 写法的 `extensionPoints` / `configSchema`。

## 在启动器中使用

1. 打开启动器设置，把插件仓库地址设为 `https://github.com/mscl-team/mscl-plugins`。
2. 资源页 → **启动器插件**：可以看到内置插件清单中未安装的默认插件，点击安装即可（走内置清单，不依赖本仓库）。
3. 插件库整库更新：检测到新版本后点击“立即更新”，启动器会下载 zip、备份现有 `plugins/` 目录，再由独立助手在关闭后替换；失败自动回滚。

> 当前实现状态：资源页的“从 GitHub 仓库安装单个插件”是预留入口，后端会返回“仓库尚未部署，无法安装”的提示。可用的安装方式是**本地目录安装**、**内置默认插件安装**和**整库更新**。如果你要部署这个仓库以测试更新，请走下面的 release 流程。

## 发版与更新测试

启动器检查插件库更新时读取本仓库的 GitHub Releases API，取最新 release 的 tag、body 和资产列表。

### 1. 发布第一个可用版本

```bash
# 在本地本仓库目录
git add -A
git commit -m "feat: 初始化插件库"
git tag -a v1.1.0 -m "插件库 1.1.0"
git push origin main
git push origin v1.1.0
```

**tag 必须大于 `1.0.0`**：启动器启动时以 `1.0.0` 为基线做版本比较，等于或低于它的 tag 不会触发更新提示。`-` 开头的 tag（如 `v1.1.0-beta`）会被标记为 pre-release，仍可用于测试。

### 2. 资产约定

- 优先下载 release 中任意以 `.zip` 结尾的资产。
- 若没有 zip 资产，则回退使用 GitHub 自动生成的 tag source zip：`https://github.com/mscl-team/mscl-plugins/archive/refs/tags/<tag>.zip`。
- zip 解压后会递归查找（最多 3 层）含 `plugin.json` 的目录；仓库顶层多一层目录（GitHub archive 默认行为）不影响识别。
- release body 会作为 markdown 更新日志显示在启动器弹窗中，建议写清本版新增、修改与修复的插件。

### 3. 验证

1. 启动器设置中确认插件仓库地址为本仓库地址。
2. 重启启动器；若开启插件检查更新，应弹出 v1.1.0 的更新日志弹窗。
3. 点击“立即更新”，启动器关闭，更新助手下载并替换插件目录。
4. 重新打开启动器，在资源页确认插件已生效；检查游戏数据目录下没有残留 `plugins.backup-*` / `plugins.stage-*` / `plugins.old-*` 目录。
5. 若要测试回滚，可临时上传一个不含 `plugin.json` 的 zip 资产并再次发版，助手应拒绝替换并保留原插件目录。

## 相关文档

- 主仓库：https://github.com/mscl-team/mscl
- 插件 DSL 权威文档：https://github.com/mscl-team/mscl/blob/main/PLUGIN_DSL.md
- 机器可读 schema：https://github.com/mscl-team/mscl/blob/main/plugins.schema.json
