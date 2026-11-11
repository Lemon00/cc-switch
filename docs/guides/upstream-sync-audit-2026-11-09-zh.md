# `decouple-core` 上游修复合并审计（2026-11-09）

> 审计对象：`decouple-core` 分支及其 `sync-main-fixes` 同步批次  
> 代码基线：`8ee8536c`（`chore(release): v4.0.27`），文档基线为 `7c5bbaa2`  
> 上游基线：`main` / `5a3d7f92`（v4.0.28 之后）  
> 同步起点：`80d1d6a6`（v4.0.26a）  
> 审计方式：只读；逐提交 patch 对比、调用链静态检查、GNU Windows 目标编译检查、前端 `tsc` 检查  
> 审计日期：2026-11-09

---

## 一、结论摘要

本批次共 30 个提交：

- 27 个来自上游 `main` 的 fix/perf 提交；
- 1 个上游 v4.0.27 版本号提交；
- 2 个本分支解耦适配提交：`e5424930`、`55d2ffd9`。

**目前没有发现会直接导致本分支原生 Windows 构建、启动或核心切换流程失败的合并缺陷。**  
`core`、`cli`、`src-tauri`（含 tests）在 GNU Windows 目标下均通过 `cargo check`，前端 `pnpm typecheck` 通过。

但有三类问题需要在合并前明确：

1. **两个测试侧回归**：`e5424930` 和 `2dc34833` 漏掉了上游配套测试提交，在 WSL/UNC 或无 file ID 的文件系统上会失败。生产逻辑本身大多数是对的，但测试会给出错误结论。
2. **一个生产兼容性变更需要确认**：`95da24b9` 不再写 `disable_response_storage`，并会从旧 live 配置中清掉它。对当前 Codex 是正确修复，但对旧版 Codex 或复用该配置的 fork 是否安全，需要明确最低版本假设。
3. **若干行为变化需要真实回归**：故障转移按目标改模型名、第三方上游剥离 ChatGPT 身份头、compaction 400 重试、symlink 写穿。这些都不是编译问题，而是运行时行为问题。

**没有发现新功能被误合入。** 上游的 sessions FTS 搜索、TokenPlus、韩语 i18n、Genesis/Mistral、官方登录暂停/额度保护、内置 openai provider、Skills 布局、界面密度/缩放等 feat 提交均未进入本分支。

---

## 二、审计范围与方法

- 以 `80d1d6a6..8ee8536c` 的 30 个提交为对象。
- 按提交标题将本地提交与上游 `main` 对应提交配对。
- 对每一对提交，比较 `git show` 的增删行集合（忽略文件路径前缀和 hunk 头），确认是否存在 cherry-pick 丢失、额外改动或手工重写。
- 对 `decouple-core` 的关键结构做静态检查：
  - `crates/cc-switch-core` 是否被 GUI/CLI 正确复用；
  - `src-tauri` 中是否还有与 core 重复的旧实现；
  - 新增的 Rust 类型、字段、函数是否在 split 后的模块路径下可见；
  - 新增前端文件、i18n key 是否完整；
  - 跳过的新功能是否被 fix 提交“半截带入”。
- 编译检查：
  - `cargo +stable-x86_64-pc-windows-gnu check --all-targets`（core）
  - `cargo +stable-x86_64-pc-windows-gnu check`（CLI）
  - `cargo +stable-x86_64-pc-windows-gnu check --tests`（GUI）
  - `pnpm typecheck`

---

## 三、验证结果

| 检查项 | 结果 | 说明 |
| --- | --- | --- |
| core 全 target 编译 | 通过 | 5 个已有 warning，无 error |
| CLI 编译 | 通过 | 无 error |
| GUI 含测试目标编译 | 通过 | 17 个 unused import warning，无 error |
| 前端 TypeScript | 通过 | `tsc --noEmit` 无输出 |
| 逐提交 patch 对比 | 基本一致 | 26/27 个上游 fix 的增删行集合完全一致；`95da24b9` 仅差 1 行注释 |
| 运行时全量测试 | **未完成** | `cargo test` 在该仓库会因多个测试共享 HOME/环境变量而互相干扰；本次尝试在无关的 `opencode_config` 测试处异常中止，不能作为本批次失败证据 |
| 前端单测 | **未完成** | vitest 启动时 esbuild `spawn EPERM`，是当前沙箱限制，不是代码失败 |

