# 安全报告 / Security reporting

本说明适用于 `Lemon00/cc-switch` 独立版本。维护目标为当前独立版本线，旧上游版本不在本仓库的发布范围内。

## 报告方式

不要在公开 Issue 中发布凭据或可直接利用的漏洞细节。优先使用[本仓库安全报告入口](https://github.com/Lemon00/cc-switch/security/advisories/new)。若仓库未开放私密报告，请先通过仓库公开的维护者联系方式请求私密沟通渠道。

请提供版本和平台、输入来源、复现步骤、影响、最小去敏样例，以及是否需要本地访问或用户确认。修复与发布记录以本仓库公告为准，不承诺固定处理时限。

## 输入边界

应用以当前用户权限运行，管理本机配置、数据库、账号和子进程。数据库、备份、同步包、深链接、远端模型数据、图标及代理请求都可能包含不可信输入；它们到文件写入、进程执行或 IPC 的完整路径属于报告范围。

本地路由的监听地址可配置，外部请求也属于输入边界。应说明请求如何到达监听器，以及已有认证、来源校验或客户端确认是否被绕过。

只展示在本地 DevTools 中直接调用 IPC，不足以证明远程攻击路径；如果能从外部输入、导入内容或渲染注入到达同一入口，请提供完整复现链。前端资源由安装包提供不意味着其中接收的所有数据可信。

## 数据与披露

配置和备份可能含供应商密钥、同步凭据与账号状态。报告前去除真实凭据，不要上传完整用户数据库。维护者会结合可达路径、所需权限及实际影响评估问题，并在修复发布后协调披露。

## English

Report vulnerabilities privately through this repository's security reporting entry. If private reporting is unavailable, request a private channel using the maintainer contact published in the repository. Do not post credentials or actionable exploit details in public issues.

Include the independent release version, platform, input origin, reproduction steps and impact. Untrusted deep links, imports, sync payloads, remote data and inbound proxy requests reaching privileged operations are in scope. Direct local DevTools calls alone do not establish a remote attack path. No fixed response deadline is promised.
