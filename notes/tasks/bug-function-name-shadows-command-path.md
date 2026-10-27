---
title: "Bug: a function may be named like a path (`fn /bin/true`); the validator then checks the external program while the compiled code calls the function"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "DG rules which names a function may take (see Question)"
  - "under that ruling, n2, n9, n10, n11 and n12 are either a definition-site rejection, or every pass treats the head as the function"
  - "no pass classifies a command head differently from the code the transform emits for it"
  - "r1/r2 follow the ruling together with bug-return-status-string-operand's value test"
  - "the taint controls t1-t8 keep their verdicts"
---

## Found

2026-10-27, while porting F1+F4 (journal 537), probing `return`'s operand. It
is **pre-existing**: every probe below gives the same result on the `1309385`
and `8e28c61` binaries and with F1+F4, in both backends, except r1, which
F1+F4 changed.

No DD defines what a function name may be. The parser takes any word, except
that `x=y` is an assignment (n4, SB4014 "expected '{' for function body") and
`-` alone is refused (n8, SB4014 "expected function name"). DD#125's SB5092
refuses a name the compiler already understands (n6, `fn echo`).

**The validator and the transform disagree about a path-named function:**

| Id | Program | Result |
|---|---|---|
| n2 | `fn /bin/true { echo hi }; /bin/true` | runs the function: `hi` |
| n12 | `fn /bin/true { echo hi }; /bin/true \| /bin/cat` | runs the function as the stage: `hi` |
| n10 | `fn /no/such { echo hi }; /no/such` | SB5095 2:1 "'/no/such' is not an executable program — no such file" |
| n9 | `fn ./x { echo hi }; ./x` | SB5051 2:1 "relative external path './x' — DD#23 requires an absolute path" |
| n11 | `fn /bin/true { … }; x=$(/bin/true); echo "[$x]"` | SB5020 3:6: the capture is tainted, as an external one is (DD#24 says a function capture is clean) |

So the checks see DD#23's external path, and the program calls the function.
In n2 every command-path check ran against a program that is never executed.

Taint held in every direction probed, the same as for a function with an
ordinary name:
- t1-t3 `fn /bin/true { g=$(getenv HOME) }` (t2 assigns `g=safe` first, t3
  writes `g` with `readline`): SB5067 at the body.
- t5 `x=$(getenv HOME); fn /bin/true { echo "[$x]" }; /bin/true`: SB5020.
- t7, which passes `"$x"` as an argument: SB5020.
- t4, t6, t8 are the same programs with `fn h`.

**Other names the parser accepts**: `fn a-b` (n1), `fn a.b` (n3), `fn %f`
(n5), `fn -x` (d1), `fn 9` (d2), `fn 9z` (z1) and `fn a[0]` (z3) all define
and call. `bug-declaration-names-unvalidated` found the last two and hands its
function-name question to this task. A numeral name collides with `return`'s
value form:
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

## Question

Which names may a function take?
- (a) Identifier names only, as for variables (`[A-Za-z_][A-Za-z0-9_]*`).
  This is the simplest rule, and `bug-declaration-names-unvalidated` would
  share its predicate. It is `bash --posix`'s rule, but it rejects
  `fn a-b`, a spelling bash scripts use. The converter's exposure is
  unmeasured.
- (b) **Recommended.** Reject the names the compiler already reads as
  something else:
  - a name containing `/`, which DD#23 makes a path. This is SB5092's
    principle applied to paths;
  - a numeral, which `return` reads as a status (r2). Bash rejects it too;
  - perhaps a leading `-`, which reads as an option.

  Everything else stays legal.
- (c) Keep every name, and make each pass resolve a head to the function
  first. This keeps n2, and bash calls the function there too, but a reader
  of `/bin/true` would have to know there is no such function before
  trusting the line.

## Fix direction

Under (a) or (b), reject at the definition, with a message that names the
rule. Then the transform can never call a function that the checks took for
an external program. Before choosing the code, measure how many existing
specs, and how much converter output, define a name the rule would reject.
