# 上游同步记录（decouple-core）

> 记录 `decouple-core` 分支从上游 `farion1231/cc-switch` 同步的事实。
> 上游基线在本地 `cc-switch-source` 仓库核对（其 `origin` 就是上游，`main` 与上游同步）。
> 同步步骤、冲突归位原则、验证清单见 [decouple-rebase-handbook-zh.md](./decouple-rebase-handbook-zh.md)。

## 记录口径

- 「合并」按版本区间记：一条记录 = 把一个上游版本区间的改动搬进本分支。
  实际操作是逐条 cherry-pick 加解耦树适配，不是 `git merge`，所以以覆盖到的上游 commit 为准。
- 每次同步收尾后补一行：日期、同步范围、分支 HEAD、同步到上游客、规模、备注。
- commit 用短 hash。上游改动落在 `src-tauri/src/xxx.rs` 时归位到
  `crates/cc-switch-core/src/xxx.rs`，不在 GUI 侧复活旧文件。
- 无 merge 冲突时也要写明（例如本轮是逐条 cherry-pick），并列出解耦树上的适配提交。

## 同步记录

| 日期 | 同步范围 | 分支 HEAD | 同步到上游客 | 规模 | 备注 |
| --- | --- | --- | --- | --- | --- |
| 2026-11-04 ~ 11-06 | v4.0.25 → v4.0.26 | `c1e56f4e` | `566448f0` | 4 + 29 提交 / 47 + 111 文件 | 分两批，冲突 3 处，见下 |
| 2026-11-09 | v4.0.26 → v4.0.27 | `8ee8536c` | `cac921f5` | 30 提交 / 65 文件 | 版本号升到 4.0.27，无 merge 冲突，见下 |
| 2026-11-08 ~ 11-11 | fix 批次（v4.0.28 → v4.0.29 区间） | `3b6b47d9` | `abf50589` | 15 提交 / 31 文件 | 逐条 cherry-pick，跳过 feat / 发布 / WSL，见下 |
| 2026-11-12 | `abf50589` → `3a0b86a2`（v4.0.30 发布前） | `f0a1566f` | `3a0b86a2` | 17 提交分析 / 合入 9 条 | 版本仍是 4.0.28，cherry-pick + core 补丁归位，见下 |
| 2026-11-17 | `3a0b86a2` → `09acca04`（v4.0.30 之后 fix/优化/清理批次） | `a32bce0d` | `09acca04` | 59 条计划 / 52 提交 + 7 空补丁 / 115 文件 | 全量矩阵筛选 + 逐条 cherry-pick，见下 |
| 2026-11-20 | `09acca04` → `bd3d3880`（fix + 功能重构） | `e4492f09` | `bd3d3880` | 101 条分析 / 合入 43 + 适配 1 / SKIP 58 | 含功能重构；大拆分与调用图清死代码跳过，见下 |

> 「分支 HEAD」列记的是每轮**同步代码收尾时**的 HEAD，不含随后的文档与复查修正提交。
> 第四轮的代码收尾是 `f0a1566f`，之后还有 `c89a3228`（同步记录 / What's New）与
> `705af7bc` 等复查修正提交，因此该列不等于任何时候的仓库当前 HEAD。
> 第六轮代码收尾是 `e4492f09`，之后还有 `8dd02d62`（补 `sync_utils`）等适配提交。
>
> 上游原版 clone（`cc-switch-source`）的 `main` 停在 `cdca0984`。第三轮的基线是本仓库
> `origin/main@abf50589`——先把上游主线同步进 fork 的 `main`，再从那里挑 fix，两边基线
> 口径不同，记录时以本轮实际取用的 `origin/main` 短 hash 为准。

### 第一次：v4.0.25 → v4.0.26

上游区间 `a3eafa40`（v4.0.25）→ `f621d421`（v4.0.26），并额外含 `566448f0`
（v4.0.26 之后的首个 fix，本轮实际同步到这里）。

