# 250 — DD#93 Stage 4: `attest numstr (( myint = numstr ))`

**Branch:** `hand-parser`. Completes the DD#93 bundle (journals 247–249).

## What it is

DD#93 Part D — the last stage. An int-assignment *is* the predicate:

    n=$(arg 1)
    attest n (( c = n ))
    echo $(( c + 1 ))

The conversion can only succeed on a valid, representable i64, so **performing
it proves the property**. On success it declassifies *both* names: `n` (the
source string) becomes clean, and `c` is bound clean to the converted value.
On failure it aborts with the ordinary attest-fail message.

This closes the DD#77 `-i`/disposition split that opened the whole thread:
`i?` validates-and-branches (keeping the string), a disposition converts
inline, and this form validates-converts-declassifies in one accountable step.

## Shape and design

`classify_attest` now returns a **predicate** alongside the form:

```rust
pub enum AttestPredicate<'a> {
    Test(&'a TestExpr),
    ArithConvert { target: &'a str },
}
```

The form itself stays `Scalar { var: "n" }` — which is the nice part. Taint
clearing, the form/kind checks, and the scope rules all keep treating this as
a scalar attest of `n`; only the *justification* is different.

The predicate is accepted **only** in the exact shape `(( TARGET = SOURCE ))`
with `SOURCE` the attested name. A wider expression (`(( t = s * 2 ))`) proves
nothing about `s`, and a different source proves nothing about the attested
name, so neither is this form. Same-name (`attest s (( s = s ))`) is **SB5053**
— DD#93 defers in-place retype, and the source is a string while the target is
an int.

Lowering is two existing SSA instructions, so **no backend learns a new form**:
an `Attest` gated on `i?` (giving the abort and the clean source), then an
ordinary int assignment of the validated source to the target — exactly what
`(( TARGET = SOURCE ))` would have emitted alone. The `i?` gate is what makes
the second step safe: `sash_atoi` cannot overflow on an already range-checked
value.

DG settled the `if` form: it is supported, branching instead of aborting, with
the target **declared but not assigned** on the else path. "Declared but not
assigned" is given sash's existing meaning — `int x` reads as `0`, so the else
path reads `0` too, rather than inventing an empty arithmetic value.

## Three rules that had to learn about the form

The predicate is a real assignment sitting in a command's args, so rules
written for `$(( … ))` *rvalues* saw it and objected. Each exemption is
narrow — pointer identity against the specific predicate expression, collected
recursively so it also covers `if attest n (( c = n ))`:

- **SB4020** ("an assignment inside `$(( … ))` can't take effect") — it is an
  assignment *by design*, and the transform emits the store.
- **SB4030** ("bare string in arithmetic needs a disposition") — this is
  DD#93's stated exemption, and the reason the form exists: the `attest`
  supplies the accountability a disposition otherwise would.
- **SB5031** ("assigned nowhere") — the attest *introduces* the target, so
  scope collection has to see it.

## janet

`emit_janet_attest` writes a comment ("validation elided in janet") for `[[ ]]`
predicates. That is defensible for a predicate, but Part D is not a predicate
to elide — it *binds a value*, and eliding it left the target undefined (a
janet compile error, "unknown symbol c"). So janet emits this form in full:
the `_isint` gate, the same abort message native/c produce, and the binding.
All three backends now agree.

The `if` form is a different story, and not mine: **janet doesn't implement
`if attest` at all**, in any form. It falls through to the general-command path
and emits `(os/execute ["attest" …])`, which fails at runtime for the existing
`[[ ]]` form too ("bad slot #2, expected string, got true"). Filed as
`janet-if-attest-emitted-as-command` (P2); specs 93.33/93.34/93.31b gated
`native,c`.

## Verification

3-mode sweep: **0 regressions**, 27 new rows (9 ids × 3 modes) — 24 passing,
3 janet-skipped. 4250 → 4274 passing, failing unchanged at 34. specs taint.sh
93.30–93.37, covering the binding, the abort (and its message), both names
being declassified, the `if` form's two paths including the `0` on the else
path, SB5053, and the SB4030/SB4020 exemptions — with 93.36 pinning that the
*same* expression outside an attest is still rejected, so the exemption is
scoped to the form and not a hole in DD#77.

Four new parser unit tests, written first.

## Complexity

Six functions this commit touches tripped the pre-commit complexity gate. They
were brought back under it by extracting flat helpers rather than bypassing:
`classify_attest` (cc 17→6, splitting out the conversion decode and the
form-by-arity table), janet's `emit_command` (cc 15→2, its dispatch split into
control-flow vs value/IO builtins), `lower_if_attest` (13/7 → 7/6),
`validate` (cog 11→5), `check_sexpr_string_producers` (cog 10→5) and
`emit_janet_attest` (cog 8→2). Behavior is unchanged — the full sweep was
re-run after the refactor and still shows 0 regressions.

## DD#93 is complete

S1 rename (247), S2 shape tests (248), S3 shorthand (249), S4 this. The
follow-ups DD#93 listed remain open: a UTF-8-aware shape test, the remaining
POSIX classes (`a?`, `d?`, …), the array analogue of the arith form
(`attest e in a (( … ))`), and same-name in-place retype.
