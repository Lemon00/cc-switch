# CLI 交互配置编辑

在交互式终端启动：

```bash
cc-switch-cli tui
cc-switch-cli --app codex tui
cc-switch-cli --app agy tui
```

从源码运行：

```bash
cargo run --locked --manifest-path crates/cc-switch-cli/Cargo.toml -- tui
```

TUI 使用与 CLI、桌面端相同的配置目录和供应商数据库，遵循 `CC_SWITCH_CONFIG_DIR` 及桌面端自定义目录。应用范围沿用 `frontend-apps.json` 的 `cli` 白名单，默认 Codex、Antigravity。不带命令仍显示 CLI 帮助。

## 编辑供应商

1. 用方向键选择供应商，`Enter` 或 `e` 打开编辑页；`[` / `]` 切换应用。
2. 用方向键或 `Tab` 选择名称、API 地址、Key、模型、原始 JSON；Codex 另有 `config.toml` 编辑入口。按 `Enter` 编辑。
3. 单行字段按 `Enter`、多行配置按 `Ctrl+S` 应用到草稿。回到字段列表后再按 `Ctrl+S` 保存供应商。

编辑器支持方向键、Home/End、PageUp/PageDown、删除和终端粘贴。`Ctrl+U` 清空当前字段。Key 字段默认隐藏，可按 `F2` 显示；原始 JSON 编辑器会显示完整配置，包括凭据。`Esc` 返回，未保存的修改需确认丢弃。

保存复用核心供应商服务。Codex 编辑器显示切到该供应商后的配置：连接与模型归供应商保存，TOML 中的全局设置修改写入原生配置；全局设置会影响其他供应商。当前直连供应商或代理配置按核心规则同步。已运行的 Codex 需重新启动才能读取新配置。Antigravity 卡片在下次 `launch` 时使用新值。

JSON/TOML 格式错误时保留编辑内容，不提交。编辑期间供应商被其他窗口修改或删除、Codex 全局设置发生冲突时拒绝保存；取消草稿，刷新并重新打开后合并修改。列表为空时先在桌面端添加或导入供应商，再按 `r` 刷新。此入口用于编辑已有配置，不提供新增、删除供应商或切换代理模式。

## CLI 设置

在列表页按 `Tab` 切到 CLI 设置，选择「代理用量记录」或「后台代理日志」，按空格或 `Enter` 切换，`Ctrl+S` 保存到 `cli.json`。只修改用户改动的开关，保留其他字段；损坏的文件会报错，修正后按 `r` 重新读取。

两个开关仅影响 CLI 自己的后台代理。已运行的代理需要重启：

```bash
cc-switch-cli --app codex proxy disable
cc-switch-cli --app codex proxy enable
```

`?` 或 `F1` 查看帮助，`q` 或 `Ctrl+C` 退出。输入或输出被重定向时，`tui` 会明确报错并退出。