**2026-11-04**（手册原记录）：合并 4 个提交（`perf(proxy) #9161` 即 `e217bfcb`、codex 并发、
presets 默认模型、macOS 签名重试），47 文件。冲突 3 处：

- `proxy/forwarder.rs`：`ActiveConnectionGuard::acquire` 加 `last_request_at`，取 main 实现并保持 `pub(crate)`；
- `proxy/json_canonical.rs`：取 main 的 `CanonicalValue` / `sort_all_objects`，core 侧保持 `pub`；
- `proxy/sse.rs`：`strip_sse_field` 取 main 实现；`take_sse_block` 被 `SseBlocks` / `SseBuffer` 取代且无调用方，删除。

合并后对 `crates/**` 全量 rustfmt，并打开 CI 的 core fmt 门禁。

> 原记录的分支 HEAD `abbeb8de` 在当前仓库已不存在，`e217bfcb` 也不是 `decouple-core` 的祖先。
> 这里作为历史记录原样保留，不作为可 checkout 的 commit。

**2026-11-06**（`c1e56f4e`，`merge(upstream): sync 29 fix/perf commits onto v4.0.25`）：
按上游顺序 cherry-pick 29 个 fix/perf 提交，111 文件（+5595 / -913）。

- 跳过：3 个已含补丁（`51fce968`、`b8fa4199`、`e217bfcb`）、2 个 macOS/AppImage release 修复
  （`7afceec1`、`c70d61de`）、9 个 feat（含新 provider imports）、v4.0.26 版本号与 release docs。
- core 侧适配：新增的 `classifier_guard` / `copilot_responses` / `providers_tests` 落到
  `crates/cc-switch-core/src/...` 并注册为 `pub mod`；`backup.rs` 采用上游拆分并公开
  `periodic_backup_if_needed`；`usage_events.rs` 保留 `std::thread` debounce（core 不依赖 tauri）；
  `usage/logger.rs` 保留 CLI usage-recording switch。

### 第二次：合并到 v4.0.27

**2026-11-09**，结束于 `8ee8536c`（`chore(release): v4.0.27`），30 提交 / 65 文件（+2119 / -282）。
本分支版本号升到 4.0.27。

- 上游区间 `f621d421`（v4.0.26）→ `cac921f5`（v4.0.28 发布前最后一个 fix）；跳过 v4.0.28
  的 feat 与发布提交。
- 30 个提交里 28 个与上游一一对应（含 `2aeb27d8 chore(release): v4.0.27` → `8ee8536c`），
  逐条 cherry-pick，**无 merge 冲突**。
- 另 2 个是解耦树适配：
  - `55d2ffd9` — WSL 卸载路径的 `Command` 保持 `mut`；
  - `e5424930` — catalog-pointer 断言对齐 `13af7fc9` 写入的绝对路径。
- 归位分布：`crates/cc-switch-core/src/` 22 个文件（config / proxy / database / services /
  live / mode 等），`src-tauri/src/` 4 个（GUI 命令层），前端与测试文件按上游位置保留。

### 第三次：同步 `origin/main` 的 fix 批次

**2026-11-08 ~ 11-11**，从 `7c5bbaa2` 到 `3b6b47d9`，15 个提交 / 31 文件
（+1664 / -80）。本分支版本号仍是 `4.0.27`，没有跟随上游 v4.0.28 / v4.0.29 的发布提交。

- 基线：本仓库 `origin/main@abf50589`（`fix(agy) #9414`）。这一轮先把上游主线同步进
  fork 的 `main`，再从那里挑 fix；上游原版 clone（`cc-switch-source`）的 `main` 仍停在
  `cdca0984`。覆盖区间约为 `cac921f5` 之后到 `abf50589`。
- 15 个提交里 13 个与 `origin/main` 的 fix 一一对应（逐条 cherry-pick，**无 merge 冲突**），
  2 个是解耦适配：`9fd7c56c`（保留路由终端可用性规则）、`3b6b47d9`（恢复
  `terminalNeedsRoute` 并保留本分支的 Grok Build 行为）。
