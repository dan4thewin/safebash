# 249 — DD#93 Stage 3: the `attest VAR OP` shorthand

**Branch:** `hand-parser`. Follows journal 248 (Stage 2, the shape tests).

## What it is

DD#93 Part B: for any unary test operator, the bare-operator form stands for
the predicate with that operator applied to the attested name.

    attest x i?          ≡  attest x [[ i? $x ]]
    attest x -n          ≡  attest x [[ -n $x ]]
    attest e in a i?     ≡  attest e in a [[ i? $e ]]

The point is not brevity for its own sake. The operator binds the attested
name **by construction**, so the wrong-var predicate that SB5020 exists to
catch cannot be written in this form — the shorthand is strictly safer than
the spelling it replaces, and `attest` still greps.

## The one thing the DD didn't settle

DD#93 Part B covers the scalar and array forms. It says nothing about the
two-binder **map** form (`attest k v in m OP`), which is a genuine fork, so I
put it to DG rather than picking. He chose: apply the operator to **both**
binders.

    attest k v in m i?   ≡  attest k v in m [[ i? $k && i? $v ]]

This falls out of an invariant already in the code — `taint_attest` requires a
map predicate to reference *both* the key and the value (SB5020,
`validate.rs:3454`). Binding only the value would have made the shorthand
SB5020 on arrival, i.e. useless for maps; binding both keeps the rule that a
declassification is justified by a predicate that actually checked everything
it declassifies. Recorded in DD#93 itself, not just here.

## Implementation

Pure parser sugar, entirely inside `parse_attest`:

- `peek_unary_test_op` — a bare word that is a unary test operator, using the
  same shape check as `try_parse_leading_unary` (`-X`, or a DD#93 `?`-suffixed
  shape test, `len >= 2` so the bare `?` stays out).
- The lead now also ends on such a word, not only on `[[`. Without that the
  operator was swallowed as another attest-all binder and the parse died
  looking for `in`.
- `attest_shorthand_test` builds the predicate from the args collected so far
  — the same shapes `classify_attest` decodes — one `TestExpr::Unary` for
  `[var]` and `[binder, array]`, an `And` of two for `[key, val, map]`.

Nothing downstream changed: it lowers onto the same args vector, so
`taint_attest`, `lower_if_attest` and `handle_attest_stmt` never learn the
shorthand exists. The parser tests assert exactly that — the shorthand and the
spelled-out form decode to the same `AttestForm` and the same predicate.

## A user-reachable ICE, found on the way

Smoke-testing `attest x -v` (which DD#93 says must stay SB5028-rejected "in
every form") panicked in codegen instead:

```
thread 'main' panicked at sash/src/codegen/tests.rs:195:21:
ICE: unimplemented unary test operator: -v
```

Not mine, and not the shorthand's: `attest x [[ -v $x ]]` ICEs identically on
the committed tree. Root cause is `check_test_constructs` (`validate.rs:753`),
which early-returned unless the command name was `[[`. An `attest` carries its
predicate as a `Test` arg exactly like `[[ ]]` does, so the whole gate — both
`check_unsupported_test_ops` (SB5028) and `check_regex_literal` (DD#39/#63) —
was silently skipped for attest predicates. The parser accepts `-v`/`-o`/`-R`
(`is_unary_test_op`), validate is the only thing that rejects them, and codegen
has no lowering, so the operator fell straight through to a panic.

Fixed by admitting `attest` to the same gate. All three now produce their
proper SB5028 diagnostics, and the regex checks reach attest predicates too —
a consistency the sweep confirms costs nothing (0 regressions).

DD#93's "What does NOT change" bullet claimed this property already held; I
amended it to record that it did not, and now does.

## Verification

3-mode sweep: **0 regressions**, 24 new rows (8 ids × 3 modes) — 18 passing,
6 janet-skipped (janet elides attest validation and lacks `getenv`).
4232 → 4250 passing, failing unchanged at 34. specs taint.sh 93.20–93.26,
including 93.22b (one non-integer element aborts the attest-all, so the
shorthand is demonstrably checked per element) and 93.25/93.26 pinning the
SB5028 behavior in both the shorthand and the full form.

Six new parser unit tests, written first.

## Next

S4 — the arith-assign form `attest numstr (( myint = numstr ))`, the last
stage of the bundle.
