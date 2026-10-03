# Changelog

All notable changes to this project are documented in this file.

> English rendition of the Chinese [CHANGELOG.md](CHANGELOG.md); the two are synced per version. In case of any divergence, the Chinese version prevails.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versioning follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Dependencies

- windows-sys 0.60 → 0.61, toml 0.9 → 1.1, and actions/checkout v4 → v7 (first Dependabot batch; semver-major bumps pass the full gate suite with zero code changes, Cargo.lock nets −71 lines)

## [1.1.0] - 2026-10-02

### Added

- tasks/agents badges (SPEC v1.5 §7.7): footer line 1 now shows background-task counts for the current session ahead of the quota group — bash background tasks `[N task(s) running]` (cyan, pid-liveness checked via `OpenProcess` + `GetExitCodeProcess`) and background subagents `[M agent(s) running]` (counted while running); the entire segment is omitted when both counts are zero. Data source is a scan of `<kimi_home>/sessions/` (≤64 workspace probes, ≤32 task-json reads — beyond the cap, counting is truncated rather than failed) with sessionId path-traversal defense and fail-and-omit semantics, never blocking rendering. `"tasks"` joins the default order; removing it from `order` disables the segment and skips the scan; no new configuration keys.
- Monochrome render switch: with `quota-bar.toml [render] colors = false` the output is plain text (no SGR sequences at all) and the host wraps the whole line in the theme text color — matching the context readout on footer line 2 and following `/theme` switches (SPEC v1.4 §7.2/§7.4/§8).

### Fixed

- tasks-badge failure semantics aligned with the contract (SPEC §7.7 Decision E / §9): a task-json read/parse failure now short-circuits to zero counts (entire segment omitted) instead of "skip the file and keep partial counts"; the corresponding unit-test assertion was inverted and an empty-state regression added (second independent code review Major, resolved by user arbitration to fix the code toward the contract). A missing `agents/<id>/tasks/` directory is treated as an empty state (skip, not failure); any other IO error still omits the entire segment.
- Added deterministic regression tests for M-1 (third-review Minor-1): good tasks counted ahead of a bad file in name order, plus a cross-agent omission case — deterministically pinning that "accumulated counts are discarded on short-circuit".

### Documentation

- SPEC v1.5.1: §7.7 Decision B now documents the sessionId character-whitelist semantics (in place since the P7 review hardening; text sync, no behavior change), Decision E adds the NotFound empty-state exemption, and §10.1 adds two unit-test items for failure semantics and the empty state. PLAN v1.5.1: the P7 "files involved" list now includes src/lib.rs and src/main.rs. HANDOFF: §2 unit-test split erratum (13/5) and a §7 addendum (second-review course and acceptance criterion 5 passing). Third-review Nit wrap-up: SPEC Decision E records the `flatten` entry-level error exemption, PLAN's contract reference bumped to v1.5.1, README gate row annotated with the first-review point in time.

## [1.0.0] - 2026-10-01

First stable release.

### Added

- Kimi Code CLI statusline command: footer line 1 shows Kimi For Coding plan usage (5h / week / month percentages + reset time)
- Three-mode process model: render mode (local-cache only, millisecond exit, no network) / `--refresh` fetch mode (detached child backfills in the background) / `--test-fetch` self-check mode (headless, never writes the cache)
- `~/.kimi-code/quota-bar.toml`: field switches and in-line order, quota sub-segment switches, color thresholds, cache TTL, base_url override
- Rendered fields: permissionMode (yolo/auto/manual coloring), model, thinking level, quota group (green/yellow/red thresholds), gitBranch
- Caching: TTL 60s + mtime rewind against refresh storms + 30s fast-retry + LKG preservation; PID-suffixed tmp + rename atomic write
- Defensive parsing (SPEC §6): numbers modeled as strings with real-number fallback, limit clamped against division by zero, NaN/hostile integers normalized to zero, resetTime lenient ladder, boosterWallet `isEnabled` defense
- Width-aware degradation ladder: drop reset suffixes → drop gitBranch → quota group only → host-side truncation as the final fallback
- Golden parity tests: 33 cases aligned byte-for-byte (30 internalized from kimi-planbar-tui + 3 native month cases)
- GitHub Actions CI (fmt/clippy gates + unit + golden + release build + 5MB size gate) and a tag `v*` Release workflow (attaches exe + sha256 automatically)
- MIT LICENSE + NOTICE (kimi-planbar / kimi-planbar-tui derivation attribution)

### Fixed

- The 2 Major + 3 Minor findings of the third independent code review: console `CONOUT$` handle access=0 made width queries always fail; the cache freshness check AND-ed in "parseable", defeating the storm-prevention mtime rewind (details in docs/REVIEW3.md)

[1.0.0]: https://github.com/shawn-0106t/kimi-code-quota-status/releases/tag/v1.0.0
