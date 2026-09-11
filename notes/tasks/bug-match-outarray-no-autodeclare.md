---
title: "bug: `match S P ARR` does not auto-declare its out-array, unlike every other bulk fill"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`if match abc \"(a)(b)\" g { … }; echo $(len g)` compiles with no prior `array g=()` — as `read -a g` and `mapfile g` already do"
  - "the fix registers the out-array in BOTH the transform (autodeclare_bulk_target) and the type checker (register_bulk_read_target), matching the existing bulk-fill precedent"
  - "a spec pins the undeclared form for `match`, alongside the existing ones for read -a / mapfile / argv"
---

## Found

2026-09-08, while confirming the auto-declaration precedent for the
`switch-free-read-family` redesign.

## The inconsistency

Every bulk fill auto-declares its target — except `match`:

| form | undeclared target |
|---|---|
| `argv 1 a` | ✅ compiles |
| `read -a a` | ✅ compiles (n=2) |
| `mapfile a` | ✅ compiles (n=2) |
| `mapfile -m m` | ✅ compiles, as a map |
| `read x` (scalar) | ✅ compiles |
| **`match abc "(a)(b)" g`** | ❌ **SB5039** |

```sh
if match abc "(a)(b)" g { echo hit }
echo "n=$(len g)"
# → SB5039: `len g` — `g` is introduced nowhere (not a variable, array,
#   map, loop variable, or `args`); this reads as 0. Declare it, or fix the typo
```

Adding `array g=()` makes it work (`hit`, `n=3`). So the out-array fill itself
is fine — only the *introduction* is unregistered.

## It is an oversight, not a decision

**DD#63** specifies the out-array form as deliberately matching the others:

> "**`match SUBJECT PATTERN OUTARRAY`** — out-array form (the array is the
> trailing positional, **consistent with `mapfile -m m` / `read -a a`**)"

The intent was consistency; the auto-declare half was not carried across.

## Mechanism

`autodeclare_bulk_target` (transform) and `register_bulk_read_target`
(type_check) handle `argv`/`read`/`mapfile`. `validate::collect_scope_names` →
`collect_read_targets` dispatches on the command name (`"read"`,
`"mapfile" | "readarray"`, `argv`) and has no `match` arm, so the out-array
name is never added to the scope set and SB5039's "introduced nowhere" check
fires.

Note the diagnostic is doubly misleading here: it names `len` as the failing
construct and suggests a typo, when the name *is* introduced — two lines above,
by the `match`.

## Note

Filed at DG's request during the read-family design discussion. Independent of
that redesign: `match` is not part of the `read`/`mapfile` family being
reshaped, so this should be fixed on its own terms.

## Resolution

Fixed 2026-09-11. The filing named three dispatch sites; there were five, and
`matchall` (added by DD#63 after this was filed) had the same gap in two of
them. All five now dispatch on `"match" | "matchall" if c.args.len() == 3`, so
the 2-arg boolean form still binds nothing:

| site | file |
|---|---|
| `collect_read_targets` | `validate.rs` |
| `command_writes_args` | `validate.rs` |
| `check_command` → `register_newread_target` | `type_check.rs` |
| `try_handle_match_into` | `transform/stmt_handlers.rs` |
| `emit_janet_match`, `emit_match_into_condition` | `emit_janet.rs` |

Spec: 63.mi1–63.mi5 in `spec/regex.sh`. mi1–mi3 are gated `native,c` — janet
has a separate pre-existing bug (`janet-match-outarray-var-in-condition`) that
emits the auto-declaring `var` inside the if-condition expression; it
reproduces with `matchall` on unmodified code, so it predates this fix.
