# `decouple-core` 上游同步与问题审计（2026-11-11）

## 范围

- 当前分支：`decouple-core`，基于前后端分离结构，并包含 `cc-switch-cli`。
- 上游基线：`origin/main@abf50589`。
- 规则：只同步 bug 修复、性能优化，以及这些修复直接依赖的代码；跳过功能、版本发布、纯文档、语言包和 WSL 专用测试适配。
- 工作区已有的 `.audit/` 和旧审计文档未纳入提交。

## 本次已同步

以下上游修复已经按当前目录结构合入，涉及 `src-tauri/src` 的代码已迁移到 `crates/cc-switch-core/src`：

- AGY 配额接口、Ultra 5x/20x 识别、区域无资格提示：`76e19b47`、`abf50589`。
- Responses 无 call-less 输出归一化、Mistral 工具历史、Codex Anthropic 默认 `max_tokens=32000`：`72e826ed`、`5e9812e0`、`3bde8262`。
- 第三方 Codex 清理私有 include 标记：`a6200c1e`。
- 官方 Codex 路由转发官方模型目录：`e38a4fd1`，落在 `crates/cc-switch-core/src/proxy/codex_models_passthrough.rs`。
- Windows Copilot CLI 检测、Omo-Slim 重渲染、Claude 账号凭据归属校验：`f38a612c`、`af791fd0`、`83d744b9`。
- 路由卡终端按钮、What's New 对话框稳定性、Copilot 模型列表错误提示：`5d961a3a`、`f16a9d74`、`aab0ea57`。
- 另有一个解耦适配提交 `3b6b47d9`，恢复 `terminalNeedsRoute` 并保留当前分支的 Grok Build 行为。

## 明确跳过

- Copilot BYOK/CLI/Deep Link/账号管理等功能及其配套修复：依赖当前分支没有引入的 Copilot 新功能，拆合会形成半套路径。
- `5c462269`：只针对桌面 Codex 在 WSL 下运行；当前构建环境是原生 Windows D 盘。
- `17e20a59`、`cf12f010`、`4d68975b`：依赖未合入的 Copilot 表单或 providers.json 新流程。
- 俄语/韩语语言包、版本号、发布说明、隐私过滤等功能或文档提交。
- WSL/9P 测试目录和 UNC 专用配套提交；当前环境不使用 WSL。

## 当前风险与问题

### P1：同步策略需要保持提交映射表

当前分支与 `main` 共享祖先后的文件路径大量发生重命名。直接 cherry-pick 后容易出现“提交成功但落在旧 `src-tauri` 路径”的风险。后续每次同步都要检查 `git status`、`git diff --name-status`，并确认新增模块位于 `crates/cc-switch-core`。

### P1：官方模型目录转发需要运行时回归

`e38a4fd1` 会让官方 Codex 路由把 `GET /models` 转到官方上游，依赖当前路由识别和官方 OAuth 头部。建议验证官方登录、第三方路由、无官方登录三种情况，确认不会把官方模型目录提供给第三方上游。

### P2：CLI 与 GUI 的进程和数据库边界

CLI 守护代理和 GUI 可能同时使用同一配置目录、端口和 SQLite。当前 CLI 已通过 pid 文件和启动等待降低风险，但 GUI 已运行时仍可能出现端口占用；应继续保留 `proxy enable` 的错误提示和日志开关。

### P2：Rust 编译验证受本机工具链限制

本次 `cargo check` 未完成，因为当前环境缺少 MSVC `link.exe`。前端 `pnpm typecheck` 已通过。安装 Visual Studio Build Tools 的 C++ 工具链后，应重新执行 core、CLI、GUI 的 Windows 检查。

### P2：版本号和上游发布节奏

当前分支版本仍为 `4.0.27`，上游已经进入 `v4.0.29` 之后的提交。由于本分支只同步修复，不自动同步发布提交，发布前需要单独确认版本号、变更日志和安装包元数据。

## 结论

本次同步已完成可独立落地的 bug 修复和性能相关修复，未引入上游 Copilot 新功能、隐私过滤或 WSL 专用逻辑。当前主要剩余工作是安装 Windows Rust 编译工具链后完成构建验证，并对官方 Codex 模型目录转发做真实请求回归。
