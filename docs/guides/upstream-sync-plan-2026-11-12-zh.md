# `decouple-core` 上游同步计划（2026-11-12）

> 本文只做提交级分析与取舍，**尚未执行任何合并、未改动任何代码**。
> 合并动作请在确认本清单后单独执行。

## 一、范围与基线

| 项 | 值 |
| --- | --- |
| 本分支 | `decouple-core`，HEAD `6096f37b`（`chore(release): v4.0.28`） |
| 上轮同步到的上游点 | `origin/main@abf50589`（2026-11-11，见 `upstream-sync-log-zh.md` 第三次记录） |
| 本次 `origin/main` HEAD | `3a0b86a2`（2026-11-12，`docs(release): add v4.0.30 release notes`） |
| 待分析提交 | `abf50589..origin/main`，共 **17 个** |
| 已在本分支的补丁 | `git cherry HEAD origin/main` 显示这 17 个全为 `+`，无一已合入 |

工作区另有一处未提交改动：`crates/cc-switch-core/src/settings.rs`（补
`codex_proxy_without_official_login` 的字段 / 默认值 / getter，用于修 v4.0.28 构建的 E0425）。
该改动不属于本轮同步，分析时已排除，也不会被本计划的合并覆盖。

## 二、判定口径

按既定原则逐项确认类型，只看提交说明不够，逐条核了说明正文、改动文件清单和本分支的对应路径：

- **纳入**：`fix`（bug 修复）、`perf`（性能优化），且能在本分支现有结构上落地。
- **排除**：`feat`（功能新增）、`refactor`（重构）、`docs` / `chore(release)`（文档与发布）、
  以及依赖本分支未合入功能的提交，即使标题写的是 `fix`。
- **路径归位**：上游在 `src-tauri/src/xxx.rs` 的后端逻辑，落到
  `crates/cc-switch-core/src/xxx.rs`；GUI 专属命令、窗口、前端文件保持在 `src-tauri/src`
  与 `src/`；不得在 GUI 侧复活已搬走的旧文件。

## 三、可合并提交（9 个）

| # | commit | 类型 | 主题 | 改动 | 归位 |
| --- | --- | --- | --- | --- | --- |
| 1 | `7d5fb657` | fix(usage) | Gemini 流式模型归属与 Vertex 整数字符串 token 数 | 3 文件 / +122-12 | core `proxy/` |
| 2 | `d0e47b2d` | perf(providers) | 打开供应商编辑器不再卡顿 | 10 文件 / +312-97 | 纯前端 |
| 3 | `16092b97` | fix(usage) | AGY 会话 transcript 同步游标 | 3 文件 / +310-84 | core `services/` + `session_manager/` |
| 4 | `74f0d41c` | fix(codex) | 合并回放工具参数里的重复 JSON key | 3 文件 / +338-0 | core `proxy/providers/`（含新文件） |
| 5 | `5628bb11` | fix(window) | 主窗口启动时限制在显示器工作区内 | 3 文件 / +148-0 | GUI 侧（含新文件） |
| 6 | `1a98613e` | fix(agy) | 识别 agy 1.5 无 `id_token` 的钥匙串凭证、切换时刷新配额 | 5 文件 / +322-23 | GUI `services/agy_account/` + 前端 |
| 7 | `d4647fbb` | fix(skills) | 本地元数据刷新移出列表查询 | 8 文件 / +424-0 | core + GUI + 前端 + 测试 |
| 8 | `2abcaa6e` | fix | Codex OAuth 卡片显示账号身份 | 2 文件 / +94-17 | 纯前端 + 测试 |
| 9 | `08ec82aa` | fix(proxy) | 第三方 Responses 上游剥离全部 OpenAI-only 字段 | 3 文件 / +274-17 | core `proxy/` |

### 逐条依据

1. **`7d5fb657`** — 修两类真实缺陷：Gemini 流式用量过滤只收含 `usageMetadata` 的分片，
   丢了首片 `modelVersion`，导致流式行全部回退成请求模型；Vertex 把 token 数写成
   protobuf int64 字符串，原来的 `as_u64()` 会让整份用量被丢弃。改
   `handler_config.rs`、`response_processor.rs`、`usage/parser.rs`，三者在 core 的
   `proxy/` 下均存在，直接归位。顺带去掉一行失效的 `#[allow(dead_code)]`。

