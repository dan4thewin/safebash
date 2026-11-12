title: "Bug: reserved and environment names escape SB5017/SB5018 outside a word — arithmetic reads them as empty or 0, and `len HOME` ICEs (native) or reads a string as a table (C)"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "every program in the first two tables is a compile error in both backends, with the rule the word form gives: SB5017 for `RANDOM`, `SECONDS`, `IFS`, `PIPESTATUS`; SB5018 for `HOME`, `PATH`; the `$0` message for `$0` and `${0}`"
  - "the subscripted reads (`${HOME[0]}`, `$(( HOME[0] ))`) get the same diagnostic; no ICE in either backend"
  - "`len`, `join` and `keys` with such an operand get the same diagnostic"
  - "every program under Correctly rejected today keeps its diagnostic"
  - "the old rejections are a subset of the new: a temporary sentinel, proved able to fire, finds no program the old check rejects and the new one accepts"
  - "a spec pins each read channel in both backends; each fails on the pre-fix binary"

## What

These compile, and the binary runs with rc 0, in both backends:

| Program | Output |
|---|---|
| `echo "[$(( RANDOM ))]"` | `[]` |
| `echo "[$(( $RANDOM ))]"` | `[]` |
| `echo "[$(( ${RANDOM} ))]"` | `[]` |
| `echo "[$(( $HOME ))]"` | `[]` |
| `echo "[$(( SECONDS ))]"` | `[]` |
| `echo "[$(( ${0} ))]"` | `[]` |
| `echo "[$(( HOME + 1 ))]"` | `[1]` |
| `echo "[$(( RANDOM + 0 ))]"` | `[0]` |
| `echo "[$(( $0 + 1 ))]"` | `[1]` |
| `if (( $RANDOM > 0 )) { echo pos } else { echo zero }` | `zero` |
| `fn f { echo "[$(( RANDOM ))]" }; f` | `[]` |
| `echo "[$(( IFS ))] [$(( PIPESTATUS ))]"` | `[] []` |

A lone operand prints the empty string; inside a larger expression the
name reads as 0.

`len` of the same names:

| Program | native | C |
|---|---|---|
| `n=$(len HOME); echo "[$n]"` | compiler ICE "undefined arrlen: HOME" | compiles; prints a garbage count, `[4294939591]` |
| `int n=$(len RANDOM); echo "$n"` | compiler ICE | compiles; prints a garbage count |
| `int n=$(len SECONDS); echo "[$n]"` | compiler ICE | compiles; prints a garbage count |

Subscripted reads ICE in both backends: `echo "[${HOME[0]}]"`,
`echo "[$(( HOME[0] ))]"` and `echo "[${SECONDS[0]}]"`.
- native: compile rc 1, "ICE: undefined array: HOME";
- C: rc 101, "ICE: collection `HOME` has no emitted SSA slot".

`join` and `keys` are rejected, but by a type rule rather than the name
rule. `for k in $(keys PATH) { echo "$k" }` is SB4030 at 1:12, "`keys`
needs a map — `PATH` is not a map". `echo "$(join HOME ,)"` is SB4030 at 1:9.

Correctly rejected today, to be kept:
- `echo "[$RANDOM]"`: SB5017 at 1:6;
- `echo "[$HOME]"`: SB5018;
- `echo "[${0}]"`: SB5017;
- `echo "[$(( foo ))]"`: SB5031;
- `echo "[$(( ${HOME:-3} ))]"`: SB5018;
- `array a=(x y); echo "[${a[$RANDOM]}]"`: SB5017 at 2:6;
- `RANDOM=4; echo "[$RANDOM]"`: SB5017 at 2:6;
- `n=$(len foo)`: SB5039, "`len foo` — `foo` is introduced nowhere";
- `${#HOME}`, in a word or in arithmetic: SB4038.

## Cause

`validate_var_name` (`sash/src/validate.rs`) owns three rejections:
- REJECTED_VARS (SB5017);
- REJECTED_POSITIONAL (`$0`: "$0 is not available in sash — use $(arg 0)");
- ENV_VARS (SB5018).