补充：本分支当前 `.github/workflows` 只有 `release.yml`，没有上游的 `wsl2-nightly.yml`。因此下面两个 WSL/UNC 测试问题不会在当前 CI 里自动暴露，但一旦手工在 WSL 共享目录下跑测试，或以后恢复该 workflow，就会失败。

---

## 四、发现的问题

### P1-1：catalog pointer 测试与 WSL/UNC 生产逻辑相反

**涉及提交：**

- `ab3c1843 fix(codex): write an absolute model_catalog_json pointer for native config dirs`
- `e5424930 test: align catalog-pointer assertions with the absolute path written by 13af7fc9`

**位置：**

- 生产逻辑：`crates/cc-switch-core/src/live/project/codex.rs` 的 `catalog_pointer_value()`
- 测试断言：`crates/cc-switch-core/src/mode/controller.rs`（约 `:4267`）
- Golden 测试：`src-tauri/tests/golden/codex_red_lines.rs` 的 `model_catalog_pointer_ownership`

**问题：**

`ab3c1843` 的生产逻辑是：

```rust
fn catalog_pointer_value(config_dir: &Path) -> String {
    let catalog_path = config_dir.join(CATALOG_FILENAME);
    let text = catalog_path.to_string_lossy();
    let is_unc = text.starts_with(r"\\") || text.starts_with("//");
    if is_unc || !catalog_path.is_absolute() {
        return CATALOG_FILENAME.to_string();
    }
    text.into_owned()
}
```

即：

- 原生本地目录：写绝对路径；
- 相对目录：写裸文件名；
- Windows UNC / WSL 共享目录（`\\wsl.localhost\...`、`\\wsl$\...`）：写裸文件名。

但 `e5424930` 的测试断言是无条件“绝对路径”：

```rust
Some(
    crate::codex_config::get_codex_model_catalog_path()
        .to_string_lossy()
        .as_ref()
)
```

当 Windows 上的 `HOME`（或测试用 `CC_SWITCH_TEST_HOME`）指向：

```text
\\wsl.localhost\Ubuntu-24.04\home\user
```

或：

```text
\\wsl$\Ubuntu\home\user
```

时：

- `get_codex_model_catalog_path()` 返回 `\\wsl.localhost\...\cc-switch-model-catalog.json`；
- 生产 `catalog_pointer_value()` 因为 `is_unc == true`，返回裸文件名 `cc-switch-model-catalog.json`；
- 测试却断言前者。

**影响：**

- 在原生 Windows 用户目录（`C:\Users\...`）下，测试通过；
- 在 WSL2 夜间测试、或让测试 HOME 跑在 `\\wsl.localhost` 共享上时，以下测试会失败：
  - `mode::controller::mode_tests::attaching_files_after_a_detached_route_switch_rewrites_the_catalog`
  - `golden::model_catalog_pointer_ownership`
- 当前分支没有 `wsl2-nightly.yml`，所以 CI 不会自动红；但这等于**放弃了该环境的回归覆盖**。以后恢复 WSL 工作流，或者用户/维护者在 WSL 共享 HOME 下跑 cargo test，就会看到“测试失败但产品是对的”的假阳性。

**根因：**

`e5424930` 手工对齐了上游 `7d10364b` 的测试，但漏掉了后置的：

```text
07798d56 fix(ci): keep the WSL2 suite on a stable test root and match UNC catalog pointers
```

`07798d56` 做了三件事：

1. 把 `catalog_pointer_value` 从私有提升为 `pub(crate)`；
2. 在 `mode/controller.rs` 测试里新增 `ours_catalog_pointer()`，用**实际配置目录**现算预期值，而不是硬编码绝对路径；
3. 给 `session_usage_dsh`、`agy` 等 SQLite 临时目录测试加 `sqlite_unsupported_in_temp_dir()` 跳过，并调整 WSL2 workflow 的稳定测试根。

本分支没有第 1、2 步，所以测试与实现脱节。

**建议（只改测试，不改生产）：**

- 采用 `07798d56` 的做法：测试预期由 `catalog_pointer_value(crate::codex_config::get_codex_config_dir())` 现算；
- 或者至少在两处断言里判断 `is_wsl_path` / UNC，WSL 形态时期望裸文件名；
- 不要为了测试通过把生产逻辑改回绝对路径。UNC 下写裸文件名是为了让 WSL 侧的 Codex 按自己的配置目录解析，属于 #3614 的既有语义。

