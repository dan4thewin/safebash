title: DD#23 — no validator rule for PATH= assignment
status: resolved
source: claude-opus-4.6
severity: gap

DD#23 says scripts cannot modify PATH at runtime. But there's no
validator rule (no SB code) that catches `PATH=...` assignment.
May be implicitly caught because PATH is a known env var, but the
enforcement mechanism isn't explicit.