2. **`d0e47b2d`** — 唯一的性能提交，收益可感知（首次打开编辑器少等 1–2 s）：空闲时
   预加载编辑/新增对话框 chunk、把正在编辑的供应商放进独立 store 避免 `App` 与所有卡片
   重渲染、表单状态改由 `initialData` 初始化并跳过 mount 时的 `form.reset`、`useWatch`
   局部读取名称。**注意**：它新增 `EditProviderHost.tsx`、`editProviderStore.ts`、
   `preloadOnIdle.ts` 并调整 `App.tsx` 等 10 个前端文件，实现手段带组件/store 拆分。
   本计划按「commit 类型为 perf、无新功能、无实验开关、有明确用户可感知收益」判定为
   **可合并**；若按「排除重构」从严把握，这一条是本清单里唯一应移入排除项的，请裁决。
   合并时建议独占一轮并跑前端 typecheck / 组件测试。

3. **`16092b97`** — AGY 会话 transcript 的同步游标修复。上游 `session_usage_agy.rs`、
   `session_usage_agy/native_models.rs`、`session_manager/providers/agy.rs` 在 core 下
   都有对应路径，三处一起归位；说明正文很短，但改动只在用量统计链路，无新接口。

4. **`74f0d41c`** — Codex 每轮回放整段会话，模型可能在 `function_call.arguments` 里
   重复写一个 key；严格 Responses 上游会回 400，且因每轮都回放导致会话永久不可用。
   修法是转发前按 last-value-wins 合并重复 key，只重写确有重复的项，官方上游跳过。
   上游新增 `proxy/providers/codex_responses_arguments.rs`（314 行），须落到
   `crates/cc-switch-core/src/proxy/providers/` 并在 `mod.rs` 注册；`forwarder.rs` 的
   23 行改动归位到 core 同名文件。

5. **`5628bb11`** — 修 #9206，主窗口启动超出显示器工作区。新增 `window_fit.rs`
   （142 行）并改 `lib.rs`、`lightweight.rs`，三者都是 GUI 专属路径，本分支
   `src-tauri/src/` 下 `lib.rs`、`lightweight.rs` 均在，新增文件可直接放在同目录。
   不涉及 core。

6. **`1a98613e`** — agy 1.5 钥匙串凭证没有 `id_token` 时识别失败、切换供应商不刷新配额。
   上游改 `src-tauri/src/services/agy_account/{keychain,mod,oauth,tests}.rs`；本分支
   **agy_account 模块保留在 GUI 侧**（`src-tauri/src/services/agy_account/`），四个文件
   都在，不需要归位到 core。另改前端 `useManagedAuth.ts`，该文件也存在。

7. **`d4647fbb`** — 把本地元数据扫描从 `get_installed_skills` 列表查询里挪走，列表查询
   变只读、刷新走单独命令并在写锁外算 hash，属于性能/正确性修复。涉及
   `commands/skill.rs`（GUI）、`services/skill.rs`（core，需归位）、`lib.rs`（GUI）、
   前端 `UnifiedSkillsPanel.tsx` / `useSkills.ts` / `lib/api/skills.ts` 及两个测试文件，
   本分支路径均存在。新增的 `refresh_local_skill_metadata` 命令是这次修复的配套接口，
   不是独立功能。

8. **`2abcaa6e`** — 看过 diff 后确认是修复而非新功能：把 `isCodexOauth` 的判定提前、
   账号查询改用 `managedCodexAccountId ?? default_account_id` 兜底、显示条件由
   `codexOfficialIdentity !== "api_key"` 收紧为 `native_login || isCodexOauth`。只改
   `ProviderCard.tsx`（10 行）加一份组件测试（84 行），本分支前端文件存在。

9. **`08ec82aa`** — 第三方路由按内置 openai provider 写出，Codex 走 `is_openai()` 分支
   会带上一堆只有 ChatGPT 后端接受的字段，LiteLLM / DeepSeek 中继会整单 400。
   修法是按 codex-rs 的 `!is_openai` 分支镜像剥离 include 标记、`stream_options`、
   `configuration_update`、逐项 `internal_chat_message_metadata_passthrough`、
   若干 `encrypted_content` 与顶层 `access_programs`，无这些字段的请求保持字节不变。
   改 `forwarder.rs`、`providers/codex.rs`、`providers/mod.rs`，三者在 core `proxy/` 下
   均存在，直接归位。本条与第 4 条都动 `forwarder.rs`，合并时注意顺序与冲突。

## 四、不合并提交（8 个）

