# 547 - a non-void function's answer is handled

2026-10-30. F6 unit A (Order item 13). F6 opens with B15: a bare call of a
status-returning function discarded the status, so
`fn f { return 3 }; f; echo after` printed `after` and exited 0. DD55
already made a bare non-void call a compile error, but only `-> int`
functions were checked. DG's rulings of 2026-10-29 and 2026-10-30
(`q-suppression-on-user-functions`) settle the rest:
- `-` stays legal on a user function, but discards only its returned value;
- `-` on a void function is an error;
- a non-void function's answer is caught, or deliberately discarded with `-`.

B15's sub-decisions are decide-and-report, so this unit asked DG nothing.
The DD55 addendum of 2026-10-30 states the rule. This entry records how it
was built and checked.

## The rule

Every function is int (`-> int`), status or void. It is void when its only
exits are `return 0`, a bare `return`, the end of the body, and self-tail
calls. A tail call `return g` hands back g's result:
- if every such g is void, f is void;
- if any is int or status, f returns a status, and an int g's value becomes
  that status. Probe t1 shows it: `fn g -> int { return 3 }; fn f { return
  g }; f || echo "caught $?"` prints `caught 3`.

Any other return producer gives a status: an external, `[[ ]]`, `(( ))`, a
status builtin, or a redirected `return g >file`.

- SB4016 now covers status functions. It fires on:
  - a bare statement;
  - the tail of a list (`/bin/true && f`);
  - `f &`;
  - a capture `$(f)`, with the advice `$(- f)`.

  Reading `$?` afterwards is not handling (nonvoid.3). The left side of
  `&&` or `||` is a condition, and stays legal (nonvoid.21).
- `if x=$(f)` stays SB4025 (DD88), and the SB4016 at the same position is
  dropped (probe q22, nonvoid.15).
- SB4076 is new: a `-` with nothing to discard. That is `-` on a void
  function, or `$(- f)` on an int f. A stage or a job is a process with a
  status of its own, so `- v | /bin/cat` and `- v &` stay legal for a void
  v (nonvoid.24).
- SB4017's scan now sees return producers. Take an int function with
  `return 0` on one path and `return g` (g int) on another: pre rejected it,
  and now it compiles (nonvoid.20). An int function whose exit is
  `return g` with a void g is SB4017 now; pre printed `g [0]` (nonvoid.16).
- Deleted: journal 530's suppressed int self-tail specialization, and
  `TailLoop.entry_readonly`.
  - `return $(- f …)` inside an int f is SB4076 now, so nothing reaches
    that code.
  - GUIDE's DD27 section still described it. The accumulator factorial
    written that way printed 120 on the pre binary; now it is SB4076 at
    7:10.
  - GUIDE's paragraph is replaced by a short statement of the three
    classes.

The code:
- `ssa.rs`: `FnClass`, `ExitSummary`, and three tables on the lowering
  state: `exits_in_force`, `class_in_force` and `class_static`.
  `call_class` picks the table.
- `transform/functions.rs`: the classifier.
  - `exit_summary` records each body's exits, using `classify_producer`,
    `exit_yields_zero` and `returned_callees`.
  - `classify` is a least fixpoint over the summaries.
  - `static_classes` builds the static table.
- `transform/entry.rs`: `register_functions` builds `class_static`.
- `transform/statements.rs`:
  - `lower_function_stmt` brings a definition into force;
  - `analyze_function_returns` is SB4017;
  - `check_void_suppression` is SB4076.
- `transform/stmt_handlers.rs`: `check_non_void_bare_call` is SB4016, for
  both classes.
- `transform/expr_handlers.rs`: `check_function_capture` and
  `call_spelling` check captures.

## Redefinitions

The first version classified each name by its last definition, and it passed
gate 1. Probing redefinitions afterwards showed it wrong in both
directions. The table also shows pre:

| Probe | Program | pre | gate 1 | now |
|---|---|---|---|---|
| d1 | `fn f -> int {return 3}` / `f` / `fn f {echo two}` / `echo after` | SB4016 int 2:1 | accepted | SB4016 int 2:1 |
| d2 | `fn f {echo one}` / `f` / `fn f -> int {return 3}` / `echo after` | `one after` | false SB4016 | `one after` |
| d3 | as d2, but the second f is `{return 3}` | `one after` | false SB4016 | `one after` |
| d4 | `fn f -> int` / `fn g { f }` / `g` / `fn f {echo two}` / `echo after` | SB4016 int 2:8 | accepted | SB4016 int 2:8 |
| r3 | `fn f {return 3}` / `f` / `echo "after rc=$?"` / `fn f {echo two}` | `after rc=3` | accepted | SB4016 status 2:1 |

A call runs the definition in force where it stands; the inliner already
used the definition lowered last before the call. The check now uses the
same definition, from one of two tables.
- **In force.** `lower_function_stmt` inserts each definition's exit
  summary into `exits_in_force`, then reclassifies with `Absent::Status`: a
  name not yet defined counts as an external, so as a status.
- **Static.** The `fn_NAME` templates and SB4017 have no call site, so they
  use `class_static`.
  - It is built before any body is lowered, over the names defined once,
    with `Absent::Unknown`.
  - `drop_unknown` removes every function whose class depends on a
    redefined name. Such a function is checked where it is inlined.

nonvoid.25-nonvoid.29 pin d1, d4, r3, d2 and d3. The gate-1 binary reddens
exactly those five, and so does a last-definition stub on the final tree.
On pre, only .27 fails; it is B15's shape.

Two programs that are already SB5041 gain a second error. There is no spec
for either; this journal is the record.
- `fn f { return g }; f; fn g { return 3 }` now adds SB4016 (status) at 2:1.
- `fn g { f }; g; fn f -> int {...}` now adds SB4016 (int) at 1:8.

