# 530 - The prefix owns what it emits

2026-10-20. DG continued coverage from `932831c`: 63 cold compiler
production lines. The selected cohort was all 17 remaining lines in
`transform/expr_handlers.rs`, across 15 functions. Reading their callers
exposed DD#55 suppression defects in both backends. Coverage paused while
those were fixed, then resumed and closed the cohort.

A historical task claimed the empty-capture arity backstop protected
"DG's `--bypass` switches". DG: *"I'm only aware of taint bypass switches.
Why would we ever want arity bypass?"* There is no structural bypass.
`--no-taint` (DD#82) disables taint checking only, and `--bypass` is not
an option. The backstop became a loud refusal (below). The task file
carries a dated correction.

## Suppression was lost, and it leaked

The capture paths treated `- f` differently depending on where it
appeared. The preserved baseline binary demonstrated each class in native.
The regression specs fail the same way in C:

- **Lost.** `- speak` still aborted on a `/bin/false` inside the
  function's `if` branch. So did `str x=$(- speak)`, a failing argument
  capture inside a suppressed call (`$(- use $(fail))`), a redirected
  capture (`$(- speak 2>&1)`) and a suppressed self-tail return. DD#55's
  prefix covers the complete operation; each of these checked part of it.
- **Leaked.** `int n=$(count)` (a branched body whose `/bin/false` must
  abort) followed by `- safe` printed `hello` and `7`. The later prefix
  unchecked blocks belonging to the earlier, unprefixed capture. A failure
  was ignored with no `-` anywhere on its operation. This is the worst of
  the three, because nothing in the source asks for it.
- **Rejected.** `int n=$(- count)` with an `-> int` function drew SB4030:
  the capture was typed as a string. `$(- toupper $(fail))`,
  `$(- strlen $(fail))`, an available `$(- status)` and
  `if x=$(- getenv NAME)` drew SB5073. The prefix hid the sole value
  command, so the intrinsic looked bare.

Programs still rejected now get the diagnosis that applies to them.
`$(- strlen)` and `$(- getpid extra)` draw SB4045 rather than SB5073.
`$(- status)` with no preceding command, or after `- cmd`, draws SB5012.
`$(- toupper token; echo trailing)` draws SB5073's specific "not one
statement among several" message.

`errors.sh` gains `supval.1`-`supval.35`. The original compiler fails
**28** of them in each mode, and passes all 430 existing checks. The
seven that already passed are controls: the checked first tail iteration,
unsuppressed recursion, a later bare failure, a nested-branch abort, a bare
value intrinsic, SB4025 and `wait`'s arity.

## The scope records its region

There were two causes. First, materializing a captured named function
discarded its `Suppressed` wrapper. Second, suppression inferred its
region: the starting block's new suffix plus blocks
`start + 1 ..= returned_block`. An inlined call allocates its return
block *before* its body blocks, so that range was wrong both ways. Body
blocks allocated after the return block were missed (lost). Older blocks
whose IDs fell inside the range, such as an earlier capture's branch, were
unchecked (leaked).

`statements::SuppressionScope` records the region instead. On entry it
notes the starting block, that block's instruction count and the program's
block count. `finish` unchecks the starting block's new suffix and every
block created since entry. It never infers ownership from a returned
block ID. Ordinary suppressed statements, suppressed captures, the tail
specialization and capture-condition bodies all use it. The state's
suppression counter alone is not enough: ordinary command lowering still
emits `checked = true`.

## The value survives the prefix

`ir::single_value_command` is now the one classifier for a value-bearing
capture. It requires exactly one statement and peels `Suppressed`
transitively. Redirects decline, because they need ordinary body capture.
Suppression changes checking, never the value or its type.

Its consumers are the capture type (`command_substitution_type`),
`intrinsic_call_name`, the transform's `single_command_subst`, and the
validator's arity, splat-operand and intrinsic-read discovery. The
validator's private `single_cmdsub_command` is deleted.
`check_cmd_at` recurses through `Suppressed` with the **same** command
position. A sole suppressed value intrinsic keeps its substitution
position; bare and multi-statement intrinsics remain SB5073.

The old type rule used a capture's first bare statement. So
`str s=$(count; echo token)` drew "cannot assign int value to string
variable". Now only a sole value command carries a value type, and every
multi-statement capture is stdout, a string. That form is still rejected:
SB4016 requires the discarded int call to be captured or prefixed.
`supval.27` pins the corrected type.

The transform reuses the existing `CmdSubstBody` rather than adding an
SSA variant. `suppressed_capture_value` rebuilds the unwrapped
substitution and lowers it through `expr_to_value`, so functions,
intrinsics and integer values take their ordinary paths.
`lift_int_func_calls` lowers it inside a suppression scope, including
nested argument evaluation.

A suppressed `getenv` condition keeps DD#101's value lookup inside the
scope. Sending it to ordinary body capture instead would make `getenv` an
unrecognized bare command. Capturing a user function in a condition
remains SB4025.

## A suppressed tail call is a second loop

`return $(- descend $next)` must stay constant-stack (DD#27). The initial
invocation is checked, though: only work from the prefixed call onward is
suppressed. Unchecking the loop body retroactively would suppress the first
iteration too.

When a tail call moves from checked to suppressed, lowering enters a
scope, lifts the arguments, and inlines **one suppressed specialization**,
returning its result. That specialization's own tail calls reuse its
unchecked loop header. Already-suppressed tail calls remain ordinary
back-edges. There is no runtime flag, call stack or new SSA variant.
`inline_int_func` takes a private flag for that one transition, so the
duplicate name does not trip the recursion diagnostic. Other recursion,
including a collection-bearing self-tail, keeps SB5037.

`supval.10` keeps the first iteration checked. `.9` and `.12` suppress
later iterations, `.14` suppresses a nested tail argument and `.13` keeps
unsuppressed recursion checked. `.11` runs a prefixed 10000-step loop to
completion.

## The expression-handler cohort

Two lines were source gaps, now pinned by `arith_literals.sh`: constant
unary plus (`exprcv.1`) and a numeric operand to arithmetic `strlen`
(`exprcv.2`). Both passed against the original compiler in both modes;
they are coverage, not fixes.

The rest were internal contracts:

- **Intrinsic counts.** One `require_intrinsic_arity` consults
  `value_intrinsic_arity` before capture and arithmetic resolution,
  including `wait`. It refuses loudly. The successful empty-capture
  fallback and each builder's count decline are gone. SB4045 remains the
  source rejection, including under `--no-taint` (`supval.29`/`.30`).
- **Command heads.** `literal_command_head` is shared by capture and argv
  construction, and refuses a dynamic internal head. SB5011 rejects
  nonliteral heads; the parser normalizes permitted whole quoted paths,
  including paths with spaces.
- **Arithmetic.** A default must be a literal, valid DD#72 number, and is
  canonicalized to decimal. A comma sequence needs at least two operands:
  the parser produces only those, and mutation hoisting preserves length.
  `arith_to_ssa` is one exhaustive match over leaves and operators;
  `arith_leaf_to_ssa` and its unreachable duplicate dispatch are deleted.
- **Join.** The guard was `args.len() >= 2`, so extras were silently
  dropped. It now accepts exactly one or two arguments; SB5040 remains the
  source rule.

Six direct-unit controls failed before implementation, each because a
malformed shape produced an answer rather than a refusal. Five new units
and the extended join unit pin them: compiler units **220 -> 225**. These
are direct calls with test-built IR, not source coverage or a supported
API.

## Complete-scope verification

Expression handlers close **17 -> 0**. Compiler production cold lines fall
**63 -> 46 / 30739**: type checking 14, validation 32, no cold-file
regressions. All **42** measured source blobs are current. Raw
cargo-llvm-cov, including test scaffolding: **99.30% line / 99.57%
function / 95.02% branch**, regions 98.97%.

Exactly **9196 CLI profiles across 32 slots** separate source execution
from direct-unit reachability:

| Site | CLI | Union |
|------|----:|------:|
| Literal command-head refusal | 0 | 1 |
| Nonliteral arithmetic default refusal | 0 | 1 |
| Join count refusal | 0 | 2 |
| Arithmetic default canonicalization | 108 | 109 |
| Constant unary plus | 6 | 6 |
| Numeric intrinsic operand | 20 | 20 |
| Suppressed capture scope finish | 56 | 56 |
| Checked-to-suppressed tail transition | 8 | 8 |
| Native capture-condition dispatch | 4 | 4 |
| Intrinsic count refusals (non-returning calls) | 0 | 38 |
| Comma refusals (non-returning calls) | 0 | 2 |

An `assert!` message line carries no DA record. A first attribution
targeted message text, read 0/0 and failed. The count and comma rows
instead subtract successful returns from function entries, which counts
refusals without calling message lines cold.

The instrumented sweep matches its prediction exactly: **9333 = 9320
passing + 13 existing C skips**, native **4769**, C **4564**. All **9259**
baseline statuses at **6248999-6258257** are preserved. Exactly **74**
intended first-ever mode IDs appear (`supval.1`-`.35`, `exprcv.1`/`.2`).
No unexpected additions, retirements or previous-row flips; liveness
remains **260**. The block is **6258258-6267590**. Source plans and both
global ID collision checks agree. The CLI/union exports, merged profiles
and database were archived before ordinary restoration.

## Ordinary publication gates

`mk cov-reset` rebuilt normally; the Go `mk` deadlock did not recur.
Serial `mk -p 1 test` passes **225 compiler + 27 converter + 20 expansion
units**.

The first ordinary sweep never finished. Its xtrace ends in `Terminated`
inside a C-mode gcc link (`var_modifiers.sh`, check 86 of 168), with no
`not ok`. Something outside killed it; the likeliest cause is the tool
session ending. `run.sh` records only at the end, so no rows were written.
The rerun produced exactly **9333**
checks at **6267591-6276923**, with the same 9320 passing and 13 skips. The
exact gate passes with zero first-ever IDs, no flips or retirements, and
liveness 260.

The no-profile canary's first program was itself invalid: it declared
`int n` globally and inside `descend`, which SB5049 (DD#86) correctly
rejects. With the global renamed, native and C print `7 TOKEN` /
`present` with empty runtime stderr. The canary covers a branched nested
suppression, the typed tail return and the suppressed `getenv`
condition. Default and explicit profile destinations stay empty, and the
target profile pool is unchanged. Freshness still certifies all 42 blobs.

GUIDE records the suppression region and the finite tail specialization.
DD#55 gains a dated correction: suppression scopes the complete call, and
leaves values, types and validation intact.
