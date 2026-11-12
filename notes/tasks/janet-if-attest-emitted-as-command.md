title: "janet: `if attest …` is emitted as an external command"
status: deferred
deferred-by: DD104
priority: P2
kind: bug
depends: []
gates:
  - "janet: `s=abc; if attest s [[ -n $s ]]; then echo yes; else echo no; fi` prints `yes` (matches native/c)"
  - "janet: `s=abc; if attest s (( c = s )); then echo ok; else echo 'not an int'; fi` prints `not an int`"

## Found

2026-09-07, implementing DD#93 Part D (journal 250). The Part D form in the
`if` position failed to compile under `--janet` ("unknown symbol c"), and the
cause turned out not to be Part D at all: **janet does not implement `if
attest` in any form.**

`attest` in a condition falls through to the general-command path and is
emitted as an external process:

```janet
(if (zero? (os/execute ["attest" "s" (not (or (nil? s) (empty? s)))] :p))
```

which fails at runtime — `error: bad slot #2, expected string, got true` —
because a boolean was spliced into an `os/execute` argv. With the Part D form
the same path produces `(set c …)` inside the argv, so `c` is never declared
and janet rejects the program at compile time.

Reproduces with the pre-existing `[[ ]]` predicate form, so this is not
DD#93-specific:

| source                                                     | native/c | janet |
|------------------------------------------------------------|----------|-------|
| `s=abc; if attest s [[ -n $s ]]; then echo yes; else echo no; fi` | `yes` | runtime error |
| `s=abc; if attest s (( c = s )); then … else … fi`          | `not an int` | compile error |

The **statement** forms are fine: `attest s [[ … ]]` emits its "validation
elided" comment, and (since DD#93 Part D) `attest s (( c = s ))` emits the
full gate + conversion + binding, matching native/c including the abort
message.

## Fix sketch

`emit_janet.rs` dispatches commands through `emit_command`, but the `if`
condition path resolves the command generically before the `BuiltinCmd::Attest`
arm is consulted. The condition lowering needs the same builtin dispatch the
statement path has, with an `attest` condition becoming:

- `[[ ]]` predicate form → the predicate expression itself as the condition
  (janet elides the *enforcement*, but in the `if` form the predicate is the
  condition and cannot be elided);
- Part D form → `(if (_isint s) (do (set c (_num s)) true) false)`, i.e. the
  gate as the condition with the binding on the true path, mirroring
  `lower_if_attest_convert` in `transform/control_flow.rs`.

Note the target must be declared *before* the branch to match native/c, where
it is declared-but-unassigned (`0`) on the else path (DG, 2026-09-07).

## Specs

`taint.sh` 93.33/93.34 (the Part D `if` form) are gated `native,c` for this.


## Deferred to post-beta (DD#104, 2026-09-12)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** — it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.
