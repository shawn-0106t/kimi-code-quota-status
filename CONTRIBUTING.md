# Contributing to quota-status

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
   that applies. Do not bypass or ignore required checks; if one fails, fix
   the cause.
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