## Spec rewrites

31 existing specs change, and keep their ids:
- **12 respellings** (the assertion is kept). Bare status calls are
  respelled with `||`, `if`, `-` or `return g`: cov.330, rdr.39,
  74.55-74.60, 74.64b, 74.67, pf.71 and div.2. div.2 also gains a
  `return 0`, since `f || exit 1` alone is SB4037.
- **5 dropped `-`s.** tmpcap.19 and tmpcap.20 drop the `-` from `- walk`, a
  void function. supval.15, supval.16 and supval.28 drop it from `- safe`.
- **14 flips** to the new rule:
  - supval.1-supval.6, .9-.12, .14, .17 and .22, to SB4076 or SB4016;
  - constw.19, to SB4076 at 7:8 (pre printed 7).

Probes r01-r14 show each respelling behaves the same on pre and on the
final binary, in both backends.

The census in `q-suppression-on-user-functions` (Q3) predicted 10 ids:
- Only rdr.39 and 74.67 flipped.
- The other 8 stop at an earlier diagnostic (SB4024, SB4046, SB4072,
  SB5011, SB5046, SB4022).
- It missed 29: captures, `-` on void functions, calls inside bodies, and
  `- walk`.

The sweep was the authority. A census of spellings says where to look, not
what changes.

## What this does not settle

- **F6 units B and C.**
  - Unit B: a `-` on a call does not reach the body, or the captures among
    the call's arguments (Q1 and Q4, ruled 2026-10-29). supval.7, .8 and
    .18-.21 move with it. B76 is not a bug, and H234/H235 are dropped.
  - Unit C: B84 (55.88).
- **`q-nonvoid-classifier-residuals`** (P3, new):
  - Q1: `$(c)` on an int function binds the int, yet `$(- c 2>&1)` is
    legal;
  - Q2: an `-> int` with no `return`;
  - Q3: a redefinition that changes class;
  - Q4: a redirected `return g` counts as a status.
- **`q-status-function-ceremony`** (P3, filed 2026-10-30): SB4017 stays
  `-> int`-only until DG rules on it.
- **`bug-int-call-by-name-redefinition`** (P1, new).
  - `function_int_typed` is keyed by name, so its consumers resolve an int
    call by name, not by the definition in force. Probe i1 prints `one` on
    the real stdout, then `x=[0]`.
  - The consumers are untouched by F6: `func_call_capture`, `var_to_ssa`,
    `capture_args_run_value_calls`, `capture_func_into` and
    `capture_body_into`. So the 2026-10-26 rule backlogs the bug.
  - Caveat (decide-and-report): `register_functions`, which builds the
    table, was touched. The fix still belongs in the consumers, and Q3
    could reshape it.
- **`bug-function-name-read-as-variable`** (P2, new). It was found while
  checking a probe in which `int n=$(( f ))`, with a status f, gave 0.
  - A bare function name in arithmetic is read as a variable unless the
    function is int. `fn f { echo hi }; int n=$(( f ))` gives 0, and does
    not print `hi`.
  - `int f=3` and `str f=3` escape SB4019, because only
    `handle_assignment` checks for the collision.
  - `echo "v=$f"` prints `v=`. Validate merges function names into its
    known set, so SB5031 never fires.
  - All of it is pre-existing, the same on pre, and in untouched functions.

## Specs and gates

errors.sh gained nonvoid.1-nonvoid.29 (plan 676 → 705). nonvoid.1 is B15's
repro.

Negative controls ran on pre (44862797's binary), errors.sh, native.
- Exactly 31 specs are red: supval.1-.6, .9-.12, .14, .17 and .22, and
  nonvoid.1-.14, .16-.18 and .20.
- The controls nonvoid.15, .19 and .21-.24 pass there.
- C gave the same 31, plus rdr.27. That one is an artifact of running the
  two modes concurrently: rdr.25-.30 and .37 write fixed /tmp paths.
- For .25-.29, the negative control is the gate-1 binary (above).

Reversion stubs, each on gate 1's tree (native errors.sh; types.sh for
(a)):

| Stub | Disabled | Red ids |
|---|---|---|
| (a) | `check_function_capture` | supval.2-.6, .9-.12, .14; nonvoid.10-.13; constw.19 |
| (b) | `check_void_suppression` | supval.1, .17, .22; nonvoid.17, .18 |
| (c) | the status branch of `check_non_void_bare_call` | nonvoid.1-.8, .14 |
| (d) | closing over returned calls | nonvoid.7 |
| (e) | first definition wins | nonvoid.8, .19 |
| (f) | `.chain(producer_exits)` in `analyze_function_returns` | nonvoid.16, .20 |
| (g) | the SB4025 retain | nonvoid.15 |
| (h) | upfront int classification | nonvoid.9 |

(d), (e) and (h) were re-run on the final tree:
- `hands_back_result` returning false reddens nonvoid.7.
- `or_insert`, so that the first definition in force wins, reddens .8 and
  .19.
- Skipping `class_static` reddens .9 and .16. .16 is new, because SB4017
  now uses the static table.

Gates:
- Gate 1: `NEW=48 mk gate` passed. Rows 6523286-6533644, 10359 checks, 0
  flips.
- Gate 2, after the redefinition fix: `NEW=10 mk gate` passed. Rows
  6533645-6544013, 10369 = 10356 + 13 skip, 0 flips, 0 ghosts, liveness
  260, 233 s.

Unit A is the first of F6's three units. Its probes are in /tmp/port/f6,
and are archived at the F6 wrap-up. 12 of the 20 Order items are done, and
F6 is in progress.
