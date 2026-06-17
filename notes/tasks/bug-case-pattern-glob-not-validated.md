---
title: BUG — case patterns ? and [...] silently never match (validator + runtime gap)
status: pending
priority: P0
kind: bug
depends: []
gates:
  - case patterns with ? or [...] either (a) get SB5006 at compile time, or (b) match correctly at runtime
  - control_flow.sh cf.c7 and cf.c8 pass under all 3 modes
  - SB5006 documentation updated to clarify which contexts it covers
---

# BUG — case patterns ? and [...] silently produce no-match

## Discovery

Found while writing `spec/control_flow.sh` (cf.c7, cf.c8). Adjacent
to the while-loop bug; same investigation surfaced both.

## Repro

```bash
$ bash -c 'x=ab; case $x in ?b) echo two-char-b ;; *) echo other ;; esac'
two-char-b

$ bash -c 'x=5; case $x in [0-9]) echo digit ;; *) echo other ;; esac'
digit

$ target/release/sash -c 'x=ab; case $x in ?b) echo two-char-b ;; *) echo other ;; esac' -o /tmp/t && /tmp/t
other

$ target/release/sash -c 'x=5; case $x in [0-9]) echo digit ;; *) echo other ;; esac' -o /tmp/t && /tmp/t
other
```

Sash silently falls through to the default arm — no validator error
and no runtime match. The compiled program produces semantically
wrong output.

## Root cause (two-part)

1. **Validator gap.** `validate.rs::check_unquoted_glob` (line 708)
   correctly flags `?` and `[...]` with SB5006, but it is only
   called from `validate_for` (line 176). Case-arm patterns never
   get checked.

2. **Runtime gap.** `sash-rt/src/lib.rs::fnmatch_impl` (line 2303)
   only implements `*`. The header comment claims "`?`, `[...]`,
   `\\` are rejected at compile time (SB5006)" — but they're not
   rejected for case patterns, so they reach runtime and silently
   never match.

## Decision required (file separately or here)

Pick one:

- **(a) Extend SB5006 to case patterns.** Reject `?` and `[...]`
  in case arms at compile time. Matches the existing claim in
  the fnmatch comment. Loses bash compatibility for these
  patterns but is consistent with sash's "subset of bash"
  philosophy.

- **(b) Implement `?` and `[...]` in fnmatch_impl.** Match bash
  exactly. Reverses the design intent stated in the fnmatch
  header comment. Larger code change in sash-rt.

The existing design intent (per the fnmatch comment) is (a). The
implementation is half-done: validator missed case patterns. The
minimum fix is to call `check_unquoted_glob` on each case pattern
in `validate.rs`. Confirm with DG before fixing.

## Expected behavior (after fix-a)

```
$ sash -c 'x=ab; case $x in ?b) echo ok ;; esac'
-c:1:18: error [SB5006] ? glob is not supported in sash — only * is allowed: ?b
```

## Test coverage to add once fixed

`spec/control_flow.sh` cf.c7 / cf.c8 already document the
behavior. Should flip from failing → passing with either fix.
Add a third test: bare `?` and bare `[abc]` in case patterns
should error with SB5006.

## References

- AGENTS.md "Bug policy" — zero tolerance
- DD#? — SB5006 design (subset-of-bash glob)
- `sash/src/validate.rs:708` — check_unquoted_glob (incomplete)
- `sash-rt/src/lib.rs:2303` — fnmatch_impl (only `*`)
- spec.db `native:cf.c7`, `native:cf.c8`
