# quota-status

[![CI](https://github.com/shawn-0106t/kimi-code-quota-status/actions/workflows/ci.yml/badge.svg)](https://github.com/shawn-0106t/kimi-code-quota-status/actions/workflows/ci.yml) [![GitHub Release](https://img.shields.io/github/v/release/shawn-0106t/kimi-code-quota-status)](https://github.com/shawn-0106t/kimi-code-quota-status/releases)

[简体中文](README.zh-CN.md) | English (this is the authoritative version)

> The Chinese rendition ([README.zh-CN.md](README.zh-CN.md)) is synced per version. In case of any divergence, this English version prevails.

A statusline quota display for the Kimi Code CLI: a static single binary `quota-status.exe` (Rust, Windows x64) that shows your Kimi For Coding plan usage (5h / week / month + reset time) and background-task badges (`[N task(s) running]` / `[M agent(s) running]`) on footer line 1, with customizable field switches and in-line order.

## How it works

```
host (1s throttle, 300ms hard timeout) ──spawn──> quota-status.exe (render mode)
                                        │ reads stdin snapshot / cache / quota-bar.toml / config.toml[thinking]
                                        │ cache age ≥ TTL → rewind mtime + spawn detached:
                                        └──spawn(DETACHED|NO_WINDOW)──> quota-status.exe --refresh
                                                                                        │ GET {base_url}/usages (8s timeout)
                                                                                        │ parse → atomic cache write → exit
```

- **Render mode** (default): assembles one line of ANSI text purely from the local cache; exits in milliseconds; no network.
- **Fetch mode** `--refresh`: credential chain → GET /usages → defensive parsing → atomic cache write; any failure writes nothing to the cache (LKG preserved) and retries after 30s (fast-retry).
- **Self-check mode** `--test-fetch`: performs one full fetch and prints the result as pretty JSON to stdout; never writes the cache.
- **Version query mode** `--version`: prints `quota-status <version>` (same source as the exe's file properties) and exits; handy for verifying a deployed copy.
- The host is event-driven: idle sessions never invoke the command, so the "1-minute refresh" holds in active sessions; cache TTL defaults to 60s.
- Credentials are read-only: token freshness relies on the running CLI — this tool never refreshes the OAuth token itself.

## Installation

1. Download `quota-status.exe` from [Releases](https://github.com/shawn-0106t/kimi-code-quota-status/releases) (or build it yourself with `cargo build --release`; the artifact lands in `target/release/`) and put it in a fixed location (the examples use `C:\tools\`; any path works).
2. Edit `~/.kimi-code/tui.toml` and add:

   ```toml
   [status_line]
   command = "C:\\tools\\quota-status.exe"
   ```

   **Note**: in a TOML basic string, backslashes in Windows paths must be written as `\\`.
3. Run `/reload-tui` inside the Kimi Code TUI (or restart the CLI) to take effect.
4. Self-check: `C:\tools\quota-status.exe --test-fetch` printing `"error": null` means the fetch pipeline is healthy (`"error": "no-token"` means no usable credential right now — see Troubleshooting below).

## Configuration `~/.kimi-code/quota-bar.toml` (optional)

When the file is absent, everything falls back to the defaults below; invalid keys fall back to their defaults per key, rendering never fails because of configuration, and unknown fields are ignored. Paths follow `KIMI_CODE_HOME`.

```toml
[render]
# In-line field order (can be trimmed or reordered; fields not listed are not shown)
order = ["permission_mode", "model", "thinking", "tasks", "quota", "git_branch"]
# Monochrome switch: with false the output is plain text (no color codes at all);
# Kimi Code wraps the whole line in the theme text color — same color as the
# context readout on line 2, following /theme switches; true (default) keeps colors
colors = true

[render.quota]
five_hour  = true
week       = true
month      = true
reset_time = true   # false = permanently drop the reset-time suffix
booster    = false  # do not show the booster wallet balance by default (still parsed into the cache)

[thresholds]
# Percentage color thresholds: percent < green_below → green; < yellow_below → yellow; otherwise red
green_below  = 60
yellow_below = 85

[cache]
ttl_seconds   = 60   # cache validity window
retry_seconds = 30   # mtime-rewind window after a refresh is triggered = fast-retry interval

[network]
base_url = "https://api.kimi.com/coding/v1"  # international site: https://api.kimi.ai/coding/v1
http_timeout_seconds = 8
```

Environment variable precedence: `KIMI_CODE_BASE_URL` > `[network] base_url` > built-in default.

**tasks badge** (the default `order` already contains `"tasks"`): shows background-task counts for the current session — bash background tasks `[N task(s) running]` (with pid-liveness checking), background subagents `[M agent(s) running]`; cyan, placed ahead of the quota group; the whole segment is omitted when both counts are zero. No new configuration keys: removing `"tasks"` from `order` disables the segment (and skips the session-task directory scan); configurations that explicitly wrote an `order` in the v1.0 era must manually add `"tasks"` back to see the badge.

## Troubleshooting (read this first)

**① The quota line disappeared entirely** — check in order:

1. Are the `[status_line]` section and its `command` value still in `tui.toml`? Kimi Code's preference saves (e.g. `/theme`) **rewrite the whole file**: the current version round-trips the `command`/`items` values faithfully, losing only in-section comments and unknown keys; but **older CLI versions** (e.g. the 0.31.1 era) could silently drop the whole section.
2. Recovery when the whole section is lost: add the `[status_line]` config above back to tui.toml, then run `/reload-tui`.
3. Debug the fetch: `quota-status.exe --test-fetch` —
   - `"error": "no-token"`: the credential chain has no usable token. If the CLI is in an active session, wait for it to renew and retry; pure API-key users should confirm their config.toml provider `base_url` contains `api.kimi.com/coding` (or `api.kimi.ai/coding`) and a non-empty `api_key`.
   - `"error": "HttpRequestException"`: network down, or a non-managed-OAuth account (404 — that account type has no usages endpoint).
   - `"error": "TaskCanceledException"`: timeout (default 8s).
4. Quota group missing while other fields render fine: inspect the cache via `--test-fetch`; on first run / cache miss the quota group is omitted at first and appears within ≤1s after a successful backfill.

**Other**: when a render is killed by the host's 300ms process-tree kill, the last-good output is kept; if you suspect antivirus slowing the first launch, add the exe directory to exclusions.

## Build and test

```bash
cargo build --release          # artifact target/release/quota-status.exe (static CRT, no runtime dependencies)
cargo test                     # 68 unit tests
cargo test --test golden       # golden parity (33 golden cases, byte-for-byte)
```

The golden input payloads and expected outputs are replicated from `repos/kimi-planbar-tui` (read-only reference repository); all 33 golden cases are internalized in this repository under `testdata/golden/` (30 pre-existing + 3 native month cases), so tests do not depend on `repos/` at runtime. The fixed clock `atZero`/`atFracs` serialization is asserted against the local timezone `+08:00`.

## Acceptance records (2026-10-01)

| Item | Result |
|---|---|
| Static single exe | dumpbin /dependents shows only system DLLs (kernel32/bcrypt/advapi32/ntdll/ws2_32), no vcruntime |
| Size | 1,787,904 bytes (≈1.7MB) — **below the lower bound of the 3–5MB estimate in SPEC §11**; the budget was intended as an upper bound (≤5MB); actual is well under, no padding applied |
| End-to-end render | 100-run mean 33.79ms/run (PowerShell CreateProcess, including process start) < 50ms |
| Hot path (in-process) | render mean ≈ bare process-start baseline (35.7ms); in-process work only a few ms < 10ms |
| golden parity | 30 pre-existing cases + 3 native month cases byte-identical (after CRLF normalization) |
| Offline LKG | cache content unchanged under an unreachable base_url, no leftover processes (self-heals within 30s fast-retry + 60s TTL, ≤90s) |
| Non-managed OAuth | both `error:"no-token"` and 404 omit only the quota group; other fields render normally |

### Deployment acceptance (2026-10-01 22:43, confirmed in the user's CLI session)

| Item | Result |
|---|---|
| Deployment | exe deployed to `~/.kimi-code/bin/quota-status.exe` (outside target/, unaffected by cargo clean; **after rebuilding, re-copy to this path**; binary then 1,793,024 bytes, rebuilt after review fixes, still ≈1.7MB); tui.toml configured with `[status_line] command` (TOML parse verified, backup `~/.kimi-code/backups/tui.toml.20261001-223956.bak`) |
| `--test-fetch` self-check (§10.3) | `error: null`; fiveHour/week/resetAt all present, `fetchedAt` live; cache mtime/content unchanged before and after — no side effects |
| footer display (§10.4 item 3, first half) | after `/reload-tui`, footer line 1 correctly shows `permissionMode \| model \| thinking \| 5h N% (rst …) · week N% (rst …) \| gitBranch`; colors/separators/cross-day reset format match §7; CONOUT$ width query works in a real console (line complete, no accidental degradation — REVIEW3 Major 1 fix re-verified) |
| 1-minute auto update (§10.4 item 3, second half) | automatic backfill holds on a live machine in an active session: cache `fetchedAt` advanced 22:35:34 → 22:43:26, fiveHour percent changed 43→45 in real time (TTL 60s + mtime rewind + detached refresh) |
| `/theme` rewrite drill (§10.4 item 6) | not actually triggered this round: tui.toml mtime stayed at 22:40:34 with in-section comments intact — the host did not rewrite the file; command round-trip fidelity is guaranteed per the current main source (config.ts:298-312); recovery steps for old-version full-section loss are in Troubleshooting ① |

Render edge sampling (deployment day): empty stdin → exit 0, degraded as an empty payload; yolo→red, gitBranch null→omitted; empty anchor cache → mtime rewind (exactly now−30s) → detached refresh → atomic 401-byte cache write — the whole chain observed passing. Render end-to-end 20-run mean ≈44ms (measured through Git Bash pipes including shell fork overhead, an over-estimate) < 50ms.

### P7 tasks/agents badge acceptance (2026-10-02, isolated environment with a temporary KIMI_CODE_HOME)

| Item | Result |
|---|---|
| Badge rendering and position | a faked running bash task (pid pointing to a live child process) → output contains cyan `[1 task running]` ahead of the quota group, exit 0 |
| pid liveness check | rendering again after terminating the child process → badge gone |
| Path defense | sessionIds `../evil` / `a/b` / `a\b` / `.` / `C:evil` all produce no badge and exit 0 (whitelist `[A-Za-z0-9_-]`, stricter than the three rejection rules of SPEC §7.7 Decision B — code-review Minor 1 hardening) |
| Terminal statuses skipped | a `status:"completed"` task produces no badge |
| agent side | `kind:"agent"` running without pid → `[1 agent running]` (counted while running); alongside bash → `[1 task running] [1 agent running]` joined by a single space |
| Monochrome combination | with `colors = false` the badge follows the monochrome path as plain text, no SGR at all |
| Gates | fmt / clippy `-D warnings` / 66 unit tests (at first-review time; 68 after the fix round) / 33 golden parity / release build 1,800,704 bytes — all green; independent falsification-oriented code review concluded "deliverable" (0 Critical / 0 Major, 2 Minor hardenings landed) |
| Hot-path timing | real-world load (1 workspace + 1 task) 30-run mean on par with the no-scan baseline (81 vs 82 ms, Git Bash pipes including process start; delta ≈0); an artificially pathological setup (64 workspace probes + 32 task jsons + 32 pid checks maxed) adds ≈18ms, exceeding the §4.1 10ms in-process budget — unreachable in real sessions and >5× margin against the host's 300ms hard timeout; per the PLAN P7 risk section, tightening the 64/32 caps is a SPEC §7.7 Decision D contract value left to a decision rather than a unilateral change (user decision on the evening of 2026-10-02: keep as is) |
| Live TUI acceptance (PLAN criterion 5, evening of 2026-10-02) | while the second review's background independent process (`kimi -p`, a bash task with a pid) was running, `[1 task running]` appeared on footer line 1 (user runs colors=false, plain text color), and disappeared on the next re-render after the process ended — **all PLAN P7 acceptance criteria 1–5 pass** |
| Second independent code review (k3-256k/max, evening of 2026-10-02) | **Request Changes**: 1 Major (M-1: task-file read/parse failure was implemented as "skip the file and keep partial counts", contradicting the entire-segment-omitted semantics of SPEC §7.7 Decision E/§9, with a unit-test assertion written backwards) — resolved by user arbitration by **fixing the code toward the contract** (short-circuit to zero counts + assertion inverted + empty-state exemption tests; 67 unit tests green after the fix); of the 3 Minors the documentation ones were synced (SPEC/PLAN v1.5.1), the remaining Minor/Nit items are intentionally kept (details in docs/HANDOFF.md §7 addendum) |

## Known limitations

- Windows x64 only (`x86_64-pc-windows-msvc` + crt-static); no cross-platform support.
- `contextTokens`/`maxContextTokens` never enter the rendered line (the host shows them natively on footer line 2).
- The booster wallet is hidden by default; with `booster = true` it appears as `boost <balance>` (pure ASCII, cyan) appended inside the quota group (Ready state only).
- For international-site (api.kimi.ai) pure API-key users, the credential fallback depends on config.toml provider matching; OAuth credentials are unaffected.

## Changelog

See [CHANGELOG.md](CHANGELOG.md) (authoritative, English) / [CHANGELOG.zh-CN.md](CHANGELOG.zh-CN.md) (Chinese, synced).

## License

[MIT](LICENSE) © 2026 Shawn Qi (shawn-0106t); the quota endpoint and credential-loading logic are derived from [kimi-planbar](https://github.com/baigong-ai/kimi-planbar) (© baigong-ai, MIT); see [NOTICE](NOTICE) for attribution.
