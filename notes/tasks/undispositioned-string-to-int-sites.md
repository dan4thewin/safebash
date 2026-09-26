---
status: open
priority: P2
depends: DD89-amendment
gates: DG decision on scope
---

# Undispositioned string→int sites outside assignment and arithmetic

SB4030's own message states the rule:

> a string→int conversion goes through arithmetic and must carry a
> disposition: `$(( ${VAR:-0} ))` … or `$(( ${VAR:?msg} ))`

DD#77 enforces it inside `(( ))`/`$(( ))`, and `check_assignment` enforces it
for `int x = $s`. Everywhere else a string silently becomes an int through the
**lenient** `sash_atoi` (DD#72: non-numeric → 0).

`return` was the seventh site and is **fixed** (journal 411, native was also
diverging from the c backend). The rest are open pending DG's scope decision.

## The remaining sites

| site | sash | bash | accountable spelling exists? |
|---|---|---|---|
| `exit $s` | 0 | rc 2 + error | yes — `exit ${s:?m}` works |
| `wait $s` | rc 0, silent | rc 1 + error | yes — `wait ${s:-0}` works |
| `${a[$s]}` read | ✅ SB4030 when `s` is *declared* `str` (journal 412) | index 0 | yes — `${a[${s:?m}]}`, braced or brace-less |
| `a[$s]=` write | ✅ SB4030, same check (journal 412) | index 0 | same |
| `[[ -t $s ]]` | fd 0 | false | parses but is ignored |
| `$(arg $s)` | index 0 | n/a | parses |

Measured 2026-09-25 against `4578aed`+fix. Contrast — already rejected:
`int x=$s`, `(( n = s ))`, `x=${a[0]}` (string element), `${t:1:$s}` (SB4030),
`${t:$s:2}` (SB4047), `${a[s]}` / `${a[i]}` (SB4030), `${a[i+1]}` (SB4041).

## PARTLY RESOLVED (journal 412)

`a[$s]=Q` is now **rejected** (SB4030) when `s` is a declared `str`: read and
write share `check_subscript_key_kind`, so one fix closed both surfaces.

**The blocker below is still real for the fix side.** Measured 2026-09-25 —
none of the accountable LHS spellings parse:

| spelling | result |
|---|---|
| `a[${s:?m}]=Q` | SB4014 |
| `a[${s:-2}]=Q` | SB4014 |
| `a[$s:-2]=Q` | SB5011 |
| `a[$((1+1))]=Q` | SB5011 |

So the *tightening* shipped without the spelling, and the write diagnostic had
to be split from the read one: recommending `a[${s:?m}]=…` would have sent the
author from SB4030 straight into SB4014. The write message names what compiles
instead — an integer literal, an `int` variable, or converting first
(`int i=$(( ${s:-2} )); a[$i]=…`). Specs 80.49/80.50 pin that.

Lifting the DD#33 LHS restriction so `a[${s:?m}]=Q` parses remains open, and
is the only thing that would make the two surfaces symmetric.

Still open for the *undeclared* cases — see
`array-index-string-by-value-flow.md`.

### Original analysis

## The blocker: no accountable subscript-LHS spelling

On the **read** side DG found the working form: `${a[$(($s:?))]}` aborts on
`abc` and indexes correctly on `1`. On the **write** side nothing parses:

- `a[${s:-0}]=Q` → SB4014
- `a[$(( ${s:-0} ))]=Q` → SB5011 ("a command name must be a literal path")
- `a[$(($s:?))]=Q` → SB5011

So `a[$s]=Q` cannot be tightened until a spelling exists. **It cannot ship with
the other five.** Terser candidates DG raised, measured: `$a[$(($s:?))]` is
SB4020 (not a sash spelling at all) and `$a[$((s:?))]` is SB4041 "ambiguous
braced index" — the compiler already diagnoses that ambiguity by name.

## DG's steer

> Rather than prioritize bash consistency, I want to prioritize sash
> self-consistency.

That settles the direction: `${a[$s]}` and `[[ -t $s ]]` matching bash is **not**
a reason to keep them lenient. It does not settle the scope, and DG asked to
discuss the DD before any tightening.

## DD#89 needs an amendment either way

DD#89 says:

> A string argument coerces to int (atoi), **as in every other int context**;
> `exit` imposes no extra type restriction.

The consistency claim is false as implemented — nearly every other int context
rejects. The clause was written while settling a **range** question (removing
the `exit $intvar` reject); the type remark rode along unmeasured. Whatever the
scope decision, DD#89's rationale has to stop asserting a consistency that is
not there.

## Note for whoever implements

Each site has its own lowering, so this is not one switch:

- `exit` — `codegen/dispatch.rs::emit_exit` / `emit_c.rs::emit_exit`
- `wait` — `codegen/dispatch.rs::emit_wait` / `emit_c.rs::emit_wait_c`
- subscripts — `codegen/values.rs::lower_subscript_as_int`, `index_var_to_int`
- `-t` — `codegen/tests.rs::emit_isatty_test` / `emit_c.rs::lower_test_unary`
- `arg` — `codegen/value_handlers.rs::lower_arg_intrinsic`

The rejection itself belongs in `type_check.rs` (where SB4030 already lives),
not in codegen — codegen only sees a value whose type was already settled.

Also: `return`'s fix was a **native/c divergence** that no spec caught. Check
both backends at each site before assuming they agree.