- 提交映射（本分支 → `origin/main`）：

  | 本分支 | origin/main | 主题 |
  | --- | --- | --- |
  | `f09f5079` | `76e19b47` | AGY 配额接口与 Ultra 5x/20x 识别 |
  | `6112a1b6` | `72e826ed` | Responses 无 call-less 输出归一化 |
  | `f26c52e8` | `5e9812e0` | Mistral 工具历史省略 `reasoning_content` |
  | `1d45bb15` | `3bde8262` | Codex→Anthropic 默认 `max_tokens=32000` |
  | `c9880044` | `a6200c1e` | 第三方 Codex 清理私有 include 标记 |
  | `82fe1663` | `abf50589` | AGY 配额传 `cloudaicompanionProject` / 区域无资格 |
  | `6982a3cb` | `f38a612c` | Windows Copilot CLI 检测跳过 shim |
  | `1488895b` | `af791fd0` | Omo-Slim 无限重渲染 |
  | `d1c4b199` | `83d744b9` | Claude 账号凭据归属校验 |
  | `cd9d41da` | `5d961a3a` | 路由卡禁用终端按钮 |
  | `b228b36c` | `f16a9d74` | What's New 对话框稳定性 |
  | `be1857ca` | `e38a4fd1` | 官方 Codex 路由转发官方模型目录 |
  | `72a62801` | `aab0ea57` | Copilot 空模型列表提示 |
- 归位分布：`crates/cc-switch-core/src/` 10 个（proxy / services / claude_account 等），
  `src/components/` 10、`src/i18n/` 5、`tests/components/` 3、`src-tauri/src/` 2、
  `src/types.ts` 1。新增的 `proxy/codex_models_passthrough.rs` 落在 core 侧，
  未在 GUI 侧复活旧文件。
- 跳过：Copilot BYOK / Deep Link / 账号管理系列 feat、`32365a54` 隐私过滤 feat、
  `5c462269`（桌面 Codex 跑在 WSL 的专用修复）、依赖未合入 Copilot 表单的
  `cf12f010` / `4d68975b`、语言包、v4.0.28 / v4.0.29 版本号与发布说明、纯文档与
  测试配套提交。完整清单见
  [upstream-sync-audit-2026-11-11-zh.md](./upstream-sync-audit-2026-11-11-zh.md)。
- 已知问题：`be1857ca` 引入的 `codex_models_passthrough.rs` 调用
  `crate::settings::codex_proxy_without_official_login()`，但该提交只改了 proxy 目录下的
  文件，core 的 `settings.rs` 既没有这个 getter 也没有对应字段（上游
  `src-tauri/src/settings.rs` 有字段与 getter），干净 HEAD 会在 core 侧报 E0425。
  本轮未处理，第四轮之后以独立提交 `705af7bc` 补齐字段、默认值与 getter，
  `#[serde(default)]` 保证旧配置文件兼容。

### 第四次：同步 `origin/main` 的 fix 批次（v4.0.30 之前）

**2026-11-12**，从 `abf50589` 到 `3a0b86a2`（v4.0.30 发布提交的前一条），区间内
17 个提交，合入 9 条。本分支版本号仍是 `4.0.28`，没有跟随上游 v4.0.30 的版本号与
发布说明。取舍清单见 [upstream-sync-plan-2026-11-12-zh.md](./upstream-sync-plan-2026-11-12-zh.md)。

- 9 条 fix/perf 分两种方式落地：前端 / GUI 路径用 `git cherry-pick -n`；core 侧文件与
  上游 `src-tauri/src` 同名文件无共同祖先，三方合并用不上，改为按路径重写补丁
  （`git diff` 改路径后 `git apply --reject`）并手工收拾 rejected 分块。
  **上游提交彼此之间没有冲突**：`74f0d41c` 与 `08ec82aa` 同改 `forwarder.rs`，
  按上游顺序应用时两轮都干净落地。真正的 3 处冲突全部来自解耦树的既有分叉
  （缺 `officialSpeedTiers`、AGY cursor 的结构性重写、无套餐后缀的 ProviderCard）。
