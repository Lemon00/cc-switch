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
- 已知问题（未处理）：`be1857ca` 引入的 `codex_models_passthrough.rs` 调用
  `crate::settings::codex_proxy_without_official_login()`，但该提交只改了 proxy 目录下的
  文件，core 的 `settings.rs` 既没有这个 getter 也没有对应字段（上游
  `src-tauri/src/settings.rs` 有字段与 getter）。因此当前 `cargo check` 会在 core 侧报
  E0425；GNU 目标在临时补齐该函数后编译通过，本机缺 MSVC `link.exe`。前端
  `pnpm typecheck` 已通过。这一项属于代码问题，本次只记录、不改动。
