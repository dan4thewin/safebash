---
title: Tilde expansion produces SB5018-rejected $HOME
status: ready
priority: P3
kind: bug
depends: []
gates:
  - `~` and `~/path` either compile cleanly or produce a focused diagnostic
  - new e2e_tests case for `echo ~` and `echo ~/foo`
  - DD or DD amendment if behavior is intentional
---

# Tilde expansion produces SB5018-rejected $HOME

Found 2026-06-07 while excising pre-DD#53 cruft from `handle_cd`
(journal 109).

## Symptom

```bash
$ echo "echo ~" > /tmp/t.sh
$ sash /tmp/t.sh
/tmp/t.sh:2:6: error [SB5018] $HOME is not available directly — use $(getenv HOME)
```

The user wrote `~`, never wrote `$HOME` — the error message is
misleading.

## Cause

`sash/src/parse/expr.rs:138 parse_word_arm` lowers `~` to
`ExprKind::SimpleExpansion("HOME")` and `~/path` to
`ExprKind::Concat([SimpleExpansion("HOME"), Literal("/path")])`.

`sash/src/validate.rs:388 ENV_VARS` lists `HOME` as a rejected bare
env-var per SB5018 (DD#53).

The parser produces an expression the validator will always reject,
with an error message that refers to `$HOME` instead of `~`.

## DD#53 considerations

Per DD#53, `$HOME` is a plain sash variable that is empty unless
explicitly assigned. So even if SB5018 didn't fire, `echo ~` would
print an empty line — not the user's actual home directory.

Tilde expansion in bash is a *shell* feature that consults
`$HOME` at expansion time. Under DD#53, sash has no path to a
"current user's home directory" without `$(getenv HOME)` + bless.

## Options

1. **Reject `~` at parse time with a focused diagnostic.** New code
   SB5021 (or similar): "`~` tilde expansion is not available — use
   `$(getenv HOME)` and bless the result". DD amendment to DD#53 to
   document the explicit rejection.

2. **Reject `~` only when used unquoted as a path.** Some uses of `~`
   are intentional (e.g., literal tilde character in regex). The
   parser would need to distinguish path-position from other
   positions.

3. **Lower `~` directly to `$(getenv HOME)` plus auto-bless.** This
   mirrors what bash does at the cost of a hidden tainted-value
   capture. The auto-bless would have to be conservative
   (`known_externals` already handles HOME).

Option 1 is the strict DD#53 choice. Option 3 is the
user-friendly-but-magic choice. DG should decide before implementation.

## Related

- Journal 109 (the handle_cd discovery)
- DD#53 (env isolation)
- DD#24 (taint/bless)
- `sash/src/parse/expr.rs:138`
- `sash/src/validate.rs:388`