**性质：** 测试回归，不是当前原生 Windows 生产回归。优先级放在“合并前应处理”，因为合并提交本身声称“逐条 cherry-pick、无冲突”，但配套测试提交没有合入。

---

### P1-2：`2dc34833` 的配套测试跳过提交 `e1f8acf6` 未合入

**涉及提交：**

- `2dc34833 fix(usage): verify Codex forks on file systems without file IDs`
- 上游配套：`e1f8acf6 test(usage): skip same-size replacement checks where files have no IDs`（本分支未合入）

**位置：**

- `crates/cc-switch-core/src/services/session_usage_codex.rs`
  - `ParentFileStamp`：约 `:99`
  - `ParentFileStamp::from_file()`：约 `:114`
  - `windows_file_identity()`：约 `:133`
  - 测试辅助 `parent_cache_supported()`：约 `:4344`
  - 三个依赖“同大小同 mtime 的替换可识别”的测试：
    - `test_parent_file_stamp_distinguishes_same_size_same_mtime_files`（约 `:4572`）
    - `test_retryable_pending_detects_same_size_same_mtime_parent_replacement`（约 `:5504`）
    - `test_retryable_pending_detects_same_size_same_mtime_child_replacement`（约 `:5626`）

**问题：**

`2dc34833` 把 `ParentFileStamp` 的身份从“必有卷序列号 + 文件 ID”改成：

```rust
#[cfg(windows)]
identity: Option<(u64, [u8; 16])>,
```

`windows_file_identity()` 在文件系统不支持 `FileIdInfo` 时返回：

```rust
Ok(None)
```

即 `Ok(None)`；`from_file()` 仍然返回 `Some(ParentFileStamp { identity: None, ... })`。

但测试辅助函数还是旧的语义：

```rust
fn parent_cache_supported(path: &Path) -> bool {
    // 这里只判断 stamp 是否为 Some
    let supported = ParentFileStamp::from_file(...).is_some();
    ...
}
```

`is_some()` 在 `identity: None` 时仍然为 `true`。于是：

- 在 NTFS 上：identity 有值，测试正常；
- 在 WSL 9P 共享、FAT/exFAT 等没有文件 ID 的文件系统上：identity 为 `None`，`parent_cache_supported()` 仍返回 `true`；
- 上述三个测试继续执行，并断言“同 size、同 mtime 的两个文件能被区分” / “替换能被检测到”；
- 实际比较逻辑只剩 size + modified_nanos，两个 stamp 相等，测试失败。

上游 `e1f8acf6` 正是为此新增：

```rust
fn same_size_same_mtime_replacement_detectable(path: &Path) -> bool {
    ...
    if stamp.identity.is_none() { return false; }
    ...
}
```

并替换了三个 guard 调用点。本分支没有这个提交。

**产品影响：**

这不是纯粹“测试写得不对”，而是暴露了 `2dc34833` 的一个有意取舍：

- 在无文件 ID 的平台上**启用** parent timeline 缓存；
- 缓存判等只能靠 mtime + size；
- 同 size、同 mtime 的重写无法识别，可能复用旧 parent usage。

正常 Codex rollout 是追加写，size 会变，所以命中概率低；但如果备份/还原、同步工具、或人工替换保留了 mtime 且大小相同，就可能出现**少算/沿用旧 usage**，而不是崩溃。上游用测试跳过承认了这个盲区，本分支既没有跳过测试，也没有注释说明这个取舍。

**建议：**

1. 合入 `e1f8acf6`（纯测试提交），或做等价修改：三个测试用 `identity.is_none()` 作为 skip 条件；
2. 在 `ParentFileStamp` 的注释里明确写出“无 file ID 时只凭 mtime+size，同 size+同 mtime 的替换检测不到”；
3. 如果团队更看重 usage 准确性而非 WSL 性能，可在 `identity.is_none()` 时禁用 parent timeline 缓存；但这会回退 `2dc34833` 的性能目的，需要产品/性能侧确认；
4. 在 WSL2 文件系统上补一条真实回归：Codex 父 rollout 追加、同 size 替换、还原备份三种场景。

**性质：** 测试回归 + 已接受的生产盲区。当前原生 NTFS 用户不受影响。

