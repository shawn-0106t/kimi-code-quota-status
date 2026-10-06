## What does this PR do?

<!-- A short description of the change: what and why. Link related issues
     with "Fixes #123" or "Refs #123". -->

## How was it tested?

<!-- Run on Windows x64 and tick only what you actually ran. -->

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets --locked -- -D warnings`
- [ ] `cargo test --lib` (unit tests)
- [ ] `cargo test --test golden` (local timezone must be UTC+08:00)
- [ ] `cargo build --release` + manual statusline smoke test (render changes)

## Docs updated?

<!-- Behavior changes must sync documentation in the same PR. Tick "N/A"
     where a section genuinely does not apply. -->

- [ ] `docs/SPEC.md` (authoritative contract) updated - or N/A: no behavior change
- [ ] `docs/SPEC.en.md` (English translation) updated - or N/A
- [ ] `docs/PLAN.md` updated - or N/A
- [ ] `CHANGELOG.md` + `CHANGELOG.zh-CN.md` updated - or N/A

## Golden parity impact

<!-- Required whenever this PR touches rendering or parsing. Fixtures in
     testdata/golden/ are a byte-for-byte contract ported from upstream:
     never hand-edit expected output to make tests pass. Contract changes
     must be discussed in an issue first. -->

- [ ] No rendering/parsing change; output bytes unaffected for all inputs
- [ ] Output bytes may change; contract discussed in issue #____ and fixtures updated in this PR

<!-- If output bytes may change, explain which inputs change and why: -->

## Checklist

- [ ] Conventional commit subject (`feat:` / `fix:` / `docs:` / `chore(deps):` etc.)
- [ ] No new dependencies (or justified against the `docs/SPEC.md` section 11 whitelist)
- [ ] Windows x64 only; no secrets or machine-specific paths in the diff
