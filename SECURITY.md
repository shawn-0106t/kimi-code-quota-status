# Security Policy / 安全策略

## 支持版本 / Supported Versions

仅最新 Release（`v*` 最新 tag 对应版本）接受安全修复 / Only the latest release receives security fixes.

| 版本 / Version | 支持状态 / Supported |
|---|---|
| 最新 tag / latest tag | ✅ |
| 更早版本 / older | ❌（请先升级 / please upgrade first） |

## 报告漏洞 / Reporting a Vulnerability

**中文**：请使用 GitHub 的**私密漏洞报告**（仓库 Security 标签页 → Report a vulnerability），**不要**为安全问题开公开 issue 或 Discussion。业余维护，通常 7 天内答复，确认后尽快修复并发布 Security advisory。

**English**: Please use GitHub's **private vulnerability reporting** (Security tab → Report a vulnerability). Do **not** open public issues for security matters. This is a hobby project — expect a response within ~7 days, followed by a fix and a security advisory once confirmed.

## 范围说明 / Scope

- quota-status 在本机读取 `~/.kimi-code/`（或 `KIMI_CODE_HOME`）下的 OAuth 凭证、缓存与配置；token **仅发往所配置的 base_url**（默认 Kimi 官方端点，可被环境变量 `KIMI_CODE_BASE_URL` 或 `quota-bar.toml [network] base_url` 覆盖），除此之外不向任何地址发送 token 或遥测。
- 值得报告的问题示例 / Examples worth reporting：token 泄漏或外发、缓存/凭证文件权限不当、sessionId 或路径处理导致的任意文件读写、渲染或取数路径中的注入与崩溃（panic）。
- 不在本仓库范围：Kimi Code CLI 自身的漏洞（请报告给上游官方）；statusline 渲染结果在终端中的视觉呈现问题（开普通 issue 即可）。