---

### P2-1：`disable_response_storage` 被无条件清理，旧 Codex / fork 兼容性需要确认

**涉及提交：**

- `95da24b9 fix(codex): stop writing disable_response_storage`

修改了 15 个文件：

- `crates/cc-switch-core/src/live/floor.rs`
- `crates/cc-switch-core/src/live/project/codex.rs`
- `crates/cc-switch-core/src/provider.rs`
- `crates/cc-switch-core/src/services/provider/codex_terminal.rs`
- `src-tauri/src/deeplink/provider.rs`
- `src/config/codexProviderPresets.ts`
- `src/config/codexTemplates.ts`
- 以及对应快照/测试

**行为变化：**

- 新建 Codex 供应商时，不再写 `disable_response_storage = true`；
- 旧供应商行里若还有这一行，`codex_terminal` 将其归为 `Leftover::Ignore`，切换时不再投影；
- `CODEX_FLOOR_TOP` 仍保留该 key，因此写 live 时会把旧 live 里的这一行**清掉**；
- Universal Provider 模板、Codex 模板、内置 preset、DeepLink 导入模板都移除了该行。

根据上游提交说明，这是正确的：Codex 0.167 起不认该键，且从 2025-09 起恒发 `store: false`，留着会在每次开会话时报 `unrecognized configuration setting`。

**需要确认的风险：**

这是**全局行为变更**，而不是只影响一个新预设：

- 用户如果仍运行旧版 Codex（低于移除该键的版本），或使用复用 Codex config 的 fork，之前依赖该键让上游收到 `store: false`；
- 切一次供应商后，旧 live 里的这一行会被清掉，行里的也被忽略；
- 如果某个第三方网关对“请求体没有 `store` 字段”和“`store: false`”处理不同，可能出现新的 400/存储行为；
- 这是**无法按供应商回退**的：`codex_terminal` 已把该键归为 Ignore，即便用户手动在行配置里写回，也不会生效。

当前仓库没有声明最低 Codex 版本，也没有版本探测。上游是按“当前 Codex 已恒发 store:false”来假设的。

**建议：**

1. 在发布说明/兼容性文档中明确：本版本起不再写 `disable_response_storage`，要求 Codex ≥ 0.167（或实际开始忽略该键的版本）；
2. 若仍要支持旧 Codex/fork，至少保留“手动行配置里的该键原样投影”的逃生口，而不是归为 Ignore；默认模板可以不再生成；
3. 回归测试至少覆盖：
   - 旧行含 `disable_response_storage = true`，切到新行后 live 不再有该键；
   - 第三方网关在无该键时能正常处理 Responses；
   - Codex 0.167 下不再出现 `unrecognized configuration setting`。

**性质：** 生产行为变更，方向正确，但兼容性假设未在仓库中固化。

---

### P2-2：Codex 故障转移改模型名，可能覆盖用户显式选择的模型

**涉及提交：**

- `3ea399e0 fix(proxy): map the model per target when Codex fails over across vendors`
- `b478ca2a fix(proxy): keep image and search models when Codex fails over`

**位置：**

- `crates/cc-switch-core/src/proxy/model_mapper.rs` 的 `map_codex_failover_model()`
- `crates/cc-switch-core/src/proxy/forwarder.rs` 的 `maps_codex_failover_model()`

**逻辑：**

```rust
// failover_chain = app_config.auto_failover_enabled
if self.maps_codex_failover_model(app_type, endpoint) {
    super::model_mapper::map_codex_failover_model(mapped_body, provider)
}
```

`map_codex_failover_model()` 的行为：

- 请求体没有 `model`：不动；
- 目标供应商 config 里没有顶层 `model`：不动（官方卡等）；
- 请求的模型等于目标自己的 `model`：不动；
- 请求的模型在目标 `modelCatalog` 的发布列表里：不动；
- 否则：把请求模型改成目标行里的 `model`。

`b478` 又加了独立端点判断，跳过 `/images/generations`、`/images/edits`、`/alpha/search` 等，避免把图片/搜索模型换成文本模型。

**需要确认的风险：**

