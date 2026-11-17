# 上游补充合入计划与进度（2026-11-17）

> 基线：`origin/main@09acca04`，起始 HEAD `2a666e91`（上一轮日志收尾 `3a0b86a2` 之后）。
> 方式：按上游拓扑顺序逐条 `cherry-pick -x`；冲突按本分支解耦结构手工解决。
> 进度：**已处理 1–59 条（共 59 条），剩余 0 条。**
> 当前 HEAD：`a32bce0d`。

## 范围

- 纳入：bug fix、代码/性能优化、重构与清理、配置修正。
- 排除：feat、新模块/新接口、功能性新依赖、语言包、发布说明、Copilot/WSL 不适用栈、文件缺失项。
- 用户已确认纳入的边界项：`9bc68243`、`4376e5df`、`1af6aaa6`、`7dec90e9`、`c70d61de`。
- 纯文档与纯测试修正本轮不纳入。

## 已完成（1–29）

| 序号 | 上游客 | 分类 | 方式 | 结果 |
| --- | --- | --- | --- | --- |
| 1 | `e7878471` fix(proxy): stop classifier retry loops | 修改 | cherry-pick + 手工解冲突 | 提交 `ad8aa128`（HEAD 已有实现，仅补 `outbound_model`） |
| 2 | `c70d61de` fix(linux): bump @tauri-apps/cli | 修改 | cherry-pick + 手工解冲突 | 提交 `536b4284`（只取 `package.json`/`pnpm-lock`；`release.yml` 保留本分支 deb 逻辑） |
| 3 | `98777515` fix(ui): DB 等锁命令移出 UI 线程 | 修改 | cherry-pick + 冲突核对 | 空补丁，HEAD 已实现，跳过 |
| 4 | `fc9b17cb` perf(db): batch provider endpoint reads | 性能 | cherry-pick + 冲突核对 | 空补丁，跳过 |
| 5 | `80ad02c5` fix(proxy): log mapped upstream model | 修改 | cherry-pick + 冲突核对 | 空补丁，跳过 |
| 6 | `883382f1` fix(proxy): name provider in circuit breaker | 修改 | cherry-pick + 冲突核对 | 空补丁，跳过 |
| 7 | `62de0f3e` build: drop unused staticlib/cdylib | 配置 | cherry-pick | 提交 `91e8288e` |
| 8 | `49f2882e` fix(proxy): Kimi tool-call history | 修改 | cherry-pick + 冲突核对 | 空补丁（HEAD 为 Kimi+DeepSeek 超集），跳过 |
| 9 | `8ff9e3b5` fix(proxy): strip ChatGPT identity headers | 修改 | cherry-pick + 冲突核对 | 空补丁，跳过 |
| 10 | `e9285cf1` fix(proxy): map model per failover target | 修改 | cherry-pick + 冲突核对 | 空补丁（HEAD 已有 `maps_codex_failover_model`），跳过 |
| 11 | `479e632a` fix(tray): macOS tray tooltip | 修改 | cherry-pick | 提交 `ed6ec069` |
| 12 | `cf341a8a` fix(claude): SUBAGENT_MODEL_FORCE env | 修改 | cherry-pick | 提交 `2175ab79` |
| 13 | `4376e5df` perf(codex): rollout 解析优化 | 性能 | cherry-pick + 手工解冲突 | 提交 `f7f50231`（保留 HEAD 测试条件编译） |
| 14 | `60a68363` chore(clippy): test-code lints | 清理 | cherry-pick + 手工解冲突 | 提交 `db51669c`（只取 `dsh_config` 修正，跳过 ru/ko 纯测试） |
| 15 | `4151bc33` fix(proxy): Claude 密文整流重试 | 修改 | cherry-pick | 提交 `5fa677b2` |
| 16 | `fd979421` fix(proxy): Gemini tool results 合并 | 修改 | cherry-pick | 提交 `01ae67fe` |
| 17 | `0f593e9f` fix(sidebar): 底栏按文字分宽度 | 修改 | cherry-pick + 手工解冲突 | 提交 `50039814`（保留本分支 `h-7`） |
| 18 | `cf319bcf` fix(dsh): 自定义模型兼容编辑器 | 修改 | cherry-pick | 提交 `27763a1b` |
| 19 | `063c22d8` fix(tray): macOS 全屏快捷面板 | 修改 | cherry-pick | 提交 `6d119aa6` |
| 20 | `f6423b5d` fix(codex): keep-official-login 开关 | 修改 | cherry-pick + 手工解冲突 | 提交 `24257738` |
| 21 | `f1d438e0` fix(codex): 镜像官方行显示名 | 修改 | cherry-pick + 手工解冲突 | 提交 `8f5bd885` |
| 22 | `6463262c` fix(apps): macOS Claude.app 安装判定 | 修改 | cherry-pick | 提交 `529acb8d` |
| 23 | `3706cac3` fix(clipboard): Linux WebView 剪贴板 | 修改 | cherry-pick | 提交 `2afd37ec` |
| 24 | `457f898c` fix(usage): Codex fork 序号导入 | 修改 | cherry-pick | 提交 `d0727b82` |
| 25 | `86aeb47a` fix(skills): symlink 失败带 OS 错误 | 修改 | cherry-pick | 提交 `c9da36f5` |
| 26 | `da830bfd` fix(proxy): Gemini Native 系统消息位置 | 修改 | cherry-pick | 提交 `d24aeb98` |
| 27 | `e91114fe` fix(proxy): Gemini Native usage 日志 | 修改 | cherry-pick | 提交 `0e30f8c4` |
| 28 | `86c749b9` fix(proxy): Copilot Responses 路由 | 修改 | cherry-pick | 提交 `757d208a` |
| 29 | `7dec90e9` chore: remove unused code/deps/files | 清理 | cherry-pick + 手工解冲突 | 提交 `0d42fc57`（29 文件，-836/+2；`Cargo.toml`/`Cargo.lock` 保留本分支结构，跳过 privacy_filter 命令） |

