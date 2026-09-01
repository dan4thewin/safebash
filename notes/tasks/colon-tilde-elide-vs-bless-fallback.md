---
title: "`:~` syntax collision — DD#84 elide (no word) vs the `${var:~DEFAULT}` bless-fallback (never built)"
status: ready
priority: P2
kind: decision
depends: []
gates:
  - "DG decides whether the bless-fallback `${var:~DEFAULT}` is still wanted; if so, it and DD#84 elide are given non-colliding syntax; DD#84 (and DD#24/journal 036) reconciled; specs pin the chosen forms"
---

# `:~` — DD#84 elide vs the `${var:~DEFAULT}` bless-fallback

DG (2026-09-01): "`:~` in variable as elide collides with use in bless context …
we have a syntax for 'use this value if the bless test fails' — the port 8080
examples."

Two meanings want the **same** `:~`:

- **`${var:~DEFAULT}` — bless-fallback, `:~` TAKES a word** (from
  `notes/journal/036-dd24-taint-quarantine.md`). "If unset **or invalid (fails
  bless)**, use DEFAULT" — contrast `${1:-8080}` = plain "if unset":
  ```
  bless [[ ${port:~8080} =~ ^[0-9]+$ ]]
  ${1:~8080}   # if unset or invalid, use 8080 (bless-only)
  ```
- **DD#84 elide, `:~` takes NO word.** `${x:~}` removes the token in a list
  position; `${x:~foo}` (a word) is rejected **SB4051**.

DD#84's SB4051 ("`:~` takes no word … use `${port:-WORD}`") is exactly what
breaks `${port:~8080}`. Verified today:

```
port=$(getenv PORT)
/bin/echo "${port:~8080}"   # → SB4051 (DD#84 elide claimed the syntax)
```

## Important: the bless-fallback was NEVER implemented

`git log -S':~'` shows `:~` first entered the parser at `496ece5` (DD#84, this
session) — never before. The ratified `DD24-taint-quarantine.md` does **not**
mention `:~` or `${var:~DEFAULT}` at all (its bless-on-failure is only "stderr +
exit 1" / "false branch," no default value). The `:~8080` form lives **only** in
`notes/journal/036`, an early DD#24 exploration that was dropped before DD#24
was finalized.

So this is **not** a clash of two shipped features — DD#84's elide is the sole
`:~` implementation. The real question is a design one: **does DG still want the
bless-fallback default** (a very useful "validated-or-default" idiom), and if so
how does it coexist with elide on `:~`.

## Decision needed (DG)

If the bless-fallback is wanted, pick how `:~` is shared/split. Options:
- Keep `:~` = **elide** (built) and give bless-fallback a new spelling
  (e.g. `${var:~?DEFAULT}`, or a `bless … else DEFAULT` clause).
- Let **`:~` take an optional word**: `${x:~WORD}` = "WORD unless x is
  present-and-valid", `${x:~}` (no word) = elide. Removes SB4051; unifies the
  two (elide = the no-word degenerate case). Needs care that "valid" means
  "passed the enclosing bless" only in a bless context.
- Drop the bless-fallback idea entirely (it was never built) and keep `:~` =
  elide, closing this out as wontfix.

Then reconcile **DD#84** (its "never takes a word" / SB4051 rule) and record the
outcome; add specs (the port-8080 fallback case + the elide case).

## Code touchpoints

- DD#84 elide: `parse/expr.rs:833` (cell disposition), `expr.rs:1135` (modifier
  op list), `parse/parser.rs:2621` (SB4051 word reject), `validate.rs:1884+`
  (`check_elide_positions`, SB5046).
- `notes/decisions/DD84-optional-args-elision.md`;
  `notes/journal/036-dd24-taint-quarantine.md` (the unbuilt `:~DEFAULT` sketch).

## Related (separate, lower concern)

- The `~` (tilde/home → `$(getenv HOME)` + bless, DD#53) overload on the `~`
  glyph is a *different* thing and not the collision DG meant here.