- 提交映射（本分支 → `origin/main`）：

  | 本分支 | origin/main | 主题 | 归位 |
  | --- | --- | --- | --- |
  | `93798b8c` | `d0e47b2d` | 供应商编辑器打开不再卡顿 | 纯前端 |
  | `d6b66e37` | `7d5fb657` | Gemini 流式模型归属 / Vertex 字符串 token | core `proxy/` |
  | `c1eeeaa6` | `16092b97` | AGY transcript 同步游标 | core `services/` + `session_manager/` |
  | `ab986077` | `74f0d41c` | 回放工具参数重复 JSON key 去重 | core `proxy/providers/` |
  | `9745da59` | `08ec82aa` | 第三方 Responses 剥离 OpenAI-only 字段 | core `proxy/` |
  | `c2955d79` | `5628bb11` | 主窗口启动收进显示器工作区 | GUI |
  | `61f0a9b5` | `1a98613e` | agy 1.5 无 `id_token` 钥匙串凭证 / 切号刷新配额 | GUI `services/agy_account/` |
  | `385337be` | `d4647fbb` | 技能元数据刷新移出列表查询 | core `services/skill.rs` + GUI + 前端 |
  | `f0a1566f` | `2abcaa6e` | Codex OAuth 卡片显示账号身份 | 纯前端 |

- 解耦适配提交：`8a3d334f` — `AgyAccountManager::quota()` / `live_quota()` 在本分支
  返回 `Vec<QuotaTier>`，实现里却直接返回 `quota::fetch()` 的 `AccountQuota`，干净 HEAD
  本来编译不过；改为在调用点取 `.tiers`。同一批复查还提交 `705af7bc` 补齐
  `settings.rs` 的 E0425 字段 / 默认值 / getter。
- **已知缺口（复查发现，未处理）**：`8a3d334f` 取 `.tiers` 会丢掉 `AccountQuota.plan`。
  上游 `origin/main` 的 `agy_account_quota` 会把它填进 `SubscriptionQuota.plan_name`，
  本分支 `crates/cc-switch-core/src/services/subscription.rs` 的 `SubscriptionQuota`
  根本没有这个字段，前端也不消费套餐名，因此第三次记录里宣称已同步的
  「Ultra 5x/20x 识别」（`f09f5079` / #9319）实际未落地。补齐属于功能补全，
  超出本轮「只合 fix/perf」范围，留待单独确认；`src/whats-new/4.0.28.json`
  已把「识别 Ultra 5x/20x」改为实际已落地的「按项目查询」。
- 手工解冲突的三处：`useCodexConfigState.ts`（丢掉上游 `officialSpeedTiers` 那一段，
  本分支没有该功能）、`session_usage_agy.rs`（`sync_agy_usage` 结构性重写，保留本分支的
  `pub mod native_models` 与 `include_str!` 相对深度）、`ProviderCard.tsx` +
  其测试（本分支已无登录后的套餐后缀，测试里 `· Plus` 相关断言一并去掉）。
- 跳过 8 条：`859ffa10`（历史发布说明页脚链接）、`49a57446` / `210103c6`（feat 定价种子）、
  `f6c7eed0`（CONTRIBUTING）、`4a1369f2`（copilot 卡片分组重构，依赖未合入功能）、
  `23c57e8d`（只改 `src-tauri/src/copilot_config.rs`，本分支无此文件）、
  `609e7914` / `3a0b86a2`（v4.0.30 版本号与发布说明）。
- 验证：core `cargo test --lib` 3918 passed / 2 failed（两个 symlink 测试因本机缺创建
  符号链接权限，`os error 1314`，与改动无关）；GUI `cargo check --all-targets` 通过
  （GNU 目标，`TMP` 需指向工作区内目录，否则 gcc 建不了临时文件）；GUI 单测因
  `tauri-build` 给 GNU 链接器塞 MSVC 的 `/MANIFEST:EMBED` 参数，本机无法链接，
  只做到编译级验证；前端 `pnpm typecheck` 通过，相关 vitest 用例（ProviderCard、
  Skills、ProviderForm、EditProviderDialog 等）全绿。
