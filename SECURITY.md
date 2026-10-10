# Security Policy

English | [中文](#安全策略)

## Supported Versions

Only the latest release (the version tagged with the newest `v*` tag) receives
security fixes.

| Version | Supported |
|---|---|
| latest tag | ✅ |
| older | ❌ (please upgrade first) |

## Reporting a Vulnerability

Please use GitHub's **private vulnerability reporting** (Security tab →
Report a vulnerability). Do **not** open public issues or Discussions for
security matters. This is a hobby project — expect a response within ~7 days,
followed by a fix and a security advisory once confirmed.

## Scope

- quota-status reads OAuth credentials, cache, and config files under
  `~/.kimi-code/` (or `KIMI_CODE_HOME`) on the local machine. Tokens are sent
  **only** to the configured base URL (the official Kimi endpoint by default,
  overridable via the `KIMI_CODE_BASE_URL` environment variable or
  `quota-bar.toml [network] base_url`). No tokens or telemetry are sent to
  any other address.
- Examples worth reporting: token leakage or exfiltration, improper
  permissions on cache/credential files, arbitrary file read/write via
  sessionId or path handling, injection or crashes (panics) in the rendering
  or fetching paths.
- Out of scope for this repository: vulnerabilities in the Kimi Code CLI
  itself (please report those to upstream); purely visual issues with how the
  statusline renders in a terminal (open a regular issue instead).

---

# 安全策略

[English](#security-policy) | 中文

## 支持版本

仅最新 Release（`v*` 最新 tag 对应版本）接受安全修复。

| 版本 | 支持状态 |
|---|---|
| 最新 tag | ✅ |
| 更早版本 | ❌（请先升级） |

## 报告漏洞

请使用 GitHub 的**私密漏洞报告**（仓库 Security 标签页 → Report a
vulnerability），**不要**为安全问题开公开 issue 或 Discussion。业余维护，
通常 7 天内答复，确认后尽快修复并发布 Security advisory。

## 范围说明

- quota-status 在本机读取 `~/.kimi-code/`（或 `KIMI_CODE_HOME`）下的
  OAuth 凭证、缓存与配置；token **仅发往所配置的 base_url**（默认 Kimi
  官方端点，可被环境变量 `KIMI_CODE_BASE_URL` 或
  `quota-bar.toml [network] base_url` 覆盖），除此之外不向任何地址发送
  token 或遥测。
- 值得报告的问题示例：token 泄漏或外发、缓存/凭证文件权限不当、
  sessionId 或路径处理导致的任意文件读写、渲染或取数路径中的注入与
  崩溃（panic）。
- 不在本仓库范围：Kimi Code CLI 自身的漏洞（请报告给上游官方）；
  statusline 渲染结果在终端中的视觉呈现问题（开普通 issue 即可）。