1. `failover_chain` 是 `auto_failover_enabled`，不是“已经发生过一次失败”。因此只要用户开了故障转移，**第一个**目标也会走这个映射。如果主供应商的 catalog 不完整，但它的 `model` 别名实际可用，用户显式选的模型会被静默替换成行里的 `model`。
2. 该逻辑只认 `modelCatalog` 和 config 的 `model`，不认供应商实际 `/v1/models` 返回的模型。catalog 过期时可能误判“这家不认”，然后替换。
3. 替换只改请求体的 `model`，不改 URL/headers；如果某家用模型名做路由前缀（例如 `provider/model`），可能反而打错。
4. `b478` 的端点判定依赖 `CodexStandaloneEndpoint::from_effective_endpoint()` 对 query 的规范化。测试覆盖了 `?client_version=...`，但未覆盖其他 query；新增图片端点参数时要同步检查，否则可能重新把图片模型映射掉。

**建议：**

- 用真实的两家供应商做一轮故障转移回归：A 家选 B 家模型名 → 失败 → 切 B → 请求成功，且日志里能看到“改用 B 的 model”；
- 覆盖 catalog 不完整、用户用 `-m` 覆盖、图片/搜索独立端点；
- 如果发现主供应商被误替换，考虑把映射限制为“仅在发生 failover 后”（attempt index > 0），而不是 `auto_failover_enabled` 即生效；
- 在发布说明中写明“故障转移会按目标供应商重写模型名”。

**性质：** 生产行为变更，编译通过，需运行时回归。

---

### P2-3：第三方 Codex 上游剥离 ChatGPT 身份头，依赖 `is_codex_oauth()` 分类

**涉及提交：**

- `17d29028 fix(proxy): strip ChatGPT identity headers for unmanaged Codex upstreams`

**位置：**

- `crates/cc-switch-core/src/proxy/forwarder.rs` 约 `:2622`
- `is_chatgpt_identity_header()` 约 `:3781`

**逻辑：**

```rust
matches!(app_type, AppType::Codex) && !provider.is_codex_oauth()
```

为真时，对非官方、非 CC Switch 托管的 Codex 上游剥离：

- `chatgpt-account-id`
- `openai-organization`
- `openai-project`
- `version`
- `x-oai-` 前缀
- `x-openai-` 前缀
- `x-responsesapi-` 前缀

同时保留 `session_id`、`originator`、`x-codex-*` 等会话/客户端头。

**需要确认的风险：**

- `is_codex_oauth()` 只看 `meta.providerType == "codex_oauth"`。如果某个 Codex 供应商实际使用 ChatGPT 登录，但 providerType 不是 `codex_oauth`，它的身份头会被剥掉，可能导致 401/403 或账号级路由失效。
- 反过来，如果某个第三方供应商为了兼容而伪装成 `codex_oauth`，身份头会泄漏给第三方。
- 本分支没有合入 `905c9148 feat(codex): write third-party routes as the built-in openai provider`。该 feature 会改变 Codex 第三方路由的身份形态；当前 `17d` 的注释已提到 `codex_direct` 的 `openai_route`，但本地没有 905 的完整改动。需要确认：**没有 905 时，这个分支的 `openai` 内置路由是否仍可能带 ChatGPT 身份头发到第三方？** 如果会，`17d` 是必要修复；如果不会，属于防御性代码，风险较低。
- `version` 是通用头，部分第三方网关可能用它做版本路由；剥离后行为可能变化。
- 该逻辑只对 `AppType::Codex` 生效，不影响 Claude/Grok，正确。

**建议：**

- 为 `codex_oauth` 分类做一组数据驱动测试：官方卡、API key 卡、内置 openai 卡、第三方 OAuth 卡；
- 抓一次真实出站请求，确认第三方上游没有 `chatgpt-account-id` / `x-oai-*`，官方上游仍有；
- 如果第三方网关依赖 `version`，在 provider 自定义 headers 里提供逃生口；
- 记录到发布说明：第三方 Codex 上游不再收到 ChatGPT 身份头。

**性质：** 安全/隐私修复，方向正确，分类依赖需要回归。

---

### P2-4：写穿 symlink 是全局原子写语义变更，不只是设置文件

**涉及提交：**

- `7a962be2 fix(config): write through symlinked live config files`

**位置：**

- `crates/cc-switch-core/src/config.rs:643` `stage_write`
- `:722` `resolve_symlink_target`
- `:758` `join_link_target`
- `:789` `commit_staged`

**行为：**