- 前端全量的噪声已定性：默认并发 3555 passed / 53 failed；改 4 workers 后两次复跑分别
  3568 passed / 44 failed 与 3596 passed / 16 failed，同一并发设置下结果都不稳定，
  失败全为 `Test timed out in 5000ms`，集中在 `PiProviderForm` 等慢组件。最集中的
  `PiProviderForm.test.tsx` 单独跑仍 4 failed / 51 passed，但在**同步前基线**
  `6096f37b` 上单独跑是 13 failed / 42 passed——属于本机既有的慢测试，不是本轮回归，
  需在 CI 上用正常机器复核。
- 复查后补的测试：`editProviderStore.test.tsx`（store 的开/换/关/退订与 hook 稳定性）、
  `useRefreshLocalSkillMetadata` 的命令失败路径、`ProviderCard.codexAccount` 的
  `enabled: isCodexOauth` 负向断言。
- 复查后仍未补的验证：core `forwarder.rs` 里 dedupe / strip 的门控组合没有直接单测；
  agy 1.5 与 `window_fit` 的 GUI 单测需要在 macOS / MSVC 环境的 CI 上跑
  （本机链接不了，且相关用例多在 `#[cfg(target_os = "macos")]` 下）。

### 第五次：同步 `3a0b86a2` → `09acca04`（v4.0.30 之后 fix/优化/清理批次）

**2026-11-17**，从 `3a0b86a2` 到 `origin/main@09acca04`。本分支代码收尾 `a32bce0d`，
115 文件（+7744 / -5632），版本号仍是 `4.0.28`。取舍与逐条进度见
[upstream-merge-plan-2026-11-17-zh.md](./upstream-merge-plan-2026-11-17-zh.md)。

- 范围：先对 `origin/main` 独有的 250 条提交做内容矩阵（已合入 108 / 部分合入 68 / 未合入 74），
  剔除 feat、新模块/新接口、语言包、发布说明、Copilot/WSL 不适用项与文件缺失项，得到 32 条
  补充候选；与上一轮 27 条暂存批次合并为 59 条统一计划，按上游拓扑顺序逐条 `cherry-pick -x`。
  原 27 条暂存批次先备份为 `.audit/staged.patch`（备份 commit
  `c62d9c2c25e5099241499288788a29444c535946`）并还原，再重新入列。
- 结果：实际提交 52 条；7 条为空补丁（HEAD 已有等价实现，核对净改动后跳过）：
  `98777515`、`fc9b17cb`、`80ad02c5`、`883382f1`、`49f2882e`、`8ff9e3b5`、`e9285cf1`。
- 解耦适配与冲突要点：
  - `c70d61de`：只取 `@tauri-apps/cli` 升级与 lock；`release.yml` 保留本分支 deb 构建逻辑，
    不引入上游 AppImage 参数。
  - `60a68363`：只取 `dsh_config` 的 clippy 修正，跳过本分支不存在的 ru/ko 语言纯测试。
  - `7dec90e9`：删除未用 UI 组件、脚本与命令层注释；`Cargo.toml`/`Cargo.lock` 保留 core
    解耦结构（`tower-http` 仍是 core 依赖），跳过上游基线带入的 `privacy_filter` 命令。
  - `f1d438e0`：保留镜像行 `displayName` 恢复；本分支没有 `officialSpeedTiers`，移除上游
    镜像分支里的 `official_speed_tiers` 更新。
  - `3369fe89`：补 `CopilotUsageResponse.access_type_sku`（上游基线已有、本分支缺）。
  - `f72c3048`：`responses_to_chat_completions_with_reasoning` 新增第三参数后，补齐测试调用。
  - `9bc68243`：墓碑机制的调用点已落在 core `mode/controller.rs` 与 GUI `lib.rs`。
- 复查修复：`copilot_auth.rs` 补 SKU 字段、`codex_config.rs` 移除无宿主变量的
  `official_speed_tiers`、`transform_codex_chat.rs` 测试补 `preserve_reasoning_content` 参数。