| # | commit | 类型 | 排除理由 |
| --- | --- | --- | --- |
| 1 | `859ffa10` | docs | 修 v3.7.0–v3.8.0 发布说明页脚相对链接（14 处 `../`→`../../`），纯文档、历史版本，非必要 |
| 2 | `49a57446` | feat(pricing) | 新增 Claude Fable 5.2 定价种子数据，功能新增 |
| 3 | `210103c6` | feat(pricing) | 新增 Gemini 4 Carbon 定价种子数据，功能新增 |
| 4 | `f6c7eed0` | docs(contributing) | 只更新 `CONTRIBUTING.md` 里供应商切换的描述 |
| 5 | `4a1369f2` | refactor(copilot) | 明确的重构：把官方登录卡并入「已添加」分组，并删 7 个语言包的 `copilot.section` 键；依赖本分支未合入的 Copilot 分组/BYOK 功能 |
| 6 | `23c57e8d` | fix(copilot) | 标题是 fix，但只改 `src-tauri/src/copilot_config.rs`；本分支 HEAD 里**不存在任何 `copilot_config` 文件**（Copilot BYOK 系列 feat 上轮已明确跳过），补丁无法落地，落进去只会形成半套路径 |
| 7 | `609e7914` | chore(release) | v4.0.30 版本号与 CHANGELOG，本分支版本为 4.0.28，发布节奏独立 |
| 8 | `3a0b86a2` | docs(release) | v4.0.30 三语发布说明 + `src/whats-new/4.0.30.json`，跟随上游发版，不属于 bug 修复 |

补充：`23c57e8d` 与 `4a1369f2` 都属于上一轮就明确跳过的 Copilot BYOK / Deep Link /
账号管理功能链。该链在上游还有 `0d69dd06`、`8d866cef`、`9bd9c516`、`65720291`、
`7b657e02` 等 feat，单独挑一个 fix 进来会形成半套路径，因此整体维持排除。

## 五、落地注意

- **归位**：第 1、3、4、9 条整体落到 `crates/cc-switch-core/src/`；第 7 条只有
  `services/skill.rs` 需要归位，其余留在 GUI / 前端；第 5、6 条按 GUI 侧原路径。
- **顺序建议**：先做只改 core 且互相独立的 `7d5fb657`、`08ec82aa`、`74f0d41c`、
  `16092b97`（其中 `08ec82aa` 与 `74f0d41c` 同改 `forwarder.rs`，按上游顺序
  `74f0d41c` → `08ec82aa`），再做 GUI / 前端的 `5628bb11`、`1a98613e`、
  `d4647fbb`、`2abcaa6e`，最后单独处理 `d0e47b2d`。
- **验证**：core 三档 `cargo check`、CLI / GUI `cargo check`、前端 `pnpm typecheck`；
  本分支已知 GNU 目标可编译，`src-tauri` 的 `tauri-build` 在本机缺 `windres`，
  与代码无关，CI runner 不受影响。
- **不在本轮范围**：工作区那处未提交的 `settings.rs` 修复（E0425）应先独立提交，
  不要混进同步提交，否则回溯困难。

## 六、执行结果

**2026-11-12 已按本清单执行完毕**（第三节 9 条全部合入，第四节 8 条维持跳过）。
逐条映射与验证结果见 [upstream-sync-log-zh.md](./upstream-sync-log-zh.md) 第四次记录。

- 合入顺序：`d0e47b2d`（单独一轮，纯前端）→ `7d5fb657` → `16092b97` → `74f0d41c` →
  `08ec82aa` → `5628bb11` → `1a98613e` → `d4647fbb` → `2abcaa6e`。
- 与计划的两处偏差：
  1. `08ec82aa` 与 `74f0d41c` 同改 `forwarder.rs`，但上游侧没有冲突，两轮都自动应用，
     原计划的"注意顺序与冲突"实际上没有触发。
  2. `2abcaa6e` 的测试补丁带 `· Plus` 套餐后缀断言，而本分支已移除该功能，断言一并删掉，
     只保留账号身份相关的验证。
- 追加一条清单外的修复：`8a3d334f`（`AgyAccountManager::quota()` 的返回类型与实现不一致，
  HEAD 本来编译不过）。它不在上游 17 个提交里，是解耦树上遗留的代码问题。
- 文档里的"工作区那处未提交的 `settings.rs` 修复"仍未提交，本轮没有把它混进同步提交。
