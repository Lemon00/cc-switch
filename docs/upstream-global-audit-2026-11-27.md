# 上游合并后的全局代码审核

审核日期：2026-11-27。基于本次选择性移植后的工作区，继续检查五应用边界、迁移与保存入口、代理路由、网络与流式处理、MCP 导入、备份恢复、用量统计、会话读取、工具安装管理和前后端接口。合并范围见 [实施记录](upstream-merge-result-2026-11-27.md)。本轮修复仍留在工作区，未提交或推送。

## 确认并修复的问题

| 问题 | 影响与修复 |
| --- | --- |
| 旧配置重新引入退役应用 | `MultiAppConfig::load` 仍会补入 Gemini CLI；JSON → SQLite 迁移直接遍历任意应用键和服务卡。加载后只保留五个规范应用键，迁移层独立拒绝不支持的应用和服务。原 JSON 在更新前保留 `.bak`。 |
| 支持应用的迁移数据丢失 | 迁移遗漏 OpenCode、dsh、Antigravity 的 MCP 启用状态，以及 OpenCode 提示词和通用配置。补齐这些字段，并验证迁移后的实际数据。 |
| 列表与路由过滤不一致 | 故障转移队列和当前 OMO 配置会显示普通供应商读取已隐藏的旧服务。统一排除；损坏但可删除的队列条目仍保留，避免用户无法清理。 |
| 重命名与文件写入绕过校验 | `replace_provider_id` 直接执行 SQL，绕过保存校验；部分更新流程先写客户端文件再保存数据库。提取共享服务校验，在重命名及服务层写入前执行；拒绝后原供应商及端点关系保持不变。 |
| OpenCode 2.x 升级选错包 | Volta shim 无法从路径判断代际，静态兜底也可能安装 `opencode-ai`。Volta 按探测版本选 `@opencode/cli`，静态兜底结合版本和包路径选包；WSL 升级仅调用原生 upgrade，失败不再回退安装 1.x。 |
| 退役客户端残留接口和资源 | 删除 6 个无后端注册命令的 Pi 提示词 API 及其类型、Oh My Pi 图标、未引用的 OpenClaw/Hermes 图标、闲置 Gemini CLI 使用量处理器配置、Copilot warmup 辅助函数，以及错误页面指引和类型分支。 |

新增回归覆盖：旧配置反复加载及备份、迁移后原始数据库中无退役条目、五应用 MCP 标志和 OpenCode 配置保留、故障转移队列与损坏行处理、OMO 过滤、拒绝重命名后数据不变、服务层提前校验、退役深链接、OpenCode 2.x 的 Volta/Windows shim/静态兜底，以及 WSL 升级命令。

## 审核范围与保留内容

- 仅支持 Claude Code、Codex、OpenCode、DeepSeek Harness、Antigravity。保留服务于这些客户端的 Gemini 协议/API/OAuth 转换、通用模型与计价信息，以及读取历史统计和备份所需的兼容字段。
- 检查了路由版本捕获、手动切换后的过期请求回写限制、自动故障转移提交路径，以及公共 Codex Responses 工具结果处理，未发现需要继续修复的确定问题。
- 核对了 MCP 确认页与导入校验、SQL 与二进制恢复校验、CONNECT 读取上限、代理配置快照、自转发限制、SSE 事件上限及首尾用量保留、QuickJS 单次求值、OpenCode 2.4 会话正文定位、统计计算和 Skill 下载/归档边界。
- 静态扫描了前端 294 处字面量 `invoke` 调用；清理 Pi API 后，无未注册命令。此检查不替代运行时 IPC 和真实客户端验收。
- 未修改用户的 `.security/scan-config.json`，未操作用户外部客户端目录和历史备份。

## 本轮验证

已通过 `pnpm typecheck`、`pnpm build:renderer`、131 个本次改动 Rust 文件的 Rustfmt 检查和 `git diff --check`。110 个前端改动文件的 Prettier 最终检查全部通过。构建仍有已有的 bundle 大小、浏览器数据版本和 Tailwind 类名提示。

Rust 核心全量测试：3499 项通过、0 失败、9 项忽略。桌面层全量单元测试：536 项通过、0 失败。

CLI 全量测试：20 项单元测试及 2 项集成测试通过，包含真实后台代理进程的启停、出站代理生效、PID/端口释放，以及 TUI 非交互输入拒绝。

前端以 `--maxWorkers=2 --minWorkers=1` 完整执行：288 个测试文件、3327 项测试全部通过，退出码 0。本轮是全量通过结果，未用分组复验代替全量统计。Pi API 清理后也完成了类型检查；相关提示词测试在全量运行中通过。

验证日志位于本地 `.git/audit-core-tests.log`、`.git/audit-desktop-tests.log`、`.git/audit-cli-tests.log`、`.git/audit-frontend-tests.log`、`.git/audit-typecheck.log`、`.git/audit-renderer-build.log`、`.git/audit-prettier-final.log`、`.git/audit-rustfmt.log`，不纳入产品提交。

验证环境沿用仓库隔离的测试 HOME 和临时目录，以及 Windows GNU Rust 工具链。Linux/macOS、MSVC、实际安装升级、真实账号与上游端点、安装包构建和性能收益不在本轮现场验证范围内。