小计：22 个实际提交，7 条空补丁跳过。

## 已完成（30–59）

| 序号 | 上游客 | 分类 | 方式 | 结果 |
| --- | --- | --- | --- | --- |
| 30 | `8c76bfe1` fix(database): 连接锁 panic 恢复 | 修改 | cherry-pick + 手工解冲突 | 提交 `c103ac4d`（只取锁恢复测试，跳过 Copilot/WorkBuddy 迁移测试） |
| 31 | `24eb40c6` fix(proxy): Codex 密文整流 | 修改 | cherry-pick | 提交 `d42ab466` |
| 32 | `add1c5ab` fix(openclaw): 注册表单模型 | 修改 | cherry-pick | 提交 `95ceb4c6`（含测试文件） |
| 33 | `3795de7e` fix(codex): session 文件 mtime | 修改 | cherry-pick | 提交 `e1f3e26c` |
| 34 | `e9bb11fa` fix(hermes): 删除 Hermes 未定义 provider | 修改 | cherry-pick + 手工解冲突 | 提交 `13f8ca64`（保留本分支 `isPiFamilyApp`） |
| 35 | `70b95694` fix(usage): locale 兜底 | 修改 | cherry-pick + 手工解冲突 | 提交 `791e0af5` |
| 36 | `c0f640cf` fix(terminal): 短窗口确认按钮可达 | 修改 | cherry-pick | 提交 `d8124a7f` |
| 37 | `5494a765` fix(codex): Mistral Chat 字段 | 修改 | cherry-pick | 提交 `5ab1bf0b` |
| 38 | `f72c3048` fix(codex): Mistral reasoning_content | 修改 | cherry-pick + 手工解冲突 | 提交 `e655c61c`（导出改到 core `providers/mod.rs`，丢弃复活的 `src-tauri` 路径） |
| 39 | `1af6aaa6` refactor(presets): 预设模型表去重 | 重构 | cherry-pick | 提交 `6f2355e3` |
| 40 | `ce967158` fix(apps): mise shim 计入安装 | 修改 | cherry-pick | 提交 `b066c821` |
| 41 | `7269b51f` fix(coding-plan): commandcode.ai/providers 识别 | 修改 | cherry-pick | 提交 `3003e69e` |
| 42 | `8b8518bf` fix(presets): PIPELLM 等模型名 | 配置 | cherry-pick | 提交 `f5f4bdef`（预期 CONFLICT，实际 CLEAN） |
| 43 | `b2b0f264` fix(sync): 自动同步 worker / S3 锁 | 修改 | cherry-pick + 手工解冲突 | 提交 `5558b4e9`（`auto_sync` 落到 core，用 `EventSink`/`spawn_background_worker`；保留 `run_with_sync_lock` 重导出供 commands） |
| 44 | `eb8c190a` fix(model-fetch): Gemini API-key 头 | 修改 | cherry-pick + 手工解冲突 | 提交 `06c70e62` |
| 45 | `d5dfa830` fix(proxy): 工具 schema null required | 修改 | cherry-pick | 提交 `ee9d6b19` |
| 46 | `6489c5d8` fix(env): Unix 路径冒号 | 修改 | cherry-pick | 提交 `127b233c` |
| 47 | `85eddc46` fix(models): 默认 User-Agent | 修改 | cherry-pick | 提交 `9b16eb6e` |
| 48 | `9bc68243` fix(claude): env 空值墓碑 | 修改 | cherry-pick | 提交 `8bafdc42` |
| 49 | `d3970527` fix(webdav): MKCOL 400 容错 | 修改 | cherry-pick | 提交 `c1aaa3a3` |
| 50 | `a062485a` fix(xai): OAuth 刷新错误传播 | 修改 | cherry-pick | 提交 `d02d1dd5` |
| 51 | `168556b4` fix(xai): 默认 OAuth 账号保留 | 修改 | cherry-pick | 提交 `866a3a47` |
| 52 | `684c388a` fix(skills): 更新先暂存 | 修改 | cherry-pick + 手工解冲突 | 提交 `36427941`（保留本分支 app 集合，不带 Copilot/WorkBuddy） |
| 53 | `0ec969ca` fix(sync): WebDAV/S3 失败真实原因 | 修改 | cherry-pick | 提交 `a8d9fa03`（预期 CONFLICT，实际 CLEAN） |
| 54 | `7d8ed536` fix(settings): 持久化 S3 预设 | 修改 | cherry-pick + 手工解冲突 | 提交 `1e718a7f`（只取 `preset` 字段与测试，跳过 poison/ru/ko 测试） |
| 55 | `373c223f` fix(gemini): 原生格式取模型 | 修改 | cherry-pick | 提交 `51dbf13b` |
| 56 | `3369fe89` chore(copilot): 模型 ID/SKU 日志 | 清理 | cherry-pick | 提交 `de3631e4` |
| 57 | `98d2781c` fix(s3): 阿里云/腾讯云/华为云虚拟主机 URL | 修改 | cherry-pick | 提交 `a249f971` |
| 58 | `65cd7ac8` fix(settings): Hermes 目录同步 providers | 修改 | cherry-pick | 提交 `67d8d919` |
| 59 | `09acca04` build: line-tables-only debug info | 配置 | cherry-pick + 部分落地 | 提交 `a32bce0d`（只取 `src-tauri/Cargo.toml`，跳过不存在的 `ci.yml`） |