`atomic_write` / `stage_write` / `commit_staged` 写入前会逐跳解析最终组件的 symlink，把临时文件建在**目标文件所在目录**，最后替换目标文件，保留链接本身。

对 dotfiles 用户是正确修复：此前 rename 到链接会把链接替换成普通文件，或者 Windows `ReplaceFileW` 报 1464。

**需要关注：**

1. **影响面是全局的**：`auth.json`、`settings.json`、Codex config、provider 配置、备份等所有走 `atomic_write` 的路径都会跟随 symlink。这不是只影响“live config”一个文件。
2. **安全边界**：解析逻辑不做 `canonicalize` 全路径，也不使用 `O_NOFOLLOW`；如果配置目录可被其他本地用户写，攻击者可以把目标文件换成指向用户可写文件的 symlink，让 CC Switch 把含 API key 的配置写到别处。这是本地同用户威胁模型下的常见取舍，不算提权，但值得在安全评审里记录。
3. **崩溃恢复语义**：`StagedWrite.path` 存的是解析后的目标路径。如果写入过程中链接被改指向/删除，恢复会写回旧目标，而不是当前链接目标。对 dotfiles 可接受，但需要知道。
4. **悬空链接**：会在链接指向的位置新建文件，链接保留，符合注释；但 `fs::create_dir_all` 会创建目标父目录，可能意外创建用户不存在的目录。
5. **Windows junction**：`file_type().is_symlink()` 对 junction 通常为 false，目录 junction 不会被解析；这与注释“只解析最后一段”一致，不是 bug，但要确认 Windows 用户预期。
6. **相对链接带 `..`**：`join_link_target` 先 canonicalize 父目录再回退，注释解释正确；但如果父目录不存在，fallback 到字面 `parent.join(target)`，后续 `create_dir_all` 可能创建出与 OS 解析不同的路径。边缘情况。

**建议：**

- 在安全/发布说明里记录“配置写入会跟随最终组件 symlink”；
- 加一个 Windows 测试：`settings.json` 是 symlink，写入后链接仍在、目标内容更新；
- 加一个失败注入测试：commit 失败时链接不被替换、临时文件清理/保留符合预期；
- 如果未来支持多用户共享配置目录，重新评估 `O_NOFOLLOW`/`openat`。

**性质：** 生产行为变更，dotfiles 场景正确，影响面比提交标题大。

---

### P3-1：`1949a065` + `55d2ffd9` 是“自我抵消”的一对提交

**位置：**

- `src-tauri/src/commands/tool_uninstall.rs:151`

**事实：**

- `1949a065` 去掉 `mut`，因为上游 feature `810775eb feat(apps): show queue state and live progress for tool installs` 把 `.output()` 改成了 `run_with_progress(process, ...)`，新接口接收共享引用，不需要 `mut`；
- 本分支**没有合入** `810775eb`，所以仍然调用 `Command::output()`，需要 `&mut self`；
- `55d2ffd9` 把 `mut` 加回来，编译恢复。

两个提交的净 diff 为零：

```rust
// 1949: let (current, process) = ...
// 55d:  let (current, mut process) = ...
```

**影响：**

- 当前没有运行时影响，也没有编译影响；
- 这是一次“为排除 feature 而引入的 fix 依赖”的典型例子：`1949` 本身不是独立修复，而是 feature 810 的配套清理；
- 未来合入 810 时，`55d` 需要再次去掉 `mut`，否则会出现 `unused_mut`；如果 CI 把 warning 提升为 error，会失败；
- 如果以后用 patch-id / `git cherry` 判断“上游 1949 是否已合入”，这一对提交会让判断变得含糊。

**建议：**

- 在同步记录中明确标注：`1949a065` 与 `55d2ffd9` 是成对适配，净 diff 为零，**不应视为两个独立修复**；
- 合入 810 时，把这两个提交一起处理（drop/revert），不要只 cherry-pick 1949；
- 不需要现在改代码。

**性质：** 维护性问题，不是当前 bug。

---

### P3-2：版本号与发布说明不同步

**位置：**

- `package.json`：`4.0.27`
- `src-tauri/Cargo.toml`：`4.0.27`
- `src-tauri/tauri.conf.json`：`4.0.27`
- `crates/cc-switch-cli/Cargo.toml`：`4.0.27`
- `crates/cc-switch-core/Cargo.toml`：`4.0.25`（内部版本，按同步记录是有意保持）

