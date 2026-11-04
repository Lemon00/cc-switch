# 解耦分支（decouple-core）上游同步手册

> 背景：这个拆分是 fork 专属的，上游不会收。上游每出新版，都要在这里同步一次。
> 上游地址：`https://github.com/farion1231/cc-switch`。
> 本分支结构：`crates/cc-switch-core`（后端，无 tauri）+ `src-tauri`（GUI 前端 + 门面）
> + `crates/cc-switch-cli`（CLI 前端）。

## 一、布局对照

上游的 `src-tauri/src/...` 在本分支分两处：

| 内容 | 位置 |
| --- | --- |
| 后端逻辑（数据库、代理、供应商、模式、服务层） | `crates/cc-switch-core/src/...`（原文件 `git mv` 过来，保留历史） |
| GUI 专属（Tauri 命令、托盘、OAuth/EventSink 的 Tauri 实现） | `src-tauri/src/...`，其中 `services/mod.rs` 是门面：GUI 自有服务 + core 重导出 |
| CLI 前端 | `crates/cc-switch-cli/src/main.rs`，只依赖 core |

## 二、上游同步步骤

```bash
git remote add upstream https://github.com/farion1231/cc-switch  # 已有则跳过
git fetch upstream
git checkout decouple-core
git merge upstream/main
```

推荐 merge，不推荐 rebase：fork 不往上游合，merge 的历史更直观，
且 rename detection 在 merge 里默认就开。同步前先记下当前 commit，
搞砸了 `git merge --abort`，或 reset 回记下的 commit。

## 三、冲突高发区（按这个清单归位）

1. **被移动的文件**：上游改了 `src-tauri/src/xxx.rs`，对应改本分支的
   `crates/cc-switch-core/src/xxx.rs`。merge 的 rename detection 一般能认出来；
   认不出来时手动把两边内容合到 core 那份，不要在 GUI 侧复活旧文件。
2. **上游新增 GUI 功能**：先判读放 core 还是 GUI。放 core 的，
   GUI 的 `services/mod.rs` 门面要补重导出；纯 Tauri 交互的留在 GUI 侧。
3. **Feature 门**：`session-monitor` 默认开（CLI 关），`session_usage_codex`
   不设门（v15→v16 数据库迁移依赖它的 reset）；`test-support` 只在 GUI 的
   dev-dependency 里开，正常构建不编。
4. **后台 runtime 铁律**：同步上下文禁止 `tokio::task::spawn`，必须走
   `core::background::spawn_background`。Tauri 的 setup 里没有 ambient tokio
   runtime，直接 spawn 会 `TryCurrentError` 启动即 panic（2026-11-04 实测踩过，
   死在 webdav `start_worker`）。async 函数内的 spawn 没问题。
   同步上下文目前有 4 处：webdav/s3 的 `start_worker`、codex 的 `after_refresh`、
   `start_background_checks`，新加的后台 worker 先查自己是不是同步上下文。
5. **可见性**：core 侧 `pub(crate)`→`pub`、私有 `mod`→`pub mod` 是常态代价，
   不要往回收。宏不能跨 crate：`lock_conn!` 这类调用改内联（已有先例）。
6. **配置目录覆盖**：core 的覆盖是 RwLock 语义，GUI 设置里改路径/清除路径时，
   必须成对调 core 的 set/clear 同步，缺一边就会用 stale 路径（2026-11-04 修过）。
7. **前端 dist**：编 GUI 前必须先编前端（`vite build` 出 `dist/`），
   tauri-build 编译时嵌 dist；本机 pnpm 不在 PATH 时 npm 也行。
8. **锁文件**：跟进 `src-tauri/Cargo.lock`；core 独立 check 生成的
   `crates/cc-switch-core/Cargo.lock` 不要提交。

## 四、前端应用白名单（P4 配置化）

应用选择是配置，不是代码。文件在应用配置目录（和 `cc-switch.db` 同目录），
名叫 `frontend-apps.json`：

```json
{ "cli": ["codex", "agy"], "tui": ["codex"], "gui": ["*"] }
```

规则（实现在 `core::frontend_apps`，任何失败都静默回退默认，绝不让启动变砖）：

- 缺文件、读不出、JSON 非法 → 内置默认。
- 缺某个前端的 key → 该前端用内置默认。
- `"*"` 表示全部应用；未知名字忽略；过滤后为空 → 内置默认。
- 内置默认：`cli` → `[codex, agy]`；其他前端 → 全部应用。
- GUI 目前不读这个文件（默认全量）；以后要给 GUI 加白名单时读 `gui` 那一项。

## 五、同步后的验证清单

```bash
# core 三档
cargo check -p cc-switch-core
cargo check -p cc-switch-core --no-default-features
cargo check -p cc-switch-core --features test-support
# core 不能再沾 tauri
cargo tree -p cc-switch-core | grep -i -E "tauri|wry|webkit|gtk"  # 应无输出
# CLI release 不能沾 webkit/gtk
cargo build --release -p cc-switch-cli && ldd crates/cc-switch-cli/target/release/cc-switch-cli
# GUI lib
cargo check --lib  # 在 src-tauri 下
# GUI 实跑：隔离 HOME + Xvfb，日志出现“主窗口已显示”、无 panic 才算过
```
