title: "Bug: the validator taints a `while readline` loop fed by a clean pipe, though the SSA backstop binds it clean"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "l11, l17 and l7 compile with no `:!` and print what l12 and l5 print, in both backends"
  - "l13, l14 and l4 keep SB5020"
  - "the validator and the SSA backstop decide a pipe reader's taint from the same upstream stages, whether the reader is a bare stage or a `while` condition"
  - "a reader inside a capture, a function body, a redirected command or a background child within the loop does not inherit the loop's clean pipe (the suspension rule `state.pipe_reader` follows in the transform)"
  - "specs pin each row in both modes; l15 is pinned when B35 lands too"

## Found

2026-10-27, while porting F1+F4 (journal 537). It is **pre-existing**: every
result is the same at `8e28c61` (before F1+F4) and with F1+F4, in both
backends. A bare reader stage fed by a clean pipe binds a clean value; the
same reader as a `while` condition does not.

| Id | Program | Result |
|---|---|---|
| l12 | `echo hi \| readline x` then `/bin/echo "$x"` | `hi` |
| l11 | `echo hi \| while readline x {` then `/bin/echo "$x"` then `}` | SB5020 2:13 "tainted variable $x used without attest" |
| l17 | l11 with `\| - while readline x` | SB5020 2:13, the same |
| l5 | `y=$(/bin/cat f)` then `echo "${y:!}" \| readline x` then `/bin/echo "$x"` | the file's contents |
| l7 | l5 with `while readline x { /bin/echo "$x" }` | SB5020 3:13 |
| l10 | l7 reading `"${x:!}"` | the file's contents |

The controls are right: an external producer taints the reader.

| Id | Program | Result |
|---|---|---|
| l13 | `/bin/echo hi \| readline x` then `/bin/echo "$x"` | SB5020 2:11 |
| l14 | `/bin/echo hi \| while readline x { /bin/echo "$x" }` | SB5020 2:13 |
| l4 | `/bin/cat f \| readline x` then `/bin/echo "$x"` | SB5020 2:11 |

DD#24 (clarification 2026-05-24) decides a pipe reader's taint by whether
the pipe's source execs; `echo` does not. The only way to use l11's value
today is `:!` (l10), which declassifies it, so a clean program has to be
written as though its data were tainted.

A user-function source is a separate finding: `fn f { echo a }` then
`f | readline x` is SB5020 too (l16), against the same clarification. That
is batch-531 **B35**, which lands with the W2 step (T1, a function's stdout)
in `batch-531-findings.md`. l15 (`f | while readline x { … }`) needs both
fixes.

Probes: `files/port-f1f4-probes.tar.gz`, directories `lw/` (l1–l17),
`bl/` (l12, l13) and `fin2/` (l14).

## Cause (read, not yet confirmed by a fix)

`taint_pipeline` (sash/src/validate.rs) exempts a stage's reader when no
stage upstream of it execs, by calling `taint_stmt_no_read_taint`. That
function recognises a reader command beneath the stage's `-`/`!` marks and
sends every other stage to `taint_stmt`. A `while` stage goes on to
`taint_while`, which walks the condition with `taint_block`, and there
`readline x` taints `x` like any read from stdin.

The SSA backstop gets it right. `handle_pipe_while_or_array_read`
(sash/src/transform/stmt_handlers.rs) computes `clean_pipe` over the
upstream stages, and `lower_pipe_consumer` hands it to the condition's
reader through `state.pipe_reader`. So the validator is stricter than the
backstop: a false rejection, not a hole. `is_external_source`'s doc says the
two layers must classify a stage identically.

F1+F4 modified `taint_stmt_no_read_taint` (B31), but the exemption has to
reach a reader inside `taint_while`'s condition, through `taint_while`,
`taint_block` and `loop_entry_fixpoint`, which F1+F4 did not modify. A copy
of `taint_while`'s fixpoint inside `taint_stmt_no_read_taint` would fork a
shared walker. So this goes to the backlog under DG's rule for the port (fix
a known bug in a function the port modifies; file the others).

## Fix direction

Give the validator walk a clean-pipe context, as the transform has
`state.pipe_reader`. `taint_pipeline` sets it for the final stage when no
upstream stage execs; a reader in that stage's `while` condition consults it
instead of tainting its target. The context is suspended wherever the
transform suspends `pipe_reader` (functions, captures, redirected commands,
background children), so only the loop's own condition reader sees it.
