---
name: Bug report
about: Report a problem with rendering, quota fetching, or caching
labels:
  - bug
---

<!-- Security vulnerabilities must NOT be reported here. Follow SECURITY.md
     and use GitHub's private vulnerability reporting instead. -->
<!-- Tip: you may write your report in Chinese (中文) if that is easier. -->

## Description

<!-- A clear and concise description of the problem. -->

## Steps to reproduce

1.
2.
3.

## Output of `quota-status.exe --test-fetch`

<!-- Run the binary with --test-fetch and paste the JSON output below.
     REDACT BEFORE PASTING: no token, API key, or other credentials. The
     output should not contain them, but verify - also redact account
     identifiers or anything else you consider sensitive. -->

```json
(paste output here)
```

## Environment

- Kimi Code CLI version: (version string, or how and when you installed/updated it)
- Windows version: (e.g. Windows 11 24H2, build 26100; run `winver`)
- quota-status.exe version: (release tag or commit, e.g. v1.1.0; if unsure, paste the file's sha256)

## Expected vs actual

**Expected:**

<!-- What you expected the statusline to show or do. -->

**Actual:**

<!-- What actually happened. A screenshot of the footer line helps a lot for rendering issues. -->