- 验证：core `cargo check` 通过；core `cargo check --tests` 通过；前端 `pnpm typecheck` 通过。
  core `cargo test` 链接、GUI `cargo check --lib`（缺 `windres`）与前端 vitest（esbuild 0.21.5
  崩溃）受本机环境阻塞，未实际执行，需要在 CI 复核。
- 未纳入：全部 feat、新模块重构（AppSpec / ProviderKind / sync_support / terminal_launch /
  ToolSpec / tool_process）、新计价能力、语言包与多语言 README、版本发布说明、Copilot/WSL
  特定栈、纯文档与纯测试提交。

### 第六次：同步 `09acca04` → `bd3d3880`（fix + 功能重构）

**2026-11-20**，从 `09acca04` 到 `origin/main@bd3d3880`。本分支代码收尾 `e4492f09`，
之后适配提交 `8dd02d62`（补 `sync_utils`）。版本号仍是 `4.0.28`。逐条进度见
`.audit/sixth-progress.txt` / `.audit/merge-plan-2026-11-20.txt`。

- 口径：在「只合 fix」之上追加**功能重构**（`refactor` 中重组现有逻辑、不引入新产品能力
  的提交）。仍跳过全部 feat、版本发布、发布说明、纯 i18n / 纯测试基建、Plugins 依赖项。
- 结果：上游区间 101 条 → 计划合入约 59；实际 **43 OK** + **16 SKIP-ADAPT**；另有计划外
  跳过的 feat/发布/调用图清死代码等。路径重写：`src-tauri/src` → `crates/cc-switch-core/src`
  （GUI 独有 commands/tray/deeplink/部分 services 除外）；缺 de/ko/ru 语言包的改动自动跳过。
- 已合入（摘）：
  - fix：MCP/DSH 锁恢复、Mistral 官方 host、skill sync 跳过坏链、Fable catalog id、
    Copilot Gemini system→user / 1M 门控、Skills 卸载、Codex OAuth 0.169.0 / Responses Lite /
    空 user / thinking 重试 / 配额放行、usage 日期与计价、OpenCode V1/V2、私有参数保留
    tool-call args、泄漏与 proxy 限流等。
  - refactor：DirectorySettings 删除、settings 目录循环与 override 表、UsageFilter /
    session cost / balance HTTP 共用、`/responses/compact` 归并、legacy transform 清理、
    once_cell→LazyLock、Moonshot/DeepSeek host-suffix、MCP 导入路径共用、McpApps/SkillApps
    生成、quota 列共用等。
- SKIP-ADAPT（16，依赖未合架构或与解耦树冲突过大）：
  - 调用图清死代码 / ProxyConfig 收缩依赖 #9570：`cfe78474` 族已在计划阶段剔除；
    `8f465ebd`。
  - 缺 `sync_utils`/`MutexExt` 大扫：`c29bfe59`（本轮后以 `8dd02d62` 只补模块，未重做
    36 文件 lock_recover 扫荡）。
  - misc.rs → tool_discovery / tool_lifecycle：`f4ef3eba` `26831d4e` `176bb05e`。
  - DAO `&AppType` 浪潮及后续：`f9e40d4f` `45ec87d7` `fca6b1bb` `bd3d3880`。
  - forwarder 子模块大拆：`69cdc63c` `3493be84` `09678477`。
  - 命令签名扫荡：`88f59fad`。
  - AGY Orchestrator/Hydrogen 计价与归因（本分支无 Hydrogen 基线）：`9331e562`
    `6da70837` `933cba13`。
- 适配：`8dd02d62` 把上游 `sync_utils.rs` 落到 core 与 GUI，供 `MutexExt`（Codex 官方模型
  缓存）与 MCP/DSH 隔离毒锁测试使用。
- 验证：前端 `tsc` 与 vitest（3694）通过。本机无 MSVC `link.exe`，core/GUI `cargo check`
  未跑，需 CI 复核。