**问题：**

- `8ee8536c chore(release): v4.0.27` 只改了 6 个版本载体，**没有带 `CHANGELOG.md`**；
- 上游对应 release 提交 `2aeb27d8` 带 44 行 CHANGELOG；
- 本分支跳过了 v4.0.26/v4.0.27/v4.0.28 的 release notes 提交，`docs/release-notes` 里没有对应版本说明；
- 结果是：产品版本显示 4.0.27，但仓库内 changelog/发布说明仍停在 v4.0.25 附近。

**影响：**

- 不影响运行；
- 影响用户理解“4.0.27 到底包含哪些修复”、issue 归因、以及自动更新说明；
- 如果你们用版本号区分“上游兼容基线”，当前 4.0.27 实际包含 4.0.27 之后的多个 fix（直到 `cac921f5`），语义上略超前于 upstream 的 4.0.27 tag。

**建议：**

- 在 `docs/guides/upstream-sync-log-zh.md` 的第二次记录里补上“产品版本 4.0.27、内部 core 4.0.25、覆盖上游范围 f621d421→cac921f5”的说明（该文档已有类似内容，补全即可）；
- 如对外发布，补一份 `docs/release-notes/v4.0.27-*.md` 或本地 CHANGELOG 条目，至少列本批 fix/perf；
- 不要把 core 4.0.25 当作错误，除非发布流水线依赖 core 版本。

**性质：** 发布/文档一致性，P3。

---

## 五、逐提交结论表

| 提交 | 主题 | 结论 |
| --- | --- | --- |
| `bed18aa2` | Linux tray DPR 坐标 | 通过；内容与上游一致 |
| `8ecb0b56` | ProviderIcon 代理对首字母 | 通过；新增测试覆盖 emoji |
| `d627998e` | deep link IPv6 内网识别 | 通过；安全修复 |
| `ab3c1843` | Codex catalog 绝对指针 | 生产逻辑通过；测试配套见 P1-1 |
| `db3b9cb3` | Skill 忽略 Python bytecode | 通过；三连之一 |
| `76712bde` | 旧 hash 迁移 | 通过；三连之一 |
| `3a25dbd2` | 迁移保留 updated_at | 通过；三连之一 |
| `20e0f41b` | 禁用 WebView 默认右键 | 通过；仅前端 |
| `69991faa` | models.dev 读取超时/错误描述 | 通过；仅 core |
| `d26fc0ed` | 不误认 Python pi.exe | 通过；GUI app 管理专用，CLI 无此命令 |
| `5f8c889a` | 仓库发现失败不再显示 0 | 通过；依赖的 API 类型已存在 |
| `7a962be2` | symlink 写穿 | 行为通过；全局语义变更见 P2-4 |
| `5c9fe7e1` | 熔断器日志带 provider | 通过 |
| `f80252f7` | half-open permit 归还 | 通过；与上游内容一致 |
| `43206586` | DSH profile YAML 错误带文件名 | 通过 |
| `2dc34833` | 无 file ID 时校验 Codex fork | 通过；测试配套见 P1-2 |
| `3d58994e` | Agy Argon 周额度 | 通过；用户可见新增，见“范围” |
| `30cde555` | DB 建表/定价 seed savepoint | 通过；未发现事务冲突 |
| `1949a065` | WSL uninstall 去 mut | 与下条抵消，见 P3-1 |
| `fc5b4f41` | Codex state DB 单步备份 | 通过 |
| `4195d368` | Codex compaction 400 重试 | 通过；行为变化见“回归” |
| `344d06e9` | Kimi tool history 修复 | 通过 |
| `17d29028` | 第三方 Codex 剥 ChatGPT 身份头 | 通过；分类依赖见 P2-3 |
| `3ea399e0` | failover 按目标改模型名 | 通过；行为风险见 P2-2 |
| `6aeeeccd` | DeepSeek 同款 history 修复 | 通过 |
| `95da24b9` | 停止写 disable_response_storage | 通过；兼容性见 P2-1 |
| `b478ca2a` | 图片/搜索端点不映射模型 | 通过 |
| `e5424930` | catalog 测试断言对齐 | **部分通过**，见 P1-1 |
| `55d2ffd9` | WSL uninstall 保持 mut | 通过；与 1949 抵消，见 P3-1 |
| `8ee8536c` | v4.0.27 版本号 | 通过；缺 CHANGELOG，见 P3-2 |
| `7c5bbaa2` | 文档拆分 | 与本批代码无关，仅记录 |

