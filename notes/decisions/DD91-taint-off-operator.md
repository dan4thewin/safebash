title: DD#91 — the `:!` taint-off modifier (a per-use bless-without-predicate)
status: accepted

Settled with DG (2026-09-04). `:!` is a new disposition operator: **`:~` (elide)
plus the taint guard disabled**. `${x:!}` / `$x:!` / `@a:!` / `@{a:!}` use a
tainted value *without* a `bless` predicate — a sanctioned per-use taint bypass.

## Motivation

DG: "a new operand like `$foo:!` which is equivalent to `:~` but also disables
the taint guard — I see it as a compromise between full bless use and the
`--no-taint` full abnegation."

The taint model (DD#24) has two poles: `bless V [[…]]` cleans a variable but
demands a validating predicate; `--no-taint` (DD#82) disables *all* taint
checking program-wide. `:!` is the middle ground — a **predicate-less bless at a
single use site**: the programmer vouches for this one value.

## Semantics

- **`:!` = `:~` + taint-off.** It **elides** when the value is the identity
  element (empty → zero tokens) and yields the value otherwise, exactly like
  `:~`; additionally the value is treated as **clean**, so a tainted subject is
  used without `bless` (no SB5020) and the result **does not propagate taint**
  (`array a=(${x:!} lit)` leaves `a` clean).
- **Wordless**, like `:~`: `${x:!foo}` is SB4051 (scalar), `@{a:!WORD}` is SB5033
  (array).
- **Elidable-position-only**, like `:~`: valid only where a token may disappear
  — a command argument or an array-literal element. A non-elidable position
  (e.g. a scalar assignment `z=${x:!}`) is **SB5046**. (So `:!` is *not* a way to
  assign a trusted value to a scalar; use `bless` for that. This mirrors `:~`.)
- Works for **scalar** (`${x:!}` / `$x:!`) and **array splat** (`@{a:!}` /
  `@a:!`), **braced and brace-less** (DD#90). `@a:!` bypasses the bless-all
  requirement on a tainted array.
- Does **not** bypass unrelated checks: an undeclared subject still trips the
  typo guard (SB5031) exactly as `:~`/`:-` do (all dispositions suppress it
  equally — `:!` is not special there).

## Implementation

Scalar `:!` lowers to `SsaValue::ElideEmpty { source, taint_off: true }` (the
`:~` spelling sets `taint_off: false`); array `@a:!` uses the `arrvals_trust`
operator → `ArraySplatDisposition { taint_off: true, … }`. The taint bypass is
applied in **both** taint passes so they agree:

- CST (authoritative, `validate.rs`): `check_param_expansion_taint` returns
  early for `:!` / `arrvals_trust`; `expr_uses_tainted` returns `false` (clean
  propagation); `check_elide_pexp` treats `:!` like `:~` (SB5046 outside a list).
- SSA backstop (`ssa_taint.rs`, defense-in-depth): `ElideEmpty { taint_off }` and
  `ArraySplat` with a trusting disposition classify **Clean**. (The backstop does
  not yet check these argument positions — task `ssa-taint-full-backstop` — so
  the flags are forward-compatible correctness; the CST gate enforces the bypass
  today.)

Parser: `split_operator` gains `:!`; `parse_splat_op` maps `@…:!` →
`arrvals_trust`; the brace-less lexer (`try_braceless_disposition`) accepts `:!`
wordless. Backends: native/c go through the SSA `ArraySplat`/`ElideEmpty`
(taint_off ignored at codegen — the emitted splat is identical to a wordless
`arrvals_default`); janet's `janet_splat_splice` gains an `arrvals_trust` arm.

No new SB code (reuses SB4051 / SB5033 / SB5046).

## Addendum (2026-09-09) — the elide is split out; `!` becomes a modifier

DG: *"so, if we split out the elide from `:!` but then allow limited
combinations of modifiers, like `:!~`, where `!` must come first…"* Approved
after discussion. This **supersedes** the "elidable-position-only" rule above.

`!` is not a disposition at all. `-` / `?` / `~` answer *what to do when the
value is absent*; `!` answers *how to treat the value we have*. Bundling them
made `:!` mean two things at once, and forced the one that mattered (trust) to
inherit the other's position restriction.

```
grammar:  ':' [ '!' ] [ '-' WORD | '?' MSG | '~' ]
```

| spelling | meaning |
|---|---|
| `${x:!}` | the value, trusted — **identical to bare `${x}` except for taint** |
| `${x:!~}` | trusted, and elides when empty (the old `:!`) |
| `${x:!-W}` | trusted, `W` when empty |
| `${x:!?M}` | trusted, abort with `M` when empty |

### `!` must come first, and that is forced

Not a style rule. `-` and `?` take a word that runs greedily to `}`:
`${u:-foo!}` yields `foo!` and `${u:?boom!}` yields `boom!`. A modifier placed
*after* a word-taking disposition would be swallowed by the word. So `!` leads
or it cannot exist. (The whole `:!X` space parsed as an error before this
change, so no existing program silently changes meaning.)

### `${x:!}` alone means "bare, but trusted"

An empty bare `$u` in a list position contributes an **empty argument**, not
nothing (`echo A $u B` passes three arguments). So removing the elide from `:!`
leaves it behaving exactly like the bare expansion. The modifier changes trust
and nothing else — which is why it is now valid in every position a bare
expansion is valid, including a scalar assignment.

That reverses this DD's original *"`:!` is not a way to assign a trusted value
to a scalar; use `bless` for that."* The justification given there was *"This
mirrors `:~`"* — i.e. the restriction was inherited from the elide mechanics,
not chosen on trust-policy grounds. With the elide gone the rationale goes with
it. `bless` remains the predicate-bearing form; `:!` remains the
predicate-less one.

### `@a:!` becomes an error, and should

A splat site must state its emptiness behaviour (bare `@a` is **SB5032**,
DD#69). Taint-off is not such a statement, so `@a:!` must become `@a:!~`,
`@a:!-WORD` or `@a:!?MSG`.

This is a break, and it is a fix: today's `@a:!` **is** a DD#69 violation
hiding in plain sight. The author asks for taint-off and silently receives an
emptiness policy they never wrote. The split surfaces it. DD#99's
`map m = @{a:!}` row becomes `@{a:!~}` for the same reason.

### Consequences

- `${x:!}` no longer elides. Existing list-position uses that relied on it
  become `${x:!~}`.
- `arrvals_trust` is retired: taint-off is a `taint_off` flag on the expansion,
  orthogonal to the operator, so the two cannot be conflated again.
- SB5046 (elide in a value position) now keys on the *elide*, so it fires for
  `:~` and `:!~` and not for `:!`.
- **The diagnostic has to render the `!` back.** Splitting `!` out of the
  operator means `pe.operator` alone spells `${x:~}` for an author who wrote
  `${x:!~}`, and the advice derived from it (`${x:-DEFAULT}`) *drops the
  taint-off* — so following it trades SB5046 for SB5020, removing the very
  trust the author asked for. The sigil is built from operator **plus**
  `taint_off` (`elide_sigil`), and the advice keeps the `!`
  (`${x:!-DEFAULT}`). Fixed 2026-09-26; specs `optional_args.sh` 84.33-84.36.


## Addendum (2026-09-27) — `!` vouches for the subject's read, and nothing else

The 2026-09-09 split defined `${x:!-W}` as "trusted, `W` when empty" but never
said what taint `W` itself carries. DG settled it:

> "x=$(getenv A); y=$(getenv B); z=$x:!-$y:! ; the `!` should only apply to the
> read of the variable's stored value"

So `!` is scoped to **the read of its own subject**. It is not a trust claim
about the expansion's result. In `${x:!-$y}` the `!` covers `x`; `$y` is a
separate read that owes its own `!` (or an `attest`). The rule runs in both
directions — a `!` in the default does not vouch for the subject either, so
`${x:-${y:!}}` is still SB5020 on `x`.

| spelling | verdict |
|---|---|
| `${x:!-${y:!}}` | clean — each read vouched by its own `!` |
| `${x:!-$y}` | SB5020 on `y` |
| `${x:!-$(getenv C)}` | SB5020 — nobody asked to bypass the `getenv` |
| `${x:-${y:!}}` | SB5020 on `x` |

This also answers the question filed as `dd91-bang-vouches-for-source-in-default`:
reading 2, and the outcome is a **spanned** diagnostic rather than the bare SSA
backstop.

### It was a bug, not just an unsettled question

`parameter_expansion_to_value` wrapped the subject in `SsaValue::Trusted` only
when `pe.operator` was `None`. The moment there was an operator, control passed
to `source_carrying_value` and the `taint_off` flag was **dropped on the floor**
— so the `!` of every `${x:!-…}` reached neither backend nor backstop. The
fully-vouched spelling DG asked for did not compile; it died on the unspanned
backstop. The wrap now happens once, before the operator dispatch, because the
subject *is* "the read of the variable's stored value".

Three validate sites had the mirror-image fault, treating `taint_off` as
covering the whole expansion including its default: `is_taint_source` (a
`!pe.taint_off &&` gate), `pexp_uses_tainted` and `check_param_expansion_taint`
(early returns that skipped the `:-` operand walk as well as the subject).

A fourth, subtler one: `source_is_int` matches `Var`/`Subscript` structurally,
so wrapping the subject hid an `int` subject from it and silently moved
`${n:!-7}` off the int identity-element path onto the string `atoi` path. `!` is
a taint annotation, not a change of type, so it now sees through `Trusted`.

Specs: `taint.sh` 91.24-91.33. journal 431.

## Cell enforcement (2026-10-16)

DD#123's cell-is-variable principle applies the existing four trust forms to a
cell too: `${a[i]:!}`, `${a[i]:!-W}`, `${a[i]:!?M}`, `${a[i]:!~}`. The parser
had routed all four to the unsupported-cell-colon diagnostic instead of the
shared trust parser. Corrected for arrays and maps, in words and arithmetic,
with the same brace-less/interpolation boundaries as a scalar.

Trust covers the stored value, **not the index expression**. Read enumeration,
taint-use/source checks and the shared expression walker now visit
`ParameterExpansion.index` as well as its operand. A vouched cell therefore
does not launder an unvouched index, and a variable read only as the index is
not dead. The index also remains subject to argv-splat and default-type rules.

Nor does trust perform a string-to-int conversion: bare `${s:!}` and
`${a[i]:!}` on strings still owe DD#77's disposition in arithmetic. The old
gate missed scalar imports as well as trusted cell imports; both now report
SB4030, while int subjects and actual dispositions preserve their behavior.
Specs `taint.sh` `ctrs.1`-`ctrs.21`; journal 512.

## Audit surface

`:!` is a deliberate validator bypass — the adversarial audit's highest-priority
target (AGENTS.md). It is inherently greppable in source. Rather than record it
in `--manifest`, DG asked for a broader **generated audit report of every
bless / taint-bypass site** (task `audit-taint-bypass-report`): the predicate
blesses, every `:!`, `--no-taint*` usage, and the external taint sources — so a
reviewer audits all bypasses without grepping.

## See also

- DD#24 taint model; DD#82 `--no-taint` full bypass; DD#84 `:~` elide; DD#90
  brace-less dispositions; DD#69 array-splat dispositions.
- Specs: `taint.sh` 91.1–91.11 (bypass + controls + rejections), `var_modifiers.sh`
  vm.bl13–15 (`$( )` operands). journal 243.
- Tasks: `audit-taint-bypass-report`, `janet-positional-default-present`.