- 未纳入：全部 feat（Plugins / Skills 托管源 / 计价种子 / Sessions 展示增强等）、语言包、
  发布说明、knip CI 清理、以及上述 SKIP-ADAPT 结构性大拆。

**复查补合（同日）**：原先因 `&AppType` / Hydrogen 捆包被 SKIP-ADAPT 的 3 条行为 fix，
按解耦树适配后补入：

| 上游 | 本分支 | 取舍 |
| --- | --- | --- |
| `bd3d3880` | `942a7f1a` | 清 health / 出队列时 `ProviderRouter::forget`；OMO 变体清理告警。仍用 `&str` app key |
| `933cba13` | `138c55ee` | 只摘 usage 同价 reasoning 合并 + Neon 计价种子；跳过 agy Orchestrator/Hydrogen |
| `fca6b1bb` | `71dcb150` | `json_column` + readable/unreadable；MCP flag 读回失败回滚；前端不可读条目提示。未跟 `#9640` DAO 浪潮 |

本机无 MSVC linker，Rust 未编译验证；前端相关 vitest / `tsc` 通过。需 CI 复核。

### 第七次：结构性大拆（misc tool_* + forwarder 子模块）

**2026-11-20**，在第六次 SKIP-ADAPT 之上，按上游模块边界**从本分支现有文件手工拆出**（不直接
三路 cherry-pick 大 diff），并改用 `rustup run stable-x86_64-pc-windows-gnu cargo check`
验证（本机 D 盘 MSYS2/ucrt64；新版 Cargo 的 `build/<pkg>/<hash>` 布局需在
`.audit/target-gnu/debug/build/build` 放空目录以绕过 tauri-build 的 `read_dir` 兼容问题）。

- **misc → tool_***（对应 `f4ef3eba` / `26831d4e` / `176bb05e`，GUI `src-tauri`）：
  - 新增 `tool_version.rs`、`tool_discovery.rs`、`tool_process.rs`、`tool_lifecycle_command.rs`；
    `misc.rs` 以 `#[path]` + `use …::*` 挂接；`mod.rs` 按上游路径重导
    `login_shell_path` / `merge_path_segments` / `CommandDeadline` / `local_tool_version`。
  - 本分支无上游 `tool_lifecycle_progress` 符号（仍用 `ToolLifecycleCoordinator`），该文件跳过。
  - `model_fetch` 改为走 `commands::run_detected_tool_command_with_timeout` 重导。
  - 验证：`cargo check --lib`（gnu）通过。
- **forwarder 子模块**（对应 `69cdc63c` + `09678477`，core）：
  - 新增 `proxy/forwarder/{copilot,diagnostics,endpoint,headers,opencode,request_body,response_check}.rs`；
    子模块内原 `super::‹proxy sibling›` 改为 `super::super::…`。
  - 引入 `Attempt`，`forward` 六处调用改为 `&attempt` + body。
  - **未合** `3493be84`（`managed_auth` / `send` / `OutboundHeaderPlan`）：本分支 `forward()` 内
    仍是内联 token/发送逻辑，尚无同名可抽方法，需另开一轮手工抽。
  - 验证：core `cargo check`（gnu）通过。
- **DAO `&AppType`（`f9e40d4f`）**：21 文件冲突已清——保留本分支 `json_column` /
  `UnreadableEntry` / `proxy_service` / `supports_local_proxy`，跳过上游
  Copilot/WorkBuddy/AppSpec 捆包；`provider_supports_failover` 改为 `pub` 供 GUI；
  `ToSql for AppType` 落在 dao/mod；`mirror_proxy_flag` 用 `supports_local_proxy`；
  **config snippet** 仍收 `&str`（本地 omo / omo-slim 键不是 `AppType`）。
  gnu core / GUI `--lib` 通过。
- **未合** `45ec87d7`（FromStr 收口）：干净树上仍 3 文件冲突，另开。补丁在
  `.audit/45ec87d7-remapped.patch`。
- 工作区尚未提交（第七次拆分 + DAO）；需要时再 `git add` 写提交说明。
