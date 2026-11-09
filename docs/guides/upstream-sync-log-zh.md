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

> 上游 `v4.0.28` = `8656ebd9`，`main` 已到 `cdca0984`，**尚未同步**。

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
