# Contributing to quota-status

English | [中文](#贡献指南)

Thanks for your interest in contributing! quota-status is a small, focused tool:
a Rust static single binary (`target/release/quota-status.exe`, about 1.7 MB)
that renders Kimi For Coding quota in the Kimi Code CLI statusline footer.
Development and CI both assume **Windows x64** - the only supported target.

## Project ground rules

- **Windows x64 only.** Cross-platform support is out of scope.
- **No async runtime.** Blocking HTTP via `ureq` + rustls only; do not add
  tokio or other heavy dependencies (see the dependency whitelist in
  `docs/SPEC.md`, section 11).
- **Render path stays light.** No network and no heavy I/O while rendering
  (the host enforces a 300 ms timeout per render); fetching happens only in a
  detached child process that writes the cache file atomically.
- **Never touch OAuth tokens.** Token renewal is the running CLI's job; the
  binary only reads local credentials and sends them to the configured
  endpoint.
- **Security issues never go to public issues.** Use the private disclosure
  channel described in [SECURITY.md](SECURITY.md).

## Getting started

Install Rust via rustup. The MSRV is `rust-version` in `Cargo.toml`
(currently 1.97); the pinned toolchain in `rust-toolchain.toml` is picked up
by rustup automatically.

```sh
cargo build --release      # produces target/release/quota-status.exe
cargo test                 # unit tests + golden parity tests
cargo fmt --check          # formatting gate
cargo clippy --all-targets --locked -- -D warnings   # lint gate
```

Smoke test without touching your local cache (with no credentials configured
the expected result is `"error": "no-token"`):

```sh
target/release/quota-status.exe --test-fetch
```

Golden parity tests fail fast unless the local timezone is UTC+08:00 (the
fixtures pin datetime offsets to +08:00). CI sets `China Standard Time` on
`windows-latest` via `tzutil` for this reason; do the same locally.

## Required checks

Branch protection on `master` requires the CI `test` job to be green. CI runs:

| Gate | Command |
|---|---|
| gitleaks | full-commit-history secret scan (any finding fails the build) |
| Formatting | `cargo fmt --check` |
| Lints | `cargo clippy --all-targets --locked -- -D warnings` |
| Unit tests | `cargo test --lib --locked` |
| Golden parity | `cargo test --test golden --locked` |
| Release build | `cargo build --release --locked` |
| Size budget | exe must stay <= 5 MB (SPEC section 11) |

Run the same commands locally before opening a PR, and never commit secrets,
tokens, or credentials (CI scans the full commit history with gitleaks).

## Golden data policy

`testdata/golden/` is a parity contract: 33 fixtures — 30 ported byte-for-byte
from the upstream reference repository plus 3 native month cases. **Never
hand-edit expected output to make a
test pass.** If a rendering or parsing change legitimately alters output
bytes, open an issue to discuss the contract change first, then update the
fixtures together with the code in your PR (see the PR template's "Golden
parity impact" section).

## Contract and documentation changes

`docs/SPEC.md` (Chinese) is the authoritative contract; `docs/SPEC.en.md` is
its English translation, kept in sync per version. Behavior changes must
update, in the same PR:

- `docs/SPEC.md`, plus `docs/SPEC.en.md` when the change is user-visible
- `docs/PLAN.md`, when the change affects the plan or iterations
- `CHANGELOG.md` and `CHANGELOG.zh-CN.md` (bilingual changelog)

## Submitting changes

1. Fork the repo and create a topic branch.
2. Use [conventional commits](https://www.conventionalcommits.org) with short
   subjects: `feat:`, `fix:`, `docs:`, `chore(deps):`, etc.
3. Open a pull request using the provided template and fill in every section
   that applies. Do not bypass or ignore required checks; if one fails, fix the
   cause.
4. Dependency and CI-action upgrades are managed by Dependabot (weekly;
   patch/minor upgrades are grouped into a single PR). Please do not open
   competing upgrade PRs.

## Reporting bugs

Open a regular issue using the bug report template and include the output of
`quota-status.exe --test-fetch` (redact anything sensitive - no tokens or API
keys, ever). For vulnerabilities, follow [SECURITY.md](SECURITY.md) instead.

Questions about behavior? `docs/SPEC.md` (and its English mirror
`docs/SPEC.en.md`) is the contract of record; when code and docs disagree,
trust the SPEC and open an issue.

---

# 贡献指南

[English](#contributing-to-quota-status) | 中文

感谢你有意为 quota-status 贡献！quota-status 是一个小而专注的工具：Rust
静态单二进制（`target/release/quota-status.exe`，约 1.7 MB），在 Kimi Code
CLI 的 statusline footer 显示 Kimi For Coding 套餐额度。开发与 CI 均假定
**Windows x64**——唯一支持的目标平台。

## 项目基本规则

- **仅 Windows x64。** 跨平台支持不在范围内。
- **不引入 async runtime。** HTTP 只用阻塞式 `ureq` + rustls；不要加
  tokio 或其他重依赖（依赖白名单见 `docs/SPEC.md` §11）。
- **渲染路径保持轻量。** 渲染期间禁网络、禁重 IO（宿主对每次渲染强制
  300 ms 超时）；取数只发生在 detached 子进程中，原子写缓存文件。
- **绝不触碰 OAuth token。** token 续期是运行中 CLI 的职责；本二进制只
  读取本地凭证并发往所配置的端点。
- **安全问题绝不进公开 issue。** 请走 [SECURITY.md](SECURITY.md) 描述的
  私密披露渠道。

## 上手

通过 rustup 安装 Rust。MSRV 即 `Cargo.toml` 的 `rust-version`（当前
1.97）；`rust-toolchain.toml` 钉定的工具链会被 rustup 自动拾取。

```sh
cargo build --release      # 产出 target/release/quota-status.exe
cargo test                 # 单元测试 + golden parity 测试
cargo fmt --check          # 格式门禁
cargo clippy --all-targets --locked -- -D warnings   # lint 门禁
```

不触碰本地缓存的冒烟自检（未配置凭证时期望结果是
`"error": "no-token"`）：

```sh
target/release/quota-status.exe --test-fetch
```

golden parity 测试在本地时区非 UTC+08:00 时会 fail-fast（fixture 的
datetime 偏移固定为 +08:00）。CI 因此在 `windows-latest` 上用 `tzutil`
设置 `China Standard Time`；本地请同样处理。

## 必过检查

`master` 的 branch protection 要求 CI `test` job 绿。CI 运行：

| 门禁 | 命令 |
|---|---|
| gitleaks | 全提交历史敏感信息扫描（任何命中即失败） |
| 格式 | `cargo fmt --check` |
| lint | `cargo clippy --all-targets --locked -- -D warnings` |
| 单元测试 | `cargo test --lib --locked` |
| golden parity | `cargo test --test golden --locked` |
| Release 构建 | `cargo build --release --locked` |
| 体积预算 | exe 须保持 <= 5 MB（SPEC §11） |

开 PR 前本地跑同样的命令；绝不提交密钥、token 或凭证（CI 用 gitleaks
扫描全提交历史）。

## Golden 数据政策

`testdata/golden/` 是一份 parity 契约：33 个 fixture——30 个从上游参考
仓库逐字节移植，加 3 个原生 month case。**绝不手工编辑期望输出来让测试
通过。**若渲染或解析变更确实改变了输出字节，先开 issue 讨论契约变更，
再在同一个 PR 里连同代码一起更新 fixture（见 PR 模板的 "Golden parity
impact" 小节）。

## 契约与文档变更

`docs/SPEC.md`（中文）是权威契约；`docs/SPEC.en.md` 是其英文译本，随版本
同步。行为变更必须在同一个 PR 内更新：

- `docs/SPEC.md`；变更用户可见时一并更新 `docs/SPEC.en.md`
- `docs/PLAN.md`，当变更影响计划或迭代时
- `CHANGELOG.md` 与 `CHANGELOG.zh-CN.md`（双语 changelog）

## 提交变更

1. Fork 本仓库并创建 topic branch。
2. 使用 [conventional commits](https://www.conventionalcommits.org)，
   subject 保持简短：`feat:`、`fix:`、`docs:`、`chore(deps):` 等。
3. 用提供的模板开 pull request，填齐每个适用小节。不要绕过或无视
   required checks；失败就修根因。
4. 依赖与 CI action 升级由 Dependabot 管理（weekly；patch/minor 升级
   合并为单个 PR）。请勿另开竞争性升级 PR。

## 报告 bug

用 bug report 模板开普通 issue，并附上 `quota-status.exe --test-fetch`
的输出（脱敏一切敏感内容——绝不包含 token 或 API key）。漏洞请改走
[SECURITY.md](SECURITY.md)。

对行为有疑问？`docs/SPEC.md`（及其英文镜像 `docs/SPEC.en.md`）是事实
契约；代码与文档不一致时，以 SPEC 为准并开 issue。
