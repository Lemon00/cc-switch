# 项目文档

本文档索引对应当前工作树（`package.json` 版本 1.0.0，核对日期 2026-11-26）。版本号不代表工作树已发布，也不代表所有平台都已验收。

## 使用 CC Switch

- [中文用户手册](user-manual/zh/README.md)：安装、供应商、扩展、会话、路由、设置与故障排查。
- [聚合模式图文指南](guides/aggregation-mode-guide-zh.md)
- [Claude 官方账号管理](guides/claude-official-accounts-guide-zh.md)
- [Codex 官方登录保留](guides/codex-official-auth-preservation-guide-zh.md)
- [统一 Codex 会话历史](guides/codex-unified-session-history-guide-zh.md)
- [Codex Desktop 自定义模型可见性](guides/codex-desktop-custom-model-visibility-zh.md)
- 协议路由：[Claude 调用 Codex](guides/claude-codex-routing-guide-zh.md)、[Codex 调用 Claude](guides/codex-claude-routing-guide-zh.md)、[Codex 调用 DeepSeek](guides/codex-deepseek-routing-guide-zh.md)。
- 客户端接入：[Antigravity](agy-integration-zh.md)、[DeepSeek Harness](deepseek-harness-integration-zh.md)。
- 独立 CLI：[CLI 交互配置编辑（TUI）](cli-tui.md)。

## 开发与维护

- [贡献指南](../CONTRIBUTING.md)：环境准备、开发、打包与提交。
- [当前架构](architecture.md)：前端、桌面适配、核心库、CLI 的职责及一致性边界。
- [测试指南](testing.md)：可复跑命令、测试隔离、模块 13 回归入口和验证范围。
- [模块划分](../PROJECT_MODULE_REVIEW_PLAN.md)：16 个功能模块的编号、代码入口及审查边界。
- [Windows GNU 构建](windows-gnu-build.md)、[Flatpak 构建](../flatpak/README.md)。
- [应用内更新摘要](../src/whats-new/README.md)、[发布清单脚本](../scripts/README.md)。

## 发布说明与项目政策

[1.0.0 发布说明](release-notes/v1.0.0-zh.md) · [五应用范围更新（2026-11-26）](release-notes/five-apps-2026-11-26.md) · [CHANGELOG](../CHANGELOG.md)。仅维护独立版本记录，当前用法以用户手册为准。

[安全报告](../SECURITY.md) · [支持渠道](../SUPPORT.md) · [行为准则](../CODE_OF_CONDUCT.md)

## 文档维护约定

长期文档记录当前行为、职责和可复跑命令；临时审查清单、机器专属日志及已完成的迁移流水记录不作为长期文档维护。功能变化同时更新对应手册和测试入口，避免在多份报告中复制状态表。测试通过声明必须说明命令、平台和代码基线，历史记录不能代替当前工作树的验证。
