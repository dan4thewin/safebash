title: "Enforce conservative identifier-only function names"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "DD125's accepted ASCII identifier grammar is enforced at every source function definition, for every return kind and whether called or unused"
  - "historically accepted non-identifier definitions reject at their names, including paths, numerals, digit-led names, punctuation, leading minus and subscripts; valid-name controls remain legal"
  - "no pass classifies a command head differently from the code the transform emits for it"
  - "fname/r1 and fname/r2 reject their invalid definitions; independent legal-name fixtures prove the return-operand guard rather than borrowing that earlier rejection"
  - "reserved-name and duplicate-definition rules remain independent; __ names stay available, and no function-name restriction is imposed on external filenames"
  - "reestablish valid-name taint controls under current DD127/DD128; invalid path-name programs and the former clean-function shortcut are not preservation evidence"
  - "converter impact is measured and affected names are handled explicitly without changing identities or silently presenting unhandled names as valid native output"
  - "specs for the new refusals fail before the naming guard and under its reversion; valid-name controls and existing unit/static/spec gates pass with measured predictions"

## Decision, 2026-11-09

DG chose **"yes to conservative names"**, accepting the proposed variable
identifier grammar:

```text
[A-Za-z_][A-Za-z0-9_]*
```

DD125 records the rule. It covers every accepted definition spelling and
every function return kind, including unused definitions. Existing
reserved-name, variable/function-collision, placement and single-definition
rules remain. The `__` namespace is not newly reserved.

Reject paths, numeric and digit-led names, hyphens, dots, brackets, leading
minus and other punctuation, and non-ASCII names. Do not retain arbitrary
names by giving functions lookup precedence, and do not silently normalize
spellings into identifiers. A legal external filename is not a function
name and is unaffected.

The narrower collision-only ban and arbitrary-name/function-first lookup
alternatives were not selected. This deliberately costs compatibility with
Bash names such as `read-file`; converter exposure is not yet measured,
and no automatic renaming scheme has been chosen.

Only the policy question is closed. This P2 bug remains open for enforcement;
no compiler or converter change is claimed by the decision.

## Found

Historical evidence, not rerun for the policy recording: found 2026-10-27
while porting F1+F4 (journal 537), probing `return`'s operand. It was
**pre-existing**: every probe below gave the same result on the `1309385`
and `8e28c61` binaries and with F1+F4, in both backends, except r1, which
F1+F4 changed. Later taint and return-kind rules supersede some premises
of these fixtures; reestablish legal controls before implementation.

At that point no DD defined a general function-name grammar. The parser
took any word, except that `x=y` was an assignment (n4, SB4014 "expected
'{' for function body") and `-` alone was refused (n8, SB4014 "expected
function name"). DD#125's SB5092 refused a name the compiler already
understood (n6, `fn echo`).

**The probes exposed a path/function resolution disagreement:**

| Id | Program | Result |
|---|---|---|
| n2 | `fn /bin/true { echo hi }; /bin/true` | runs the function: `hi` |
| n12 | `fn /bin/true { echo hi }; /bin/true \| /bin/cat` | runs the function as the stage: `hi` |
| n10 | `fn /no/such { echo hi }; /no/such` | SB5095 2:1 "'/no/such' is not an executable program — no such file" |
| n9 | `fn ./x { echo hi }; ./x` | SB5051 2:1 "relative external path './x' — DD#23 requires an absolute path" |
| n11 | `fn /bin/true { … }; x=$(/bin/true); echo "[$x]"` | SB5020 3:6: the then-current classifier treated it as an external capture |

So the checks saw DD#23's external path, and the program called the function.
In n2 every command-path check ran against a program that was never executed.

The old clean-function-capture assumption is not a preservation requirement:
DD127 now classifies taint at the read boundary, and DD128 governs its
resolution. Fixing name resolution must not restore a clean-name shortcut.

Taint held in every direction probed, the same as for a function with an
ordinary name:
- t1-t3 `fn /bin/true { g=$(getenv HOME) }` (t2 assigns `g=safe` first, t3
  writes `g` with `readline`): SB5067 at the body.
- t5 `x=$(getenv HOME); fn /bin/true { echo "[$x]" }; /bin/true`: SB5020.
- t7, which passes `"$x"` as an argument: SB5020.
- t4, t6, t8 are the same programs with `fn h`.

**Other names accepted by those probes**: `fn a-b` (n1), `fn a.b` (n3),
`fn %f` (n5), `fn -x` (d1), `fn 9` (d2), `fn 9z` (z1) and `fn a[0]` (z3)
all defined and called. `bug-declaration-names-unvalidated` found the last
two; this task owns function-name enforcement. A numeral name collided
with `return`'s value form:
- r2: `fn 9 { return 4 }; fn f { return 9 }` returns 9. `9` is a status value.
- r1: `fn 99999999999999999999 { return 4 }; fn f { return 99999999999999999999 }`
  returns 4. Since F1+F4, a word that does not fit in i64 heads a producer,
  so the function is called. Before, any all-digit word was a value, and r1
  hit the native int-lowering ICE (C: "arithmetic overflow", rc 2).

Probes: `files/port-f1f4-probes.tar.gz`, directory `fname/`. `index.txt`
maps n1-n8, `all.{base,pre,cur}.txt` hold every probe's result on the three
binaries, and d1/d2 are copies from `r4/`. z1-z3 were run on the F1+F4
binary only; `bash.txt` holds the bash 5.0.17 measurements below.

**Bash, for reference.** Outside POSIX mode bash defines every name above
except a numeral: `9() { … }` is "not a valid identifier". It calls the
function for `/bin/true`, `./x` and `-x`, as sash does. `bash --posix`
rejects every name that is not an identifier.

## Fix direction

Reject at the definition, with a diagnostic identifying the name and its
identifier rule, before passes rely on a distinction between function
names and external paths. No diagnostic number or parser/validator
implementation choice is selected here. Reuse one canonical identifier
predicate with the variable-name checks; coordinate the declaration/for
guard without making either repair depend on the other landing first.

Cover every accepted definition spelling, all three return kinds, unused
definitions, and calls through ordinary/captured/arithmetic/stage/return
positions where otherwise legal. Use valid-name counterparts, including
underscores and digits after the first character. Keep reserved-name and
duplicate-definition guards independently pinned.

Use otherwise valid, unique definitions so a path-existence, type,
missing-return or duplicate-name diagnostic cannot masquerade as the name
guard. The numeric-name fixtures now require definition refusals, not a
new precedence choice. `bug-return-status-string-operand` still owns
numeral range/type checks and return value/producer classification; use
legal function names to prove that repair. External return producers
remain governed by their existing rules.

Before predicting gate counts, census existing specs and converter output
for affected names. A converter migration must preserve every function
identity and its calls, not blindly replace punctuation with underscores:
`read-file` and `read_file` may be distinct Bash names. Where a safe
mechanical conversion is unavailable, DD57 already supplies the explicit
`# MANUAL:`/unchanged-source route. Do not label such output as fully
converted. The migration mechanism remains to be assessed, not silently
selected by this policy recording.

Add focused specs at fresh IDs above database history, prove the relevant
refusals on the pre-change binary and by an independent naming-guard
reversion, then run the ordinary gates with measured predictions.
