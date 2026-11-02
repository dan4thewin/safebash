---
title: "bug: a sexpr call of a slot function escapes the slot-arity check"
status: open
priority: P1
kind: bug
depends: []
gates:
  - "`fn f -> int { int n=$(len A); return $n }; echo $(( (f) ))` is SB5066 (today: native ICE, C prints garbage)"
  - "the same under an arithmetic subscript index, `$(( a[(f)] ))`"
  - "the advice does not say `pass it with =name`: a sexpr cannot pass a collection (`$(( (f =x) ))` is SB4020), so it says to capture first, `int n=$(f =x)`"
  - "`int k=$(f)` and a bare `f` keep today's SB5066"
---

A function whose body uses a collection slot needs every call to pass the
collection (DD100). SB5066 enforces that for a command call and a capture,
but a sexpr call in arithmetic gets no check:

```
fn f -> int { int n=$(len A); return $n }
echo $(( (f) ))
```

| Probe | Form | native | C |
|---|---|---|---|
| v4t | `$(( (f) ))` | ICE `undefined arrlen: A` (codegen/types.rs:284) | prints `4294939979` |
| v4 | `$(( a[(f)] ))` | same ICE | prints `0` |
| v4c | `int k=$(f)` | SB5066 2:9 | SB5066 2:9 |
| v4p | bare `f` (a string fn) | SB5066 2:1 | SB5066 2:1 |

Measured the same on the pre-F5 binary (2320434), on journal 544's
tree, and on the batch-531 branch (whose C prints `4294939947`). So this
is pre-existing, and the branch did not fix it.

Cause: validate.rs `check_call_args_in` → `check_one_call_args` →
`check_slot_arity` visits only `StmtKind::Command` calls, never an
`ArithExpr::FuncCall`. type_check's one-signature check ("called with both
1 and 0 collection arguments", SB4030) catches the sexpr call only when
another call passes a collection (journal 544's p3, spec sx.38). With no
such call there is nothing for it to compare against.

Fix direction: run `check_slot_arity` over sexpr calls through the shared
`ir::walk_arith_funccalls` (journal 544). Do not write another private
walker (`sweep-private-duplicates-of-shared-walkers`).

Rechecked during W1b-2 (2026-11-02, journal 559), on 685ea2f1 and the slot
summary implementation. The top-level example still ICEs natively;
`--emit-c` still succeeds. The same holds for a call inside a function:

```
fn inner -> int { : "${A[0]}"; return 7 }
fn outer -> int { : "$(len A)"; return $(( (inner) )) }
argv 1 a
int n=$(outer =a)
echo "$n"
```

The native error is `undefined array: A`, not implicit inheritance of
outer's collection. Generated C was not executed in this recheck. A mixed
command-capture/arithmetic call still gets SB4030 ("both 1 and 0 collection
arguments") in both compilers and both modes. No changed arity verdict or
new lost taint check was observed. W1b-2's mapper tolerates missing actual
arguments without inventing slot bindings; this P1 remains open.

Not probed: a `(( … ))` test (`if (( (f) > 0 ))`).

Probes: the session archive files/port-f5-probes.tar.gz (b73/v4*.sh,
b73/v4-results.txt); W1b-2's `arity-*` sources and before/after records are
in files/port-w1b2-slots-probes.tar.gz.