小计：30 个实际提交，0 条跳过。

## 本轮合计

- 计划 59 条；空补丁跳过 7 条；实际提交 **52** 个（前 22 + 后 30）。
- 当前 HEAD：`a32bce0d`（相对上一断点 `0d42fc57` 前进 30 个提交）。

## 明确未纳入

- 全部 feat（WorkBuddy、privacy filter、后台更新、多语言、IP 诊断、Ollama Cloud、agy Windows 切换、会话折叠、Skills Resync、错误码筛选等）。
- 新模块重构：`1863149f` AppSpec、`56c5d54d` ProviderKind、`3630d9de` sync_support、`e30ef18d` terminal_launch、`3bd8b457` ToolSpec、`66b922fc` tool_process。
- 新计价能力：`f37b4ba2`、`db09fcb8`。
- 语言包/多语言 README：`3dfc92c6`、`bcb29997`、`3cf78371`、`c0a8d87b`、`51a96e24`。
- 版本发布与 release notes：`8c05b622`、`2aeb27d8`、`3321cb9e`、`cb187a11`、`609e7914`、`c3cf24ca`、`daccf6fa`、`3f074eac` 及对应说明。
- 文件缺失/不适用：`7afceec1`、`0bb502cb`、`b3594905`、`c65a523c`、`c6a5a789`、`07798d56`、`5c462269`、`761b4c02`。
- Copilot/WSL 特定栈：`17e20a59`、`4d68975b`、`cf12f010`、`23c57e8d`、`36041067`、`8dc7c704`、`1c1346d0`、`fc3eb797`、`db7a0e75`。
- 依赖新功能的配套：`61bd4d42`、`2cbfa8c3`、`b72af3d4`、`57d17960`。
- 预设文件不存在：`0a77d828`、`242abf78`、`2360631f`。
- 纯文档/纯测试（用户本轮不纳入）：`d300a120`、`27e2cffa`、`fd7497fc` 等。

## 验证状态

- core `cargo check`：通过（GNU 目标，仅既有/新增 dead_code 与 deprecated 警告）。
- core `cargo check --tests`：通过。
- 前端 `pnpm typecheck`：通过。
- 运行时测试未执行：core `cargo test` 因本机 GNU 运行时链接失败；GUI `cargo check --lib` 缺 `windres`；前端 vitest 的 esbuild 0.21.5 崩溃。需在 CI 复核。
- 复查修复（2026-11-17）：补齐 `copilot_auth.rs` 的 `access_type_sku` 字段、移除 `codex_config.rs` 中无宿主变量的 `official_speed_tiers` 更新、为 `transform_codex_chat.rs` 测试调用补 `preserve_reasoning_content` 参数。
- 内容核对：59 条计划中 55 条新增行完整匹配；4 条差异均为有意适配（`49f2882e` HEAD 超集、`60a68363` 跳过 ru/ko 纯测试、`7dec90e9` 保留 core 的 `tower-http` 依赖、`b2b0f264` 的 `auto_sync` 落到 core）。

## 断点与恢复素材

- 当前 HEAD：`a32bce0d`
- 下一条：无（59/59 已完成）
- 还原前完整补丁：`.audit/staged.patch`
- 还原前备份 commit：`c62d9c2c25e5099241499288788a29444c535946`
- 机器可读计划：`.audit/merge-plan.txt`
- 全量矩阵：`.audit/remaining-matrix.txt`