---

## 六、本次**没有**合入的上游功能（确认）

以下 feat 未进入本分支，因此“只合 fix/perf”的范围控制基本成立：

- `deb6d5a1` sessions FTS5 全文搜索；
- `4ed410ad` TokenPlus provider presets；
- `5a3d7f92` Korean i18n；
- `cd15a164` / `b9fd5d99` / `2360631f` Genesis、Mistral 预设；
- `78971d56` Copilot 配额展示；
- `cf3c86fe` / `626f0226` Auth Center 套餐与官方卡套餐；
- `89409412` Grok Build 终端；
- `4e7413c5` 界面密度/缩放；
- `65106ce6` Skills 布局整理；
- `b0e452ff` / `e6256c50` / `e499d1f0` Codex 官方登录暂停、额度保护；
- `2c5e5d2c` / `c7f7caf2` / `6a0acf07` Codex 会话合并/提示；
- `905c9148` 第三方 Codex 路由写成内置 openai provider；
- 相关 release/docs/CI 提交（`3321cb9e`、`8656ebd9`、`62de0f3e`、`b3594905`、`c65a523c`、`c6a5a789`、`40b602a3`）。

**结论：** 没有把新功能主路径带进来。  
**但注意：** `3d58994e` 的 Argon 周额度行、`17d29028` 的身份头剥离、`95da24b9` 的配置清理，都是用户可感知的行为变更，不是“纯内部重构”。如果分支口径是“零新功能”，`3d58994e` 至少应单独确认是否符合口径。

---

## 七、建议的处理顺序

**合并前（P1）**

1. 修复 `e5424930` 的 WSL/UNC 测试断言：不要写死 `get_codex_model_catalog_path()`，改用实际 `catalog_pointer_value()` 或按 UNC 分支期望裸文件名。可参考上游 `07798d56`。
2. 补 `e1f8acf6` 的测试跳过逻辑：三个同 size/同 mtime 替换测试在 `identity.is_none()` 时 skip；同时把该盲区写入注释/已知限制。
3. 用 `cargo nextest` 或串行方式重跑 core / GUI 测试，确认不是“测试环境问题掩盖了真实失败”。

**发布前（P2）**

4. 确认 `disable_response_storage` 的最低 Codex 版本假设，写进发布说明；若仍需支持旧 fork，保留手动配置逃生口。
5. 真实回归 Codex 跨供应商故障转移：模型名替换、图片/搜索端点不替换、官方/托管账号不被误剥身份头。
6. 真实回归 symlink 配置写入：dotfiles 场景、悬空链接、Windows symlink/ReplaceFile 失败回退。

**观察项（P3）**

7. 版本号与 CHANGELOG/发布说明的一致性。
8. 在同步记录里标注 `1949a065` + `55d2ffd9` 的成对关系，避免后续重复 cherry-pick。

---

## 八、总体判断

这批合并**不是“带入了新功能”**，主要风险也不在业务逻辑本身，而在“**fix 与它的 test companion 被拆开**”：

- `ab3c1843` 带了 `e5424930`，但漏了 `07798d56` 的 UNC 测试口径；
- `2dc34833` 带了实现，但漏了 `e1f8acf6` 的无 file-ID 测试跳过；
- `1949a065` 是为了一个未合入的 feature 服务的，结果只能再补 `55d2ffd9` 抵消。

在**原生 Windows + NTFS**、**现有 CLI 命令集**下，没有发现这批代码会直接引入运行故障；编译和类型检查均通过。

但在 **WSL/UNC HOME**、**FAT/exFAT 或 9P 临时目录**、**旧版 Codex/fork**、**开启自动故障转移的第三方路由**这些边界上，当前合并会带来测试假红或行为变化，需要按上面的 P1/P2 项处理后再对外。

---

## 九、审计声明

- 本次审计**没有修改任何源码、Cargo 配置、lockfile、workflow 或测试文件**。
- 编译缓存和临时日志写入工作区未跟踪的 `.audit/` 目录。
- 运行时测试未完成，原因见第三节；不要把“编译通过”当成“运行时无回归”。
- 行号为当前 HEAD 近似值，后续提交可能移动；以函数名为准。
