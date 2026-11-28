# CC Switch

面向多种 AI 编程客户端的桌面配置与管理工具，基于 React、TypeScript、Tauri 和 Rust。此仓库由 `Lemon00/cc-switch` 独立维护，版本从 **1.0.0** 重新开始。

[用户手册](docs/user-manual/zh/README.md) · [1.0.1 发布说明（待发布）](docs/release-notes/v1.0.1-zh.md) · [文档索引](docs/README.md) · [贡献指南](CONTRIBUTING.md)

## 功能

- **供应商管理**：预设和自定义配置、切换或多供应商共存、模型配置、原生配置同步。
- **本地路由**：协议转换、模型映射、故障转移和聚合模型目录；能力按客户端区分。
- **扩展管理**：MCP 配置与连通性检查、Skills 发现与同步、提示词及工作区文件编辑。
- **会话与用量**：会话浏览、搜索、恢复和部分客户端的删除；Token、费用、价格、汇率与配额查询。
- **桌面与数据**：托盘面板、轻量模式、深链接导入、备份恢复、WebDAV / S3 同步。
- **独立 CLI**：与桌面复用共享核心，支持终端中的配置管理和代理操作；运行 `cc-switch-cli tui` 可交互编辑供应商与 CLI 设置，见 [TUI 使用说明](docs/cli-tui.md)。

## 管理的客户端

| 类别 | 客户端 |
| --- | --- |
| 单供应商切换 / 专用接入 | Claude Code、Codex、Antigravity |
| 多供应商共存 | OpenCode、DeepSeek Harness |

应用列表、MCP、Skills、会话和用量入口仅提供以上五个客户端。本地路由与 Stack 面向 Claude Code 和 Codex。旧版本配置与数据库中的其他应用记录保留用于兼容读取，不再显示应用入口或自动扫描、导入。

Antigravity 的第三方 Key 卡通过卡片「打开终端」使用；CLI 会话支持删除，IDE、ACP 会话仍需在原应用处理。最新 main 修复与 Codex #8970 的处理见[本次更新说明](docs/release-notes/five-apps-2026-11-26.md)。

## 安装

从 [本仓库 Releases](https://github.com/Lemon00/cc-switch/releases) 获取已发布产物。当前工作流构建 Windows x64 和 Linux x64；可用文件以对应 Release 为准。其他平台需要自行构建，不沿用上游官网、Homebrew 或 AUR 包作为本独立版本的安装渠道。

旧 4.x 用户需手动安装本仓库已发布的独立版本。版本号降低不会触发旧版自动更新，安装前请备份配置；具体步骤见[安装指南](docs/user-manual/zh/1-getting-started/1.2-installation.md)。

## 开发

环境版本由 `.node-version`、`package.json` 的 `packageManager` 和 `rust-toolchain.toml` 指定。安装平台所需的 Tauri 依赖后，在仓库根目录运行：

```bash
pnpm install --frozen-lockfile
pnpm dev
```

前端测试：

```bash
pnpm test:unit
```

Rust 核心、桌面层和 CLI 使用三个独立 Cargo manifest。测试隔离、针对模块的命令及编译方法见[测试指南](docs/testing.md)；职责划分见[架构说明](docs/architecture.md)和[模块地图](PROJECT_MODULE_REVIEW_PLAN.md)。

## 来源与许可

本项目源自 [farion1231/cc-switch](https://github.com/farion1231/cc-switch)，感谢上游作者和贡献者。项目继续遵循 [MIT License](LICENSE)。当前变更见 [CHANGELOG](CHANGELOG.md)。本仓库的安装、更新和问题反馈独立维护。
