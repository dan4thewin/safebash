---
title: 318 — The modulo was the whole answer
---

DG: *drop backticks as sugar.*

DD#67's keep/drop table has carried this row since 2026-06-19:

| bash form | native | 1-to-1? | verdict |
|-----------|--------|---------|---------|
| `` `cmd` `` | `$(cmd)` | ~yes (modulo nesting) | review |

Every other row in that table is decided. This one said "review" for two
years, and the reason is visible in the cell next to it: *modulo
nesting*. A qualifier that size is not a footnote on a yes. `` `cmd` ``
maps to `$(cmd)` only as long as it does not nest, and nesting in bash
requires `` \` `` — which DD#64 removed. The mapping is partial, and a
partial mapping is precisely what the 1-to-1 test excludes.

The compromise we shipped instead — accept the flat form, reject the
nested one — split a single construct across the two verdicts. It needed
two diagnostics, two lexer paths, and an explanation.

## The mechanism came from the policy

DD#67 already says what a drop is: "A bash form that can **not** map
1-to-1 … is **dropped** (and the converter rewrites it into the native
form)." So there was nothing to decide about *how*. Reject in the
compiler; rewrite in `sash-convert`. And `sash-convert` already had
`convert_backtick_substitution` — the converter half has been shipped
the whole time.

`scan_backtick` collapses to: consume the body, emit the `` `backtick` ``
sentinel. The `nested` flag is gone, `Tok::Backtick` is gone, and the two
`parse_expansion_arm` arms that consumed it are gone. One diagnostic,
SB5076, for every spelling.

SB5076 is a new code rather than the SB5020 the nested case used. SB5020
is a **taint** code; using it for a syntax rejection was a category
error that the flat/nested split had papered over. The heredoc drop
(SB5002) is the convention this follows.

A backtick inside `"…"` is untouched — literal since 2026-08-28.

## What fell out

Removing the token exposed this, in `command_sub_to_expr`:

```rust
if content.starts_with('`') {
    return Expr { kind: ExprKind::Literal(content.to_string()), span };
}
```

A carve-out from when backticks were their own token. It outlived that,
and the surviving caller is `$( … )` inside a double-quoted string. So:

```
$ sash -c 'echo "$(`/bin/echo hi`)"'
sash: -c -> a.out (static binary)
```

It compiled. The **whole substitution** became a literal string, which
dodged the backtick rejection *and* `check_taint` — `$( … )` in
argument position is otherwise a hard SB5020 — and diverged from bash,
which runs the command. Three failures from one stale branch.

Deleted. The inner script is now re-lexed like any other, the sentinel
surfaces, and the case is SB5076. Pinned as `cov.103`, which previously
tested the nested-backtick wording that no longer exists.

## The general shape

This is the fourth time this campaign that a **legacy carve-out** —
not a missing feature, an explicit special case someone added on purpose
— turned out to be the hole. The carve-out is always narrower than the
construct that motivated it, and when that construct moves, the
carve-out keeps firing on whatever still matches its predicate.

Sweep unchanged: 4691 passing, 0 failing.
