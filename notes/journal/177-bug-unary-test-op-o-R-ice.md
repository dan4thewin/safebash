# 177 — bug: `[[ -o ]]` / `[[ -R ]]` reached a codegen ICE

Found while doing the reachability audit for the ICE-arms keep+cite pass: the
`panic!("ICE: unimplemented unary test operator")` at `codegen/tests.rs` was
**not** a pure invariant — it masked a reachable compiler crash.

## The gap

The parser's `is_unary_test_op` (parser.rs) accepts 24 operators, including
`-v`, `-o`, and `-R`. Codegen (`tests.rs`) implements only `-n`/`-z`/`-i`/`-t`
plus the `file_test_op_code` set (`-e -f -d -r … -N`). Validate rejected `-v`
(SB5028, DD#62) — but **not** `-o` or `-R`. So:

```
$ sash -c 'if [[ -o errexit ]]; then echo y; fi'
thread 'main' panicked at sash/src/codegen/tests.rs:232:
ICE: unimplemented unary test operator: -o
```

Same for `[[ -R x ]]`. A user-reachable input crashed the compiler — a
zero-tolerance bug, not an invariant to cite.

I diffed the full parser-accepted set against the codegen-handled set: exactly
`-o` and `-R` fall through (every other accepted operator is handled, and `-v`
is gated by validate).

## Fix — reject at validate (the `-v` precedent)

`-o` (shell-option state) and `-R` (nameref state) have no analogue in sash, so
the correct behavior is the same as `-v`: reject at validate with SB5028, before
codegen. Extracted `unsupported_unary_test_hint(op) -> Option<&str>` (the three
rejected operators + their hints) and drove the `check_unsupported_test_ops`
Unary arm from it:

```
[[ -o … ]]  →  SB5028 sash has no `[[ -o ]]` — shell-option tests are unsupported
[[ -R … ]]  →  SB5028 sash has no `[[ -R ]]` — namerefs are unsupported
[[ -v … ]]  →  SB5028 sash has no `[[ -v ]]` — use -n to test for a non-empty value
```

With `-o`/`-R`/`-v` all gated upstream, the `tests.rs` panic is now genuinely
unreachable, so it stays as a drift backstop **with a keep+cite `// INVARIANT:`
comment** pointing at `is_unary_test_op` + `unsupported_unary_test_hint`.

## Specs

`spec/wordsplit.sh` 62.20 (`-o`) and 62.21 (`-R`) → SB5028, alongside the
existing 62.5 (`-v`). Picked 62.20/62.21 to dodge a **pre-existing** `chk 62.11`
collision (types.sh and wordsplit.sh both define it — filed nothing yet, noted
here). 0 regressions native/c/janet; 6 new passing (2 × 3 modes); native gate 0.

## Follow-up (not this commit)

`file_test_op_code` implements `-k`/`-g`/`-u` (sticky/setgid/setuid) that
`is_unary_test_op` does **not** accept — so `[[ -k f ]]` won't parse as a unary
test. That's a missing-feature gap (parser under-accepts), not an ICE; worth a
task if those file tests are wanted.