Only `validate_expr` calls it, for a `SimpleExpansion` or
`ParameterExpansion`. An arithmetic operand is neither, and neither is the
operand of `len`, `join` or `keys`.

`handled_elsewhere` (`sash/src/validate.rs`) exempts the same names from
two other checks, "so the typo check never double-diagnoses":
- the undeclared-read check, SB5031 (`check_undeclared_reads`), which is
  the check that catches `$(( foo ))`;
- `flag_len_cmdsub`'s SB5039.

Each check defers to a rule that never sees these channels, so nothing
fires.

Lowering then reads a name nothing binds:
- `arith_to_ssa` (`sash/src/transform/expr_handlers.rs`) lowers `Var`
  through `positional_to_value(..).unwrap_or_else(|| var_to_ssa(..))`, and
  `DollarVar` through `.unwrap_or_else(|| SsaValue::Var(state.lookup(name)))`.
  Both backends treat the unbound version as empty. The C declares
  `const char* RANDOM_ptr = ""; i64 RANDOM_len = 0;`.
- For `len`, native's `find_array_slot` (`sash/src/codegen/types.rs`) has
  no slot and ICEs. The C declares `HOME_ptr` as `""` in the same way.
  `lower_value_as_int`'s `arrlen` arm (`sash/src/emit_c.rs`) gets
  `HOME_ptr` from `lookup_table_var` and passes it to `sash_table_len` as a
  `SashTable*`. The program reads the bytes of a string literal as a table.
  That is undefined behaviour; today it yields a garbage count. (Related,
  but not the same code path: `emit-c-var-slot-fallback-asymmetry`.)

This is the second instance of this shape. Journal 152 found the first:
braced `${RANDOM}`/`${HOME}` in a word escaped the same rule.
`bug-braced-rejected-var-bypass` fixed that for words.

## Fix (proposed, not yet ruled)

Reject the names in the one walk that already visits every read channel,
rather than adding channels one at a time:
- Add `SinkMode::Rejected(&mut Vec<Diagnostic>)` to `ReadSink`
  (`sash/src/validate.rs`). Its `record_param` and `record_splat` call
  `validate_var_name`.
- Add one pass, `check_rejected_reads(program)`, to the structural batch.
  It runs on source names, before bindings, and uses `check_reads_in_scope`
  over top level and END, with an empty `known` set and the real function
  table.
- Add a sink method for attest collection names. Add a span-carrying
  operand visitor for `len`, `join` and `keys`, shared with `collect_reads`.
- Remove `validate_expr`'s two calls to `validate_var_name`.
- Update the docs of `handled_elsewhere`, `classify_read` and `ReadSink`.
- Do not name the new mode "Reserved". DD#125 already uses that word, for
  `ReadVerdict::Reserved` (SB5093).

To verify, keep the old calls and the new pass side by side at first. Add
a temporary sentinel that reports any program the old check rejects and
the new one accepts. Prove it fires on a positive control (stub the new
pass for one name), then run the full sweep with it.

Follow-up: once this task, `bug-digit-led-names` and
`bug-multi-digit-positionals` land, no unbound name should reach
arithmetic lowering. Consider an ICE in `arith_to_ssa`'s unbound fallback.

## Open question

Should writes and declarations of these names be rejected too? These
compile today:

| Program | Output |
|---|---|
| `int RANDOM=4; echo "[$(( RANDOM ))]"` | `[4]` |
| `int HOME=3; echo "[$(( HOME ))]"` | `[3]` |
| `(( SECONDS++ )); echo done` | `done` |
| `RANDOM=4` | compiles; only the later word read is rejected |

DD79 Part 2 explains why `(( SECONDS++ ))` is not SB5031: an assignment in
arithmetic binds its name ("anything you assign exists"). That part governs
SB5031 only. It does not say whether a reserved or environment name may be
bound.

Found during the batch-531 port (step F14): findings B40 and B41, and the
`$0` / `${0}` rows of finding B38. Filed under DG's rule for the port that
questions and uncovered cases become backlog tasks.
