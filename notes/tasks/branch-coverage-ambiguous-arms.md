title: "Branch-coverage leads the counts could not settle"
status: open
priority: P3
depends:
gates:

# Branch-coverage leads the counts could not settle

Three zero-hit arms found during the 85.34% measurement (journal 320) that
resisted diagnosis from `coverage.db` alone. Each needs a probe or an
instrumented build, not more reading.

## 1. `parse/parser.rs:2669` — branch index ambiguity

`is_identifier_or_subscript` has two `let`-`else` arms. `coverage.db` reports
line 2669 block 1 branch 3 as unvisited, which reads as "the word contains `[`
but does not end in `]`" never happening. But `aw.20` (`a]b[=1`) takes exactly
that path, and it passes.

Either the branch indices on a line carrying two `let`-`else` arms do not map
the way the rest of the file's do, or one of the two probes reaches the
callsite by a different route. `aw.27`/`aw.28` were added for this arm
regardless; re-measure and see whether the count moves. If it does not, the
branch-index mapping itself is the finding and `contrib/covdb.py` may be
mis-attributing.

## 2. `codegen/entry.rs:478` — all three artifacts always `Some`

`embed_artifacts` loops over `.sash_source`, `.sash_ir` and `.sash_manifest`
and skips any that is `None`. The `continue` never runs: 4302 hits on the
`Some` arm, which is 1434 compiles times exactly three. So all three are
always present, even though `--manifest` is opt-in.

Worth knowing *why* before touching it. If `Embedded` always populates all
three regardless of flags, the flag is doing less than it appears, and that is
a bigger finding than one branch.

## 3. `parse/lexer.rs:1399` — a loop bound, not dead code

`consume_dq_cmd_sub`'s `while self.pos < self.source.len() && depth > 0` never
exits via the first condition. An unterminated `"$( ` is caught by SB4057
(unterminated `"`) before this loop can run to EOF.

**Do not remove it.** Without the bound, `step_dq_cmd_sub` would index past
the end and panic. It is a necessary guard whose false exit is foreclosed
elsewhere — the loop-bound analogue of the cross-pass `let`-`else` arms. The
only open question is whether it deserves a comment saying so, which is cheap
and probably yes.
