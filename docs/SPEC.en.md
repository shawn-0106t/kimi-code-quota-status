# quota-status Contract Document (SPEC)

> English rendition of [`docs/SPEC.md`](SPEC.md) (Chinese, authoritative); the two are synced per version. In case of any divergence, the Chinese version prevails.

- Project: quota-status — a Kimi Code CLI statusline quota display implemented in Rust (single binary `quota-status.exe`)
- Version: v1.6 (2026-10-10; added section 4.5, the `--version` query mode; the section 11 dependency whitelist now admits the build-dependency `winresource` — via `build.rs` it embeds a Windows VERSIONINFO resource whose version comes from `CARGO_PKG_VERSION`, staying consistent with the `--version` output and the release workflow's tag<->version check; no render/fetch path changes)
- Version history: v1.5.2 (2026-10-03; §11 CI/Release engineering entry now records that all jobs across the two workflows declare explicit least-privilege `permissions` (per-job `contents: read` in ci.yml, workflow-level `contents: write` in release.yml) — fixes the medium alert `actions/missing-workflow-permissions` from the first CodeQL default-setup scan; engineering-record-only change, no behavior change); v1.5.1 (2026-10-02; revised after the second independent code review (k3-256k/max): §7.7 Decision B documents the sessionId character whitelist `[A-Za-z0-9_-]` semantics, Decision E adds the NotFound empty-state exemption, §10.1 adds two unit-test items for failure semantics and the empty state; same-round code fix: a task-file read/parse failure short-circuits to zero counts with the entire segment omitted — a Major contract conflict resolved by user arbitration to fix the code toward the contract; see CHANGELOG [1.1.0] Fixed); v1.5 (2026-10-02; added the tasks/agents badge: §7.7 data source and defense rules (Decisions A–E), with consequential updates to §7.1/§7.2/§8/§10.1/§12, including the Decision C split by kind and the exclusion of question tasks — both settled through two rounds of arbitration; full change notes in git history); v1.4 (2026-10-02; monochrome render switch: §7.2 colors=false monochrome semantics, §7.4 plain-character separators, §8 [render] colors key, §10.1 unit-test items); v1.3.2 (2026-10-01; §11 records the CI/Release engineering infrastructure and the CHANGELOG convention); v1.3.1 (2026-10-01; golden test data internalized into this repository's `testdata/golden/`, CI self-sufficient, §10.2 synced); v1.3 (2026-10-01; revised after the third independent code review: §4.1 freshness check pinned to pure-mtime semantics, §5.1 error classification refined by send/body phase, §9 corrupt-cache row synced); v1.2 (2026-10-01; revised after the P0–P5 implementation landed); v1.1 (2026-10-01; revised F1–F8 per the 8 findings of the second review REVIEW.md); v1.0
- If this document conflicts with the finalized decision list, the decision list prevails (this document faithfully incorporates the decision list; two factual errata are recorded in §2.3)
- Path convention: all `path:line` references are relative to the repository root (the parent of the `docs/` directory containing this file)

---

## 1. Overview and Goals

### 1.1 Problem and Shape

The Kimi Code CLI footer supports a user-defined statusline command (`tui.toml [status_line] command`): the host feeds a JSON snapshot to stdin and takes the first stdout line to replace footer line 1, but the whole subprocess is capped at **300ms** (`repos/kimi-code/apps/kimi-code/src/tui/utils/status-line-command.ts:13`). Quota data comes from the remote `GET /usages`; a network round trip cannot reliably fit into 300ms. Therefore:

**quota-status is a static single binary with three cooperating run modes**: the render mode reads only the local cache (done in milliseconds); when the cache expires it spawns a detached child process to run the fetch mode and backfill the cache; the parent never waits on the network. This simultaneously satisfies three user requirements:

1. **Quota data refreshes every minute** — cache TTL 60s + detached background fetch;
2. **Statusline fields are customizable** — `~/.kimi-code/quota-bar.toml` controls field switches and in-line order;
3. **Lightweight and fast** — single exe, no runtime dependencies, the render path forbids network and heavy IO (budget <10ms), target size ≤5MB (v1.2: the original "3–5MB" was an estimated range; measured ≈1.7MB).

### 1.2 Non-goals

- hooks data layer, plugin/MCP integration
- Forking the official Kimi Code source
- Interactive TUI
- Showing the booster wallet balance by default (it is still parsed into the cache, reserved for a future config extension)
- Auto-refreshing the OAuth token (token freshness relies on the running CLI; this tool never refreshes tokens itself)
- Cross-platform (the first version is Windows x64 only)

---

## 2. Terms and Reference Repositories

### 2.1 Terms

| Term | Meaning |
|---|---|
| host | The Kimi Code CLI TUI, i.e. the process that invokes the statusline command |
| render mode | quota-status default behavior: read stdin snapshot + local cache → assemble one line of ANSI text |
| fetch mode | `--refresh`: credential chain → GET /usages → parse → atomic cache write |
| self-check mode | `--test-fetch`: one full fetch, prints the parsed result as JSON, then exits (headless) |
| quota group | The combined segment built from the 5h / week / month percentages |
| LKG | last-known-good: the cached data from the last successful fetch |

### 2.2 Reference repositories and reused assets (extracted directly during implementation)

| Asset | Path | Lines (verified) | Extraction approach |
|---|---|---|---|
| Credential chain | `repos/kimi-planbar-tui/rust/src/credentials.rs` | 147 | Extracted verbatim (including unit tests, credentials.rs:115-147) |
| HTTP client semantics | `repos/kimi-planbar-tui/rust/src/http.rs` | 16 | Extracted the "OnceLock shared client + fall back to None on build failure" semantics (http.rs:11-16); **timeout changed from 10s to 8s** (decision list fixed 8s) |
| Quota parsing | `repos/kimi-planbar-tui/rust/src/quota.rs` | 374 | Extracted parsing and defense rules; **5h segment matching upgraded** to window conditional matching (see §6.4), **month segment added** (see §6.6) |
| Retry rules | `repos/kimi-planbar-tui/rust/src/polling.rs` | 74 | Extracted the "keep LKG on failure + 30s fast-retry" semantics (polling.rs:20-41); the periodic loop is not ported (this tool has no resident process; an equivalent retry is achieved via the mtime rewind, see §5.3) |
| Golden test matrix | `repos/kimi-planbar-tui/go/testdata/golden/quota-*.txt` | 30 cases | parity alignment (see §10.2); input payloads are inlined at `repos/kimi-planbar-tui/go/internal/core/quota_test.go:240-266` |
| API contract reference | `repos/kimi-planbar-tui/docs/SPEC.md` §16 (lines 406-465 of the 694-line document) | — | §16.1-16.5 is the upstream contract behind this SPEC's §5/§6 |
| Render segment reference | `repos/kimi-planbar/quota-status.py` | 227 | Semantic source for color codes, separators, reset format, the thinking ladder, and UTF-8 handling (Python prototype) |

Only three blocks are newly written: the **render module + field-config module + cache module**, estimated at 300–500 lines.

### 2.3 Factual errata against the decision list (recorded as-is)

1. The decision list says the golden matrix is at `repos/kimi-planbar/go/testdata/golden/quota-*.txt` — that path **does not exist** (verified: `ls` returns No such file or directory). The actual location is `repos/kimi-planbar-tui/go/testdata/golden/` (37 files in the directory, exactly 30 of them `quota-*.txt`; `ts/test/golden/` and `ts-nodejs/test/golden/` each hold a copy of the same content). This document always cites the actual paths.
2. The decision list describes the host contract as a "1s throttle": the precise semantics are **at least 1s between executions of the same command** (`STATUS_LINE_RERUN_INTERVAL_MS = 1_000` at status-line-command.ts:14; the throttle check is at status-line-command.ts:139-149); updates triggered during the interval are delayed, not dropped.

---

## 3. Host Contract: Kimi Code CLI statusline

> Every claim in this chapter has been verified against the official source code.

### 3.1 Enabling

`~/.kimi-code/tui.toml`:

```toml
[status_line]
command = "C:\\tools\\quota-status.exe"
```

- The host reads `status_line.command` and enables the feature only if it is non-empty after trim (`repos/kimi-code/apps/kimi-code/src/tui/config.ts:238`; empty string becomes null, config.ts:287-290).
- To take effect: run `/reload-tui` in the TUI (command registered at `repos/kimi-code/apps/kimi-code/src/tui/commands/registry.ts:304`, dispatched at dispatch.ts:512-513) or restart the CLI.
- Backslashes in Windows paths must be escaped in TOML strings; the host applies the same escaping via `escapeTomlBasicString` when writing tui.toml back (config.ts:311).

### 3.2 Process invocation contract

- On Windows the host launches the command via `cmd.exe /d /s /c <command>` (status-line-command.ts:46, with `%ComSpec%` falling back to `cmd.exe`); on POSIX via `sh -c`. The command is a full command string; the exe must be an absolute path.
- `stdio: ['pipe', 'pipe', 'ignore']` — **stderr is discarded by the host** (status-line-command.ts:47); diagnostics may be written to stderr without affecting rendering.
- The env var `KIMI_CODE_STATUS_LINE=1` is injected (status-line-command.ts:48); it may be used to recognize the calling context but must not be relied upon to exist.
- stdin: the host writes the JSON-serialized payload and closes the pipe (`child.stdin?.end(JSON.stringify(payload))`, status-line-command.ts:113). Render mode must read to EOF before parsing.
- stdout: **only the first line is taken** (`stdout.split('\n')[0].trimEnd()`, status-line-command.ts:106); later lines are ignored. An empty first line → treated as failure (same file, :107).
- Exit code: non-zero → treated as failure (status-line-command.ts:100-104). Render mode exits 0 under all circumstances.
- Failure (spawn error / non-zero exit / timeout / empty output) uniformly yields null and the host falls back to its built-in footer layout (status-line-command.ts:4-8 comment, footer.ts:316-319).

### 3.3 Timeout, throttle, and capture caps

| Item | Value | Evidence |
|---|---|---|
| Per-run timeout | 300ms; on timeout, Windows `taskkill /pid <pid> /T /F` kills the whole process tree | status-line-command.ts:13, 58-78 |
| Rerun throttle | ≥1s interval; payloads arriving while in-flight are recorded as pending and land when the interval expires | status-line-command.ts:14, 139-149 |
| stdout capture cap | 64KB (65536 bytes); accumulation stops at the first newline | status-line-command.ts:15, 84-95 |
| Result cache | the runner caches the last-good line; **failures do not clear it** — the previous successful output keeps showing | status-line-command.ts:174-179 |

Corollary: invocation is **event-driven** — the command is spawned only when the footer re-renders (at most once per second in an active session, bounded by the throttle; an idle session — no input, no streaming, no active goal — is never invoked at all: the only trigger is footer.ts:312 and the only periodic timer is syncGoalTimer during goal activity, footer.ts:529-537). The quota line's update semantics are therefore "≤1s within an active session + the next render after TTL expiry", with data unchanged while idle (§10.4 item 3); our output line (worst case ~200 chars) is far below the 64KB cap.

### 3.4 footer two-line behavior

- **Line 1**: the statusline command's stdout first line **takes over the whole line** (`apps/kimi-code/src/tui/components/chrome/footer.ts:311-318`); the host additionally wraps it in the theme foreground color `chalk.hex(colors.text)(customLine)` (footer.ts:318) — the ANSI SGR sequences we embed still apply in order, unaffected.
- **Line 2**: rendered natively, not customizable. When a command takes over line 1: the left side shows the ctrl+o hint (transient/warning hints take the left side when present, footer.ts:374-384, 386-394), and the right side shows `context: N% (abbrevTokens/abbrevMax)` — tokens abbreviated in base 1024 (footer.ts:177-183 `formatContextStatus` + `formatTokenCount`); with the native layout (no command) the left side is empty.
- Therefore `contextTokens` / `maxContextTokens` **explicitly stay out of this tool's rendered line** — footer line 2 already shows them natively.
- Both lines are finally truncated to the terminal width via `truncateToWidth` (footer.ts:397) — the host-side last-resort truncation; we should degrade on our own before outputting (§7.5) so the host never hard-truncates.

### 3.5 stdin payload schema (`StatusLinePayload`, status-line-command.ts:17-28)

| Field | Type | Use in this tool |
|---|---|---|
| `model` | string (displayName; constructed at footer.ts:509) | renders the model segment; used for thinking-ladder fallback matching |
| `cwd` | string | unused (decision: cwd is not in the field list) |
| `gitBranch` | string \| null | renders the gitBranch segment |
| `permissionMode` | string | renders the permissionMode segment |
| `planMode` | boolean | unused |
| `contextUsage` | number | unused (§3.4) |
| `contextTokens` / `maxContextTokens` | number | unused (§3.4) |
| `sessionId` | string | locates the session task directory (tasks/agents badge, §7.7; enabled since v1.5) |
| `version` | string | unused |

---

## 4. Run Modes and Process Model

```
host (1s throttle) ──spawn──> cmd.exe ──> quota-status.exe (render mode, <300ms)
                                          │ reads stdin snapshot / cache / quota-bar.toml / config.toml[thinking]
                                          │ cache age ≥ TTL → rewind mtime + spawn detached:
                                          └──spawn(DETACHED|NO_WINDOW, stdio DEVNULL)──> quota-status.exe --refresh
                                                                                          │ credential chain → GET /usages (8s timeout)
                                                                                          │ parse → atomic cache write → exit
```

### 4.1 Render mode (default, no arguments)

1. Read stdin to EOF, parse the JSON after `String::from_utf8_lossy` (parse failure is treated as an empty payload, same semantics as quota-status.py:177-181).
2. Read the cache `~/.kimi-code/cache/quota-status.json` (§5.3).
3. Determine cache age (file mtime): `age ≥ TTL` → first rewind the mtime, then spawn a detached `--refresh` child process, **without waiting for or reading any of its output**. The freshness check looks **only at mtime**, independent of whether the cache content is parseable (pinned in v1.3: an empty anchor / corrupt cache / persistently failing refresh is equally suppressed by the rewound mtime, retrying automatically after retry-seconds — the same pure-age semantics as quota-status.py:135-149 `maybe_refresh`).
4. Assemble one line of ANSI-colored text per the field switches and order in `quota-bar.toml` (§7), write it to stdout (UTF-8) + newline, exit 0.
5. When no field is renderable, output an empty line — the host treats an empty first line as failure and falls back to its built-in layout (§3.2); this is the desired behavior.
6. The render path forbids network and heavy IO: it may only read 3 small files (the cache JSON, the thinking-related sections of `config.toml`, and `quota-bar.toml`) + one console width query + one tasks-directory scan (relaxed in v1.5, §7.7; bounded by scan caps — ≤64 workspace probes, ≤32 task-json reads); hot-path budget **<10ms**, process end-to-end (including startup) budget <50ms.

### 4.2 Fetch mode `--refresh`

1. Obtain the token via the credential chain (§5.2); no token → exit immediately (no cache write).
2. `GET {base_url}/usages` (HTTP timeout 8s).
3. Defensive parsing (§6).
4. On success: `mkdir -p` the cache directory, write `quota-status.json.<pid>.tmp`, then atomically replace via `fs::rename` (v1.2: the tmp carries a PID suffix, eliminating the race window of concurrent refreshes interleaving on the same tmp; on Windows std rename carries `MOVEFILE_REPLACE_EXISTING`, equivalent to `os.replace`, same semantics as quota-status.py:127-131). What gets written is the **parsed structured JSON**, not a rendered string.
5. On failure (network / non-2xx / parse failure): **do not write the cache** — the old data and the rewound mtime are preserved as-is; the render mode naturally triggers again after 30s (fast-retry, see §5.3).
6. `--refresh` always exits 0 (no consumer reads its exit code; failure is expressed by "the cache was not updated").

### 4.3 Self-check mode `--test-fetch`

- Performs exactly the same fetch+parse as `--refresh`, prints the parsed result as pretty JSON (2-space indent, camelCase, §5.3 schema) to stdout, then exits.
- Headless: does not read stdin and **never writes the cache** (keeps automated tests side-effect-free and repeatable).
- Errors also print JSON (the `error` field holds the error type name, §6.10), exit 0.
- Purpose: end-to-end self-check on a live machine + the output-format baseline for parity tests (§10.2/§10.3).

### 4.4 Detached refresh and storm prevention

- Child process: `Command::new(<own absolute path>).arg("--refresh")` with `creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)` (0x08000000 | 0x00000008) and all three stdio streams to DEVNULL. Semantics correspond to the Python prototype's `start_new_session` + DEVNULL (quota-status.py:146-149).
- The parent continues rendering right after spawning and exits; the refresh runs independently. The renderer exits far before the 300ms timeout, so the host's `taskkill /T` never reaches the detached child.
- **Refresh-storm prevention**: before spawning, render mode rewinds the cache mtime to `now - TTL + 30s` (same semantics as quota-status.py:143-145) — subsequent renders see "not expired" for 30s and do not spawn again; if the refresh fails (cache untouched), it retries automatically after 30s. Concurrent renders from multiple sessions all read the same rewound mtime, so only a bounded number of spawns occur.

### 4.5 Version query mode `--version` (v1.6)

- Prints `quota-status <CARGO_PKG_VERSION>` to stdout, then exits 0; reads no stdin and touches no credentials, cache, or any file IO.
- Shares its compile-time `CARGO_PKG_VERSION` source with the Windows VERSIONINFO resource embedded by `build.rs` (`winresource`, section 11): `tag = Cargo.toml = exe file properties = --version output` — a four-way consistency complementing the release workflow's tag<->version check.
- Purpose: post-deploy version verification — after `deploy.ps1` copies the exe to `~/.kimi-code/bin/`, the version can be confirmed without a sha256 comparison.

---

## 5. Data Layer Contract

### 5.1 Endpoint

- Default: `GET https://api.kimi.com/coding/v1/usages`; international site `https://api.kimi.ai/coding/v1` (dual-site source: the official oauth package managed-usage.ts:8-9 `DEFAULT_KIMI_CODE_BASE_URL` / `GLOBAL_KIMI_CODE_BASE_URL`; env override at :18).
- Override precedence: env `KIMI_CODE_BASE_URL` > `quota-bar.toml [network] base_url` > built-in default. The base value must include the version segment (e.g. `.../coding/v1`); the tool strips any trailing `/` and appends `/usages`.
- Headers: `Authorization: Bearer <token>`, `Accept: application/json` (quota.rs:85-86, kimi-planbar-tui SPEC §16.1).
- HTTP timeout **8s** (decision list; note the extracted http.rs originally used 10s, http.rs:14 — changed in implementation). Error classification: timeout → `TaskCanceledException`, other transport errors → `HttpRequestException` (quota.rs:92-98).
- Error classification is refined by phase (pinned in v1.3, removing the ambiguity of "other transport errors"): send-phase timeout → `TaskCanceledException`; non-2xx and other send-phase transport errors → `HttpRequestException`; body-phase (headers already received) timeout → `TaskCanceledException`; other body-phase errors (connection reset, invalid UTF-8, etc.) → `JsonException` (the body non-timeout classification matches the reference implementation's reqwest `.json()` semantics, repos/kimi-planbar-tui/rust/src/quota.rs:103-106; the body-phase timeout follows this tool's established rule as `TaskCanceledException`, an intentional deviation from the reference implementation).
- Only managed OAuth (Kimi For Coding) accounts have this endpoint; pure API-key accounts get 404 (official message verbatim: "Usage endpoint not available. Try Kimi For Coding.", managed-usage.ts:240-241). All non-2xx go into the `HttpRequestException` failure path (quota.rs:100-102).

### 5.2 Credential chain (`load_token`, extracted from credentials.rs:44-66)

`<kimi_home>` = `%USERPROFILE%/.kimi-code`, overridable by the env var `KIMI_CODE_HOME` (credentials.rs:35-42):

1. **`<kimi_home>/credentials/kimi-code.json`** → `access_token` (string); validate `expires_at` (Unix seconds, number-or-string) > now + 30s; if expired, treat as invalid and continue (credentials.rs:49-60). Token refresh relies on the running CLI; **this tool never refreshes tokens itself**.
2. **Fallback `<kimi_home>/config.toml`**: parse line by line (not a full TOML parser), looking for a section whose name starts with `providers.`, whose `base_url` contains `api.kimi.com/coding`, and whose `api_key` is non-empty; return its `api_key`; when a new section starts, finalize the previous one (credentials.rs:62-113). Settled in v1.2: the match string now also accepts `api.kimi.ai/coding` (international-site pure API-key users get the same fallback; the original reference implementation only matched `api.kimi.com/coding`); parsing is hand-written key-value extraction rather than regex (regex is not in the §11 dependency whitelist), with semantics pinned by ported unit tests.
3. Neither present → no token: `--refresh` exits silently, `--test-fetch` outputs `error = "no-token"`, and render mode omits the quota group entirely.

### 5.3 Cache

- Path: `<kimi_home>/cache/quota-status.json` (follows `KIMI_CODE_HOME`).
- Content: parsed structured JSON (camelCase). The schema extends kimi-planbar-tui's `QuotaResult` (quota.rs:40-48) **with a `month` field**:

```jsonc
{
  "fiveHour": { "percent": 21.0, "resetAt": "2030-01-01T00:00:00+08:00" },  // Option
  "week":     { "percent": 18.5, "resetAt": "2030-01-08T00:00:00+08:00" },  // Option
  "month":    { "percent": 43.0, "resetAt": "2030-10-31T00:00:00+08:00" },  // Option, added by this tool; skipped during serialization when None
  "extra":    { "state": "Ready", "balanceCents": 1235, "monthlyEnabled": true,
                "monthlyUsedCents": 4567, "monthlyLimitCents": 10000 },       // Option; booster is parsed into the cache but not rendered by default
  "fetchedAt": "2030-01-01T08:00:00.123456789+08:00",
  "error": null
}
```

- TTL defaults to **60s** (overridable via `quota-bar.toml [cache] ttl_seconds`; the decision list fixed 60s; the Python prototype's 300s was not adopted).
- Atomic write: same-directory `.tmp` with a PID suffix + rename (§4.2 step 4).
- Expiry check and mtime rewind: see §4.4; the rewind writes mtime via the `filetime` crate (Windows `SetFileTime`).
- First run / cache missing: render mode creates an empty cache file to anchor the mtime (same as quota-status.py:137-139), then rewinds + spawns; the quota group is not shown yet.
- **Any failure preserves LKG**: a failed refresh never writes the cache (§4.2 step 5) and rendering never clears existing quota data — semantically equivalent to kimi-planbar-tui's `fill_missing_from` (quota.rs:63-73, polling.rs:22-27, SPEC §16.5 step 2).

---

## 6. Parsing Defense Rules (recorded item by item, continuing the kimi-planbar-tui lessons-learned contract)

> Upstream contract: `repos/kimi-planbar-tui/docs/SPEC.md` §16.3 (lines 429-441); reference implementation quota.rs (with unit tests at quota.rs:243-374).

1. **Numeric fields are modeled as strings, with real-number fallback**: server-side numeric fields such as `used`/`limit`/`amountLeft`/`priceInCents`/`expires_at` are parsed as strings, and a true JSON number yields the same result (`get_f64`/`get_i64`, quota.rs:135-150; golden `quota-mixed_string_number`).
2. **limit ≤ 0 is clamped to 1 to prevent division by zero**: clamped before computing `percent = used/limit*100` (quota.rs:180-183; golden `quota-div_zero`, `quota-neg_limit`).
3. **NaN/inf/hostile integers normalize to zero**: non-finite percents produced by hostile strings (`"1e999"`, `"NaN"`) normalize to 0, and serialization never emits a non-finite double (quota.rs:188-193); hostile `i64` values (e.g. the string `"-9223372036854775808"`, i.e. `i64::MIN`) must be parseable — that case is the monthly `priceInCents`, stored directly via `parse_cents` without conversion (unit test quota.rs:359-373); the `saturating_add` in the balance conversion guards the **positive overflow** of an amountLeft near `i64::MAX` (quota.rs:229-230 comment verbatim "a pathological amountLeft near i64::MAX must not overflow"; golden `quota-amount_huge`).
4. **The 5h segment prefers window matching**: within `root.limits[]`, prefer the element with `window.duration == 300` and `window.timeUnit == "TIME_UNIT_MINUTE"`, falling back to `limits[0]` if absent; the segment's `detail` feeds `parse_segment` (quota-status.py:78-82). Note that the extracted quota.rs:115-123 currently only takes `limits[0]`; implement per this window-matching semantics (decision list).
5. **The week segment takes top-level `root.usage`** (parsed only if an object; quota.rs:124-129).
6. **The month segment takes `root.totalQuota`; a segment exists only with a limit**: no month segment when `totalQuota.limit` is missing, 0, or non-finite (NaN — `NaN != 0.0` is always true, so it must be excluded explicitly, recorded in v1.2) (quota-status.py:93-97). This segment is new in this tool; quota.rs and the existing goldens have no month field.
7. **resetTime lenient parsing**: RFC3339 first; on failure fall through a lenient ladder (quota.rs:158-174) — 4 offset variants (space or `T` separator + `%:z`/`%z` offset), of which **only `%H:%M:%S%.f %:z` (space-separated + offset) supports fractional seconds**; 2 naive variants (`%Y-%m-%d %H:%M:%S`, `%Y-%m-%dT%H:%M:%S`) do not support fractional seconds; without an offset, the local timezone applies (golden `reset_time.txt`, `quota-reset_fraction`).
8. **boosterWallet defense**: non-object or missing → `NotActivated`; `isEnabled === false` → `NotActivated` — **in that case `amountLeft` is an estimate of "monthly cap − used", not a real balance, and must not be treated as one** (quota.rs:204-220; golden `quota-disabled_wallet`). Only a strict boolean `false` triggers the defense; a non-boolean `isEnabled` (string `"false"`, number 0, etc.) does not trigger it and balance parsing continues on the enabled path (quota.rs:218 `as_bool` only recognizes strict booleans; string "false" case at quota_test.go:137-141, number 0 case in golden `quota-isenabled_zero`'s inlined payload (quota_test.go:255); both goldens have state=Ready).
9. **Balance unit conversion**: `balance.amountLeft` is in units of 1e-8 yuan; `cents = (raw + 500000) / 1_000_000` (round-half-up; quota.rs:222-233; golden `quota-amount_frac_number`, `quota-amount_negative_round*`).
10. **Error type names follow .NET style** (for cross-version diffing of `--test-fetch` output): `"no-token"` / `"HttpRequestException"` / `"TaskCanceledException"` (timeout) / `"JsonException"` (quota.rs:77-105, kimi-planbar-tui SPEC §16.4).

---

## 7. Render Contract

### 7.1 Fields and order (user-selected; switches and order controlled by quota-bar.toml)

Default in-line order: **permissionMode → model → thinking level → tasks/agents badge → quota group (5h/week/month) → gitBranch** (since v1.5 the tasks badge sits ahead of the quota group). Each segment shows only when it has a value; segments are separated by a gray `|` (§7.4).

| Segment | Data source | Shown when |
|---|---|---|
| permissionMode | stdin `permissionMode` | non-empty string |
| model | stdin `model` | non-empty string (schema guarantees a string, status-line-command.ts:18; non-strings ignored) |
| thinking | side-read of `<kimi_home>/config.toml` | see §7.2 |
| tasks badge | session task directory scan (§7.7, v1.5) | at least one running task counted — agent side counted while status=running, bash side via pid-liveness check (§7.7 Decision C); the entire segment is omitted when both counts are zero |
| quota group | cached fiveHour/week/month | the corresponding segment exists and is enabled |
| gitBranch | stdin `gitBranch` | non-null and non-empty |

The thinking ladder (not part of the stdin snapshot; same semantics as quota-status.py:101-119):

1. `[thinking] enabled = false` → render gray `off`;
2. otherwise take `[thinking] effort`;
3. if missing, fall back: find the `[models.*]` entry whose `display_name` or `model` equals the current stdin `model`, take `overrides.default_effort`, then fall back to `default_effort`;
4. if none of these work → omit the segment. When the field switch is off, skip the whole segment (and skip reading config.toml, saving IO).

`contextTokens`/`maxContextTokens` stay out of this line (§3.4). The booster wallet is hidden by default (still parsed into the cache, reserved for a config extension); settled in v1.2: with `[render.quota] booster = true`, state=Ready, and balanceCents available, a cyan `boost <balance in yuan>` (pure ASCII with two decimals, avoiding the compatibility problems of ¥ on non-UTF-8 terminals) is appended inside the quota group.

tasks badge segment format (v1.5; data source and defenses in §7.7): the segment text replicates the host's native footer badges — bash side `[N task running]` (N=1) / `[N tasks running]`, agent side `[M agent running]` / `[M agents running]` (singular/plural per count, footer.ts:483-488, 489-494). When both counts are non-zero they are joined by a **single space** to form one segment of this tool (the host's native layout joins slot fragments with double spaces, footer.ts:327-330 — the single space is this tool's in-segment design decision, consistent with the layer distinction from the inter-segment `|` separator); when both counts are zero the entire segment is omitted. At degradation level 3 (quota group only) it is dropped together with the other non-quota segments (§7.5 drop-only, no-reorder semantics).

### 7.2 Color codes (basic SGR; the host's chalk wrapping does not interfere)

| Element | Color | ANSI |
|---|---|---|
| permissionMode: `yolo` | red | `31` |
| permissionMode: `auto` | yellow | `33` |
| permissionMode: `manual` | green | `32` |
| permissionMode: unknown value | white | `37` |
| model | cyan | `36` |
| thinking effort | cyan | `36` |
| thinking off | bright black (gray) | `90` |
| tasks badge (task/agent running) | cyan | `36` |
| gitBranch | magenta | `35` |
| quota segment percent < 60 | green | `32` |
| quota segment 60 ≤ percent < 85 | yellow | `33` |
| quota segment percent ≥ 85 | red | `31` |
| inter-segment separator `\|` and intra-group separator `·` | gray | `90` |

Basis: quota-status.py:21 (thresholds 85/60), :195 (permissionMode color table), :202 (model cyan), :111/:119 (thinking colors), :222 (git magenta). Threshold boundaries: yellow from 60, red from 85 (`>= 85`, `>= 60`, quota-status.py:21). Each segment ends with a `\033[0m` reset. permissionMode displays the raw value string. tasks badge cyan `36` (v1.5): the host's native badges use the theme `colors.primary` (footer.ts:486, 492); this tool has no access to the theme, so the closest informational color cyan is used (same family as model/thinking).

**Monochrome switch (v1.4)**: with `[render] colors = false` the output is **plain text containing no SGR sequences at all** (separators and per-segment resets are omitted too) — the host wraps the whole command output in the theme foreground color (`chalk.hex(colors.text)`, footer.ts:318), so the line automatically takes the theme text color, matching the context readout on line 2 and following `/theme` switches. Under this semantics the color table in this section and all SGR codes in §7.4 are disabled; separators are the plain characters ` | ` / ` · ` and visible width equals the character count. Note there is no clean "partially colored" intermediate state: an in-segment `\x1b[0m` is a full attribute reset, and text after it falls back to the terminal default color rather than the theme color.

### 7.3 Quota segments and reset-time format

- In-segment format: `{label} {percent:.0}%{reset}`, with label `5h` / `week` / `month`; percent is rounded per `%.0f` semantics (**round-half-to-even**: 60.5→60, 61.5→62 — not round-half-up; boundaries pinned by unit tests).
- Reset suffix: appended within the same color span as ` (rst HH:MM)`; when the reset time is on another day, `(rst MM/DD HH:MM)` (quota-status.py:27-31; `reset_time` comes from the cached `resetAt`, local timezone).
- The three segments inside the quota group are joined by ` · ` (gray dot): `" \033[90m·\033[0m "` (quota-status.py:98).

### 7.4 Line assembly

- Separators between segments and inside the group: between segments `" \033[90m|\033[0m "` (space + gray pipe + space, quota-status.py:224), inside the group `" \033[90m·\033[0m "`; with `colors = false` (§7.2 monochrome switch) these become the plain characters `" | "` and `" · "` respectively.
- The whole line = surviving segments joined in configured order; output one line + `\n`, exit 0.
- Visible-width calculation excludes ANSI escape sequences; approximating width by character count is acceptable (fields are mostly ASCII), and the host's `truncateToWidth` is the final fallback (footer.ts:397).

### 7.5 Width awareness and degradation ladder (built in, effective before host-side truncation)

- Width source: the payload has no terminal-width field (§3.5); although the render process's stdout is a pipe, it is still attached to the host console — query via `CreateFileW("CONOUT$")` + `GetConsoleScreenBufferInfo().dwSize.X`; if unavailable (headless), fall back to **120**.
- Degradation order (drop-only, no reordering; try levels in order until it fits):
  1. drop all reset-time suffixes;
  2. drop the gitBranch segment;
  3. keep the quota group only;
  4. if still too wide, output as-is and let the host truncate (footer.ts:397).

### 7.6 stdout UTF-8 and output constraints

- Output bytes are forced UTF-8; strings from external sources (paths, model names, etc.) are lossily replaced first (`U+FFFD`), corresponding to the Python prototype's `sys.stdout.reconfigure(encoding='utf-8', errors='replace')` (quota-status.py:171-175) — preventing garbled GBK pipes on Chinese Windows and lone-surrogate crashes. stdin is likewise read as lossy UTF-8 (§4.1).
- Exactly one line is output (the host takes the first line, §3.2); the total length (including ANSI) must be < 64KB (§3.3; in practice far below).

### 7.7 tasks/agents badge data source and defenses (added in v1.5)

> This section has two blocks: **host facts** (verified against the official source code and a live machine, each tagged with its `repos/kimi-code/...` source) and **this tool's design decisions** (settled in v1.5; not host behavior). The two blocks are kept clearly distinct — do not conflate them.

**Host facts (verified)**:

- Persistence layout: every background task is written to `<kimi_home>/sessions/<workspaceId>/<sessionId>/agents/<agentId>/tasks/<taskId>.json` (agentId such as `main`, `agent-0`). Directory chain sources: sessionDir = `<homeDir>/<sessions scope>/<workspaceId>/<sessionId>` (`repos/kimi-code/packages/agent-core-v2/src/workspace/sessionLifecycle/internal/addressing.ts:11-13`; workspace persistence scope at `repos/kimi-code/packages/agent-core-v2/src/workspace/workspaceInstance/workspaceInstanceManagerService.ts:203`), agent scope = `<sessionScope>/agents/<agentId>` (addressing.ts:15-17), task file = `<scope>/tasks/<taskId>.json` (constants `TASKS_SCOPE='tasks'`/`JSON_SUFFIX='.json'` at `repos/kimi-code/packages/agent-core-v2/src/agent/task/persist.ts:12, 14`; written by `writeTask` at persist.ts:87-89 and `agent/task/taskService.ts:245-250`). The local machine's `~/.kimi-code/sessions/wd_*/session_*/agents/*/tasks/*.json` matches this structure (verified live 2026-10-02).
- The payload's `sessionId` is the directory name verbatim: the host's `createSessionId()` returns `session_<randomUUID()>` (`repos/kimi-code/packages/agent-core-v2/src/workspace/sessionLifecycle/sessionLifecycleService.ts:903-905`), matching the session directory names under sessions.
- Key fields of the task json (type definitions at `repos/kimi-code/packages/agent-core-v2/src/agent/task/types.ts:1-34, 64-92`; matched one-to-one against live samples on this machine): `status` ∈ running \| completed \| failed \| timed_out \| killed \| lost — only running is non-terminal; the other five are terminal (`TERMINAL_STATUSES`, types.ts:8-14). `kind` has three persistable values: `"process"` = a bash background task (with command/pid/exitCode), `"agent"` = a background subagent (types.ts:81-87, with agentId/subagentType, **no pid field** — a background agent is an in-process async task of the CLI, not a separate OS process), `"question"` = a background ask-user-question task (types.ts:88-92, with questionCount/toolCallId, also **no pid field**; creation chain at `agent/tools/ask-user-question/askUserQuestionTool.ts:223` and `QuestionBackgroundTask` at question-background-task.ts:24-25; flows through the same taskService pipeline and `writeTask` persistence as bash/agent). Others include `detached`, `startedAt`/`endedAt` (Unix ms), etc. No question task json was found in local sessions (verified by grep 2026-10-02; the source evidence chain is complete).
- The host's in-memory counting semantics (the data behind the footer badges): non-terminal tasks are split by kind — `kind === 'agent'` → agentTasks, otherwise (including `"process"`, `"question"`) → bashTasks (`repos/kimi-code/apps/kimi-code/src/tui/controllers/session-event-handler.ts:1287-1308`; same semantics at `apps/kimi-code/src/tui/utils/message-replay.ts:97-113`); the two counts hide independently — zero means not shown (`apps/kimi-code/src/tui/components/chrome/footer.ts:290-300`). So a running question task is counted on the host's bash side and displayed as `[N tasks running]`.
- Stale scenario: a CLI crash leaves status=running task files on disk (the host's in-memory count vanishes with the process, while the disk still says running); when the host **reloads the session** it marks non-terminal leftover tasks as `lost` and writes that back (`repos/kimi-code/packages/agent-core-v2/src/agent/task/taskService.ts:931-945` `markLoadedTasksLost`) — the leftover window is between the crash and the reload.
- Background note (not implemented in this tool): workspaceId looks like `wd_<slug>_<hash12>`, generated from the workspace root via `encodeWorkDirKey` — slugified directory name + the first 12 hex chars of the normalized path's sha256 (`repos/kimi-code/packages/agent-core-v2/src/_base/utils/workdir-slug.ts:3-5, 17-23`).

**This tool's design decisions (settled in v1.5)**:

- **A Location: scan-and-probe, not sha256 inversion**. read_dir `<kimi_home>/sessions/` → probe each workspace directory for a `<ws>/<sessionId>` subdirectory (sessionIds are globally unique; the first hit locates the session; a missing/empty payload `sessionId` → segment omitted). Rationale: avoids adding a sha256 dependency (§11's whitelist has no sha2, and the path-normalization details of workdir-slug.ts would also need replicating), tolerates cwd ≠ workspace.root (payload `cwd` unused, §3.5), and costs only O(number of workspaces) stats.
- **B Path defense and scan scope**: the sessionId comes from the host payload and must be validated before path joining — containing `/`, `\`, or `..` makes it invalid outright (segment omitted), preventing path traversal. After a hit, iterate `<sessionDir>/agents/*/tasks/*.json` to read task files (file names are not validated; the json content is authoritative). **Only `agents/*/tasks/` is scanned; data written by older host CLIs directly under `<sessionDir>/tasks/` is not supported** (arbitrated and settled 2026-10-02: the host main agent's persistence fallback exists only to read old data — persist.ts:99-103, 151-154; the current CLI writes only to the new path persist.ts:87-89; verified no data at the old location on this machine). On the implementation side, ever since the P7 first-review hardening, a stricter **character whitelist `[A-Za-z0-9_-]`** is used (the host truth `session_<uuid>` always falls inside it; the whitelist covers the three rejection rules above in one step and closes residual traversal surfaces such as drive-relative paths, bare `.`, UNC, and non-ASCII; documented in v1.5.1, no behavior change).
- **C Stale defense: split by kind (arbitrated 2026-10-02)**. The `kind == "agent"` task json has no pid field (see host facts): **status=running is counted into the agent count immediately, without a pid check** — accepting a stale over-count in the agent count after a CLI crash; the host reload marks leftover tasks `lost` and self-heals (see host facts). Every other kind (`"process"`, `"question"`, unknown, or missing) goes through **pid liveness checking**: a status=running task must prove liveness via the json's pid — `windows-sys`'s `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `GetExitCodeProcess`; a NULL handle or exit code ≠ STILL_ACTIVE(259) → treated as finished and not counted; **a missing or non-positive-integer pid (string form, negative, decimal, beyond u32, and other hostile forms) is treated as missing and conservatively not counted**. Corollary: a running `kind=question` task is never counted because it has no pid — a known deviation from the host, which counts it on the bash side (second arbitration 2026-10-02: keep the conservative exclusion; better to under-report than over-report).
- **D Scan caps** (so pathological directories cannot blow the 300ms render budget): ≤64 workspace probes, ≤32 task-json reads; beyond the cap, stop and **count what has been read** (truncation is not a failure and does not trigger segment omission).
- **E Fail and omit**: sessions directory missing, json parse failure, any IO error → the entire segment is omitted, never blocking rendering, never panicking (in the spirit of §9). Semantics pinned in v1.5.1 (second-review Major, resolved by arbitration to fix the code): a task-file read/parse failure **short-circuits to zero counts** — the entire segment is omitted, and accumulated partial counts must not be kept; a missing directory is an empty state rather than a failure — a missing sessions directory yields zero counts/omission, and a missing `agents/<id>/tasks/` subdirectory is treated as "this agent has no tasks" and skipped (other IO errors on that path still short-circuit to omission); entry-level IO errors during directory enumeration iteration are skipped (`flatten` semantics) without triggering omission (third-review Nit 3, exemption recorded).
- Counting semantics: only `status == "running"` is counted (the five terminal statuses are skipped; unknown statuses likewise); `kind == "agent"` (strict string equality, matching the host's `===`) → agent count (counted while running, Decision C); every other value → bash side (with pid check). The known deviations from the host's in-memory semantics are exactly two: the bash-side pid check is stricter than the host (whose in-memory count is inherently authoritative, while this tool only has on-disk data), and question tasks are not counted (arbitrated and settled).

---

## 8. Configuration File `~/.kimi-code/quota-bar.toml`

When the file is absent, built-in defaults apply (the example below is the complete default); keys that fail to parse are ignored and fall back to defaults, rendering never fails because of configuration, and unknown fields are ignored. Paths follow `KIMI_CODE_HOME`.

```toml
# quota-status configuration (optional; when missing, all values below apply as defaults)

[render]
# In-line field order (can be trimmed or reordered; fields not listed are not shown)
order = ["permission_mode", "model", "thinking", "tasks", "quota", "git_branch"]
# Monochrome switch: with false the output is plain text (no SGR), and the host wraps
# the whole line in the theme text color (same color as the line-2 context readout,
# following /theme); true by default keeps the full color scheme
colors = true

[render.quota]
# Sub-segment switches within the quota group
five_hour  = true
week       = true
month      = true
reset_time = true   # false equals degradation level 1 (reset suffix permanently dropped)
booster    = false  # do not show the booster wallet by default (data is still parsed into the cache)

[thresholds]
# Percentage color thresholds: percent < green_below → green; < yellow_below → yellow; otherwise red
green_below  = 60
yellow_below = 85

[cache]
ttl_seconds   = 60   # cache validity window (default 60s)
retry_seconds = 30   # mtime-rewind window after a refresh is triggered = fast-retry interval

[network]
base_url = "https://api.kimi.com/coding/v1"  # international site: https://api.kimi.ai/coding/v1
http_timeout_seconds = 8                      # default 8s
```

Precedence: env `KIMI_CODE_BASE_URL` > `[network] base_url` > built-in default. Unknown field names inside `order` are ignored (no failure).

The tasks badge (v1.5) **adds no configuration key**: the switch is the order itself — removing `"tasks"` from order disables the whole segment (and skips the sessions directory scan, saving IO, same convention as the thinking segment's switch-saves-IO), adding it back enables it; when both counts are zero the segment is omitted regardless of configuration. Note: configurations that explicitly wrote an `order` before v1.5 do not contain `"tasks"`, so by order semantics the badge does not show — expected behavior, not a defect; users who want it should add `"tasks"` to their own order.

---

## 9. Error and Degradation Matrix

| Scenario | Fetch-side behavior | Render-side behavior |
|---|---|---|
| No token (both credential steps empty) | `--refresh` exits silently without a request; `--test-fetch` outputs `error:"no-token"` | quota group omitted entirely, other fields normal, no blocking |
| 404 (pure API key / non-managed OAuth account) | classified as `HttpRequestException` (quota.rs:100-102), no cache write | same: quota group omitted, other fields normal |
| Network failure / timeout (8s) | `HttpRequestException` / `TaskCanceledException`; no cache write | old cache (LKG) keeps rendering, **never cleared**; fast-retry after 30s |
| Response JSON parse failure | `JsonException`; no cache write | same |
| Cache missing / first run | render mode creates an empty cache to anchor mtime → rewind → spawn refresh | quota group omitted at first; appears within ≤1s after a successful write (host throttle) |
| Corrupt cache (invalid JSON) | — | rendered as missing (quota group omitted); garbage content not cleared; spawning still driven by the pure-mtime check (§4.4): age ≥ TTL → rewind + spawn, no repeated spawning within TTL |
| Terminal too narrow | — | built-in degradation ladder (§7.5): drop reset → drop gitBranch → quota group only → host truncation as the last resort |
| stdin JSON invalid / empty | — | render as an empty payload (missing fields omitted), exit 0 |
| Session task directory scan failure (sessions directory missing / sessionId invalid or containing traversal strings / task-json parse failure / IO error) | — | tasks badge segment omitted entirely, other fields render normally, no blocking, no panic (§7.7 Decisions B/E; v1.5). Note that **exceeding the scan caps is not an omission trigger**: stop at the cap and count what was read (§7.7 Decision D — truncation, not failure) |
| Render process over 300ms / crash | — | host kills the process tree (status-line-command.ts:75-78), keeps the last-good line (:174-179) or falls back to the built-in layout |
| quota-bar.toml missing / invalid | — | all built-in defaults, rendering proceeds |
| tui.toml loses `[status_line]` | — | the quota line disappears entirely (the host stops calling), see risk ① |
| `KIMI_CODE_HOME` override | credential/cache/config paths all follow it (§5.2/§5.3/§8) | same |

---

## 10. Test Contract

### 10.1 Unit tests

- Every parsing defense rule in §6 has at least one case, modeled on quota.rs:243-374 and `repos/kimi-planbar-tui/go/internal/core/quota_test.go` (string/number mixes, division by zero, `isEnabled=false`, unit rounding, the `i64::MIN` hostile value, the reset ladder).
- Credential chain: the two pre-existing unit-test functions from credentials.rs:115-147 ported verbatim (covering compact/whitespace section names, section settlement, and empty api_key rejection).
- Added by this tool: thinking ladder (enabled=false / effort / models fallback), color threshold boundaries (59.x/60/84.x/85), reset cross-day format, width degradation ladder, UTF-8 lossy, mtime rewind computation (`now-TTL+30`), month segment with/without limit, and both states of the colors monochrome switch (with false the output contains no SGR and visible content matches the colored version).
- tasks badge (v1.5, §7.7): workspace probing (sessionId hit / miss / path-traversal rejection — sessionIds containing `/`, `\`, `..` are invalid), count routing (kind=process counted into bash via pid check, kind=agent counted into agent while running, kind=question not counted without pid, the five terminal statuses and unknown statuses not counted, missing kind routed to the bash side), pid-liveness defense with three bash cases (live process counted / exited process not counted / missing or non-positive-integer pid not counted) + one agent case (counted while running, no pid check), scan caps (>64 workspaces, >32 task jsons — truncated and counted from what was read, without triggering segment omission), render position (the tasks segment ahead of the quota group) and the colors=false monochrome combination (no SGR, two badges joined by a single space), and failure semantics (added in v1.5.1: a task-json parse/read failure → entire segment omitted, partial counts must not survive; a missing `agents/<id>/tasks/` directory is skipped as an empty state, without triggering omission).

### 10.2 golden parity (30-case alignment)

- Mechanism replicated from `quota_test.go:206-293`: input payload inlined (copied from quota_test.go:240-266) → inject the parsing function → serialize with a fixed clock → **byte-for-byte alignment** against this repository's `testdata/golden/quota-*.txt` (the 30 pre-existing cases were copied byte-for-byte from `repos/kimi-planbar-tui/go/testdata/golden/` on 2026-10-01, making CI self-sufficient; CRLF normalization before comparison, same as goldens_test.go:23-25).
- The fixed clock follows the two constants at quota_test.go:14-15: `atZero` (1893456000000ms flat) and `atFracs` (+123456789ns).
- The serialization format is the same function used for `--test-fetch` output (serde pretty, 2-space indent, camelCase), so parity also covers the binary's output format.
- The `month` field serializes with `skip_serializing_if = "Option::is_none"`: the 30 pre-existing goldens (no month key) stay byte-identical (internalized into `testdata/golden/` on 2026-10-01); the 3 new month cases (`quota-month-full`, `quota-month-no-limit`, `quota-month-zero-limit`) are native to this project's `testdata/golden/` and are never written back to the reference repository.
- Golden assertions include a +08:00 timezone offset (the fixed clock serializes in the local timezone, continuing the reference repository's Go test time.Local semantics): on non-+08:00 machines the parity tests fail fast with a clear reason rather than skipping silently.
- The 4 `quota-error-*` cases use no network; error results are constructed directly for comparison (same as quota_test.go:272-275).

### 10.3 `--test-fetch` self-check

- On a live machine (managed OAuth signed in), run `quota-status.exe --test-fetch`: assert stdout is valid JSON, `error == null`, `fiveHour`/`week` present, and `fetchedAt` close to now.
- Credential-less environment: assert the output is `error == "no-token"`.
- Assert no cache side effects (cache file mtime/content unchanged before and after).

### 10.4 Live acceptance criteria

1. The artifact is a single static exe of size ≤5MB (§11; measured ≈1.7MB).
2. Render process end-to-end <50ms (6× margin under the host's 300ms cap); hot path <10ms.
3. After configuring tui.toml and running `/reload-tui`, the quota line shows on footer line 1; in an **active session** (with input / streaming / an active goal) the quota data auto-updates **within 1 minute** (TTL 60s + event-driven re-renders, §3.3); a purely idle session never invokes the command and no refresh triggers — data self-heals within seconds of activity resuming, riding the next render.
4. After a network cut / API failure, old quota data is preserved on display, never cleared; self-heals within ≤90s of network recovery (30s fast-retry + 60s TTL).
5. Non-managed OAuth account: permissionMode/model/gitBranch render normally, no quota group, no error line.
6. Run the investigation drill after `/theme` or a CLI upgrade: in current main, preference saves rewrite tui.toml wholesale, but the status_line command/items values round-trip faithfully (config.ts:298-312), losing only in-section comments and unknown keys — just verify the command value survives; on old versions (reported in the 0.31.1 era) with full-section loss, restoring `[status_line]` and running `/reload-tui` recovers (risk ①).

---

## 11. Engineering Constraints and Build

- Single cargo crate; Windows x64 target only (`x86_64-pc-windows-msvc` + `rustflags = ["-C", "target-feature=+crt-static"]` in `.cargo/config.toml` for static CRT linking); the artifact has no runtime dependencies.
- release profile: `opt-level = "s"`, `lto = true`, `codegen-units = 1`, `strip = true`, `panic = "abort"`.
- Dependency stance (size control): `serde`/`serde_json`, `chrono`, `toml` (for quota-bar.toml and the thinking/models sections of config.toml); HTTP via a lightweight blocking client (first choice `ureq` + rustls + bundled root certificates; if switching to `reqwest`, it must be blocking + rustls); `filetime` (mtime rewind); `windows-sys` (console width, creation flags). **No tokio/async** — fetching is a one-shot blocking call and rendering exits right after spawning. **The v1.5 dependency whitelist is unchanged**: the tasks badge's pid-liveness check reuses the existing `windows-sys` (`Win32_System_Threading` feature: `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `GetExitCodeProcess`; the feature is already enabled in the project's dependency declaration), no new crates. **v1.6 admits the build-dependency `winresource`** (sole transitive dependency `version_check`; build-only, never ships in the artifact): via `build.rs` it embeds a Windows VERSIONINFO resource (FileDescription/ProductName/OriginalFilename/LegalCopyright plus the version, sourced from `CARGO_PKG_VERSION`, see section 4.5); the ~1 KB resource lives in the `.rsrc` section, untouched by `strip = true` (symbols only); on non-Windows targets build.rs skips outright.
- Size budget: static exe **≤5MB** (v1.2: the original "3–5MB" was the estimate at writing time; implementation measured 1,788,928 bytes ≈1.7MB, and the budget is enforced as an upper bound); if over budget, swap the HTTP/TLS backend first.
- Render path forbids network and heavy IO (<10ms budget); the fetch path's HTTP timeout is 8s.
- stdout forced UTF-8 (errors=replace semantics, §7.6); detached child gets `CREATE_NO_WINDOW | DETACHED_PROCESS` + stdio DEVNULL (§4.4).
- Newly written code estimated at 300–500 lines (render + field-config + cache modules).
- CI/Release engineering (recorded in v1.3.2): `.github/workflows/ci.yml` (on push/PR: `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` + unit + golden + release build + 5MB size gate); `.github/workflows/release.yml` (tag `v*`: tag↔Cargo.toml version consistency check + full tests + build + an automatic GitHub Release attaching exe/sha256). All jobs across both workflows declare explicit least-privilege `permissions` (per-job `contents: read` in ci.yml, workflow-level `contents: write` in release.yml; recorded in v1.5.2 — fix for the CodeQL first-scan medium alert `actions/missing-workflow-permissions`). The change history is maintained by hand in `CHANGELOG.md` (Keep a Changelog format).

---

## 12. Risks and Mitigations

| # | Risk | Mitigation |
|---|---|---|
| ① | Preference saves (e.g. `/theme`) rewrite tui.toml wholesale: in current main the status_line command/items values round-trip faithfully (config.ts:298-312); what is lost are in-section comments and unknown keys; the "whole section silently lost" reports from the 0.31.1 era are old-version behavior, not reproduced in current source | Document "check this first when the quota line disappears" (§10.4 acceptance 6): first confirm the command value survived (round-trip should preserve it); on old versions with full-section loss, restore `[status_line] command` and run `/reload-tui` |
| ② | The usages API structure may drift across versions | Defensive parsing (§6) + golden regression (§10.2): drift usually manifests as missing fields / type changes, falling into existing fallbacks rather than panicking; new drift shapes get new golden cases |
| ③ | Non-managed OAuth accounts have no endpoint (404) | 404/no-token omits the quota segment entirely without blocking other fields (§9); `--test-fetch` can self-report the `error` type |
| ④ | The task json is the host's internal persistence format with no external contract, and may drift across CLI upgrades (renamed fields, moved directories, new kinds) (v1.5) | Defensive reading (§7.7): status/kind parsed leniently (unknown statuses treated as terminal and skipped; unknown kinds routed to the bash side); drift manifests as segment omission or count deviation rather than panic/blocking; once confirmed, adapt to the new format and add unit tests. Settled boundary for directory-shape drift: legacy data written by older CLIs directly under `<sessionDir>/tasks/` is **unsupported by default** (§7.7 Decision B, arbitrated 2026-10-02); supporting it would require revising §7.7 |
| ⑤ | A CLI crash leaves orphan status=running task files (the host marks them lost on reload, taskService.ts:931-945; within the pre-reload window this tool must guard itself); pid checking has a theoretical PID-reuse residue (an exited pid gets reused → miscounted as alive) (v1.5) | Split per Decision C (§7.7): bash side (process/question/unknown kinds) pid-liveness check — NULL handle or exit code ≠ STILL_ACTIVE(259) not counted, missing/non-positive-integer pid treated as missing; agent side counted while running, **explicitly accepting** a stale over-count after a CLI crash (window = crash until host reload, then self-heals). The PID-reuse window is tiny and the worst case is a briefly over-counted badge — acceptable |

---
