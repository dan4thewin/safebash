---
title: "Fix: positional `${N:OP}` diverged from `${args[N-1]:OP}` (SB5042 + always-elide)"
date: 2026-09-02
---

# Positional `${N:~}` / `${N:-}` reached parity with the `args[]` form

DG reported: a function with `${2:~}` in a command, called with a single arg,
gives **SB5042** ("expects 2 arguments but was called with 1"). Investigating
uncovered **two** coupled bugs — the positional `${N:OP}` disposition form was
never tested (all DD#84 specs used `${args[k]:OP}`), and both the arity
analysis and the value lowering treated `$N` as something other than
`${args[N-1]}`, contradicting DD#84 §(a)'s explicit "`${args[i]}` / `$N`".

## Bug 1 — arity mis-analysis (spurious SB5042)

`transform/functions.rs::sig_scan_pexp` classified an `args[k]` read's
disposition (`:-`/`:~` → optional, DD#84) but sent a positional `${N:OP}`
through the `else` arm to `sig_record_name`, which marks `$N` **required**
unconditionally — ignoring the operator. So `${2:~}` demanded 2 args and
`f one` tripped SB5042. The equivalent `${args[1]:~}` compiled fine.

Fix: recognize a positional name in `sig_scan_pexp` and record it optional or
required per the same `:-`/`:~` test (`sig_record_positional`, hoisted the
`optional` computation to cover both the `args` and positional paths).

## Bug 2 — wrong disposition subject (always elided/defaulted)

Even with the arity fixed, `${2:~}` with **two** args still elided `$2`, and
`${2:-D}` with two args produced `D` instead of the value. Root cause:
`transform/expressions.rs::pe_source_value` computes the disposition subject,
and for a no-index expansion it returned `SsaValue::Var(lookup(&pe.name))` — a
scalar variable literally named `"2"`, which never exists. So the subject was
always empty → `:-` always defaulted, `:~` always elided, regardless of the
argument. (Bare `$2` was fine — it lowers via `positional_to_value`; only the
`${…:OP}` path went through the broken `pe_source_value`.)

Fix: route a positional name through `positional_to_value` (→ the current
function's `args[N-1]` cell), falling back to `Var(lookup)` for ordinary names:

```rust
None => positional_to_value(&pe.name, state)
    .unwrap_or_else(|| SsaValue::Var(state.lookup(&pe.name))),
```

This also corrects a bare braced `${N}` (which took the same no-operator
subject path) and is a strict superset of the old behavior — `positional_to_value`
only intercepts `"1".."9"` and `"#"` (and `${#…}` is already rejected upstream
by SB4034, so `"#"` never reaches here).

## Behavior after the fix

```
fn f { /bin/echo A $1 ${2:~} B }
f one       # A one B        ($2 absent → elided; NO SB5042)
f one two   # A one two B    ($2 read)

fn f { /bin/echo A $1 ${2:-D} B }
f one       # A one D B      (default)
f one two   # A one two B    (value)

fn f { /bin/echo $1 ${2:?need} }
f one       # SB5042         (:? is required, like ${args[1]:?} — compile-time)

fn f { /bin/echo $1 $2 ${2:~} }
f a b       # SB5047         (bare + disposition mixing, like the args form)
```

## Consistency notes

- `:?` on a positional stays **required** — a too-few call is SB5042 at compile
  time, which is stricter (and earlier) than bash's runtime require-abort, and
  matches `${args[N-1]:?}`.
- SB5047 (mixing bare + disposition) and SB5048 (required-after-optional) now
  fire for the positional form too, since it feeds the same `ArgSig`.

## Tests / verification

`spec/optional_args.sh` 84.19–84.24 (plan 18→24): elide absent/present,
default absent/present, `:?` required, mixing SB5047 — all `in_modes native,c`
(janet has no `args` model; unchanged). 3-mode sweep vs HEAD: **native 0
regressions, c 0, janet 0** (+6 native, +6 c). fmt/clippy clean; complexity
gate green — `sig_scan_pexp` was refactored (extracted `sig_record_pexp_subject`
+ `positional_index_1_9`) to drop cognitive 8→1 after the touch.

## Files

- `sash/src/transform/functions.rs` — `sig_scan_pexp` split;
  `sig_record_pexp_subject`, `positional_index_1_9`, `sig_record_positional`.
- `sash/src/transform/expressions.rs` — `pe_source_value` positional routing.
- `spec/optional_args.sh` — 84.19–84.24.
- `notes/decisions/DD84-optional-args-elision.md` — 2026-09-02 addendum.
