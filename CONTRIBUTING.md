# 贡献指南

本仓库从 1.0.0 开始独立维护。问题和 PR 提交到 `Lemon00/cc-switch`；参与前请阅读[行为准则](CODE_OF_CONDUCT.md)，安全问题按[安全报告说明](SECURITY.md)处理。

## 环境与启动

- Node.js 版本参考 `.node-version`，pnpm 版本由 `package.json` 的 `packageManager` 固定。
- Rust 工具链由 `rust-toolchain.toml` 固定。
- Windows 需要 WebView2 和对应编译工具；使用 GNU 宿主的环境见 [Windows GNU 构建](docs/windows-gnu-build.md)。macOS 需要 Xcode 命令行工具；Linux GUI 需要 GTK 3、WebKitGTK 4.1 等开发库，具体依赖见当前发布工作流。

在仓库根目录执行：

```bash
pnpm install --frozen-lockfile
pnpm dev
```

`pnpm dev` 启动 Vite 和 Tauri。只开发渲染层可以运行 `pnpm dev:renderer`，但浏览器中的 IPC 需要测试替身，不能代替真实桌面环境。

## 代码结构

React 主窗口和托盘面板通过 `src/lib/api/` 调用 Tauri。`src-tauri/` 负责桌面适配；`crates/cc-switch-core/` 保存通用业务、数据库和客户端原生配置；`crates/cc-switch-cli/` 为独立命令行入口。三个 Rust crate 有独立 manifest，根目录没有 Cargo workspace。

前端能力定义见 `src/config/appCapabilities.ts`，展示配置见 `appConfig.tsx`。读取快照和显式原生刷新分开，Query Key、缓存失效与事件清理由公共入口维护。职责和一致性限制见[架构说明](docs/architecture.md)，模块编号见[模块划分](PROJECT_MODULE_REVIEW_PLAN.md)。

## 验证

按改动范围选择检查，并在提交说明中列出实际结果：

```bash
pnpm test:unit
pnpm typecheck
pnpm format:check
pnpm build:renderer
cargo fmt --manifest-path crates/cc-switch-core/Cargo.toml --check
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo fmt --manifest-path crates/cc-switch-cli/Cargo.toml --check
```

Rust 测试须设置隔离的 `CC_SWITCH_TEST_HOME`；分别运行 core、桌面和 CLI 的测试，桌面 crate 的 `cargo test` 不会执行依赖库的单元测试。命令、模块 13 回归和集成测试入口见[测试指南](docs/testing.md)。`pnpm format:check` 只覆盖 `src/`；文档和新增测试文件需单独检查格式。

当前 CI 在 tag 推送后检查发布元数据并构建 Windows/Linux 产物，不是全量跨平台测试矩阵。单元测试、类型检查、编译和桌面实测应分开报告。

## 本地打包与发布资料

```bash
pnpm build
```

生成应用内更新产物需要 `TAURI_SIGNING_PRIVATE_KEY`。仅本地打包时可关闭更新产物（POSIX shell 示例）：

```bash
pnpm tauri build -c '{"bundle":{"createUpdaterArtifacts":false}}'
```

PowerShell 可把 JSON 写入文件，再使用 `-c <文件路径>`。发布时同步更新 `package.json`、Tauri 配置、三个 Cargo manifest 及锁文件中的本地包版本，并提供 `docs/release-notes/v<版本>-zh.md` 和 `src/whats-new/<版本>.json`。tag 使用 `v<版本>`，正文直接取自对应 Markdown；摘要规则见[更新摘要说明](src/whats-new/README.md)。

## 提交要求

- 描述具体问题、最终行为、验证方法及尚未验证的范围。提交尽量聚焦，不混入无关格式化或生成文件。
- 修改数据库结构时同时提供新建、升级和失败回滚的验证；不得只修改 schema 版本号。
- API 参数、Query 缓存、原生配置或设置字段变化时同步调用方、测试及文档。
- 使用已有 UI 组件和国际化入口。用户可见文本维护简中、繁中、英文和日文；土耳其语为社区翻译，可回退英文。保留翻译占位符。
- 供应商预设应有可维护的协议、端点和模型依据；未收录的供应商仍可通过自定义配置使用。新增预设先说明用途，不沿用上游的赞助申请渠道。
- AI 辅助贡献同样需要作者理解并审查代码、实际验证结果；不要把工具生成的“已通过”当作证据。
- 提交信息可使用 `fix:`、`feat:`、`docs:`、`test:` 等 Conventional Commits 前缀。

## English quick reference

This repository maintains an independent release line starting at 1.0.0. Use `Lemon00/cc-switch` for issues, pull requests and releases. Preserve upstream attribution and the MIT license.

Install dependencies with `pnpm install --frozen-lockfile`, then use `pnpm dev`. The desktop app, shared core and CLI use separate Cargo manifests. See the [architecture](docs/architecture.md) and [testing guide](docs/testing.md) for responsibilities, test isolation and commands. Report checks actually performed; the release workflow does not run a full test matrix.

Keep changes focused, explain observable behavior, maintain translations and documentation, and review AI-assisted contributions yourself. Version manifests, local lockfile entries, release notes and in-app summaries must agree before tagging a release.
