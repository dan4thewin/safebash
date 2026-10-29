# 541 - A function defined in END is a function

2026-10-29. DG said "go" for F3, Order item 11 of the batch-531 port: B8
and B16, functions defined inside `END { }`. Every change makes such a
function behave like one defined at the top level. It was ported as the
branch wrote it, decide-and-report.

## The bugs

SB5071 admits a `fn` definition as a direct statement of the top level or
of END. Two passes looked at the top level only:
- **B8**, the transform. `register_functions` scanned `program.statements`,
  so `known_functions` and `function_int_typed` held top-level functions
  only. In END:
  - `(f)` read a variable named `f`, so `$(( (f) + 1 ))` printed 1 (h5,
    c11). A use before the definition did the same, silently (h9; b8g for
    a bare `f`).
  - A function used as a pipeline stage compiled to a bare exec and died at
    runtime with "bare command passed to sash_exec" (b8b).
  - The SB4019 collision between a variable `f` and an END function `f`
    compiled (b8d; its top-level twin b8e was rejected).
  - A top-level `(f)` with the definition in END compiled (h2), where the
    top-level twin h4 is SB5041.
- **B16**, binding resolution. `bindings::resolve` built its function set
  from the top level, so `h =a` between two END functions was not
  renamed, and the program drew a false SB5066 (b8r/r2).

A bare `f` after an END definition already worked: `lower_function_stmt`
inserted the function's `-> int` return type into `function_int_typed`
when it lowered the definition. That lazy insert was the only thing
covering END, and it covered one form.

## The port

The branch's fix, as written:
- `ir::Program::function_defs` yields every definition: the top level, then
  END. Its doc cites SB5071 as the reason the two lists are complete.
- `register_functions` (transform/entry.rs) and `bindings::resolve` iterate
  it.
- The lazy insert in `lower_function_stmt` is deleted, now that
  `register_functions` covers both bodies, and the function's doc says so.

The branch's hunk H199 also carries W2's half (`exec_fns =
exec_reaching_functions`, B114). That half is not ported; W2 re-implements
it.

Two scans stay private and say why in the shared-walkers sweep task:
`resolve.rs` `collect_functions` already recurses into END, and
`validate.rs` `collect_functions` runs before SB5071, so it must recurse.
The type checker's three top-level scans are B5 and land with the typing
family (Order item 19).

## Specs

Seven specs in signals.sh (plan 70 → 77), dual-mode, so 14 new ids:
- 56.94: `(f)` after an END definition gives 8.
- 56.95: an END function as a pipeline stage prints `hi`.
- 56.96: `f=1` plus an END function `f` is SB4019 at 1:1, asserted with its
  collision text (SB4019 has three sites).
- 56.97: a use before the definition inside END is SB5041 at 1:28.
- 56.98: a top-level `(f)` with the definition in END is SB5041 at 2:7.
- 56.99: `h =a` between END functions prints `main` and 7 (B16).
- 56.100: a bare `f` after its END definition gives 8. It passed before the
  port, through the lazy insert, and pins its replacement.

I left out c11 because it exercises 56.94's mechanism.

Negative control, the pre-port binary in both modes: TAP 71-76 (56.94-56.99)
fail, and 77 (56.100) passes, as the control should.

Reversion stubs, each restored by copy, verified with `cmp` and rebuilt:
- (a) `register_functions` scans the top level only (the lazy insert stays
  deleted): 56.94-56.98 and 56.100 fail;
- (b) `bindings::resolve` scans the top level only: 56.99 alone fails;
- the same result in both modes.

So every new spec is pinned by at least one stub. 56.100 is pinned by (a)
alone, which is the point of it.

## The gate

`NEW=14 TOTAL=10201 mk gate`: GATE PASS.
- `mk test`: 237 units.
- Static checks: all clean.
- Sweep rowids 6461662-6471862: 10201 rows, 5211 native + 4990 C.
- 0 failing, 13 skip, 0 flips, 14 first-ever, 0 retired, 0 ghosts,
  liveness 260.

## The SB5041 text

The SB5041 message reads "'f' is used as a command before this definition
… (rename, or invoke the external via an absolute path)". For an arithmetic
use, there was no command and there is no external; the fix is to move the
definition up. F3 touches `lower_function_stmt`, which emits the text, so
the 2026-10-26 rule (fix findings in touched functions) applied. But the
emission site cannot tell what kind of use came first:
`VersionState::externally_used` is a set of names filled by four producers,
one per kind of use. A neutral rewording at the one site would drop the
absolute-path advice, which the command case needs and functions.sh pins.
The real fix is in the producers, which F3 does not touch. So it was filed
as `bug-sb5041-text-for-arithmetic-uses` (P3), decide-and-report. The text
is the same on the pre-port binary for a top-level arithmetic use (h4), so
F3 did not introduce it. 56.97 and 56.98 assert today's text and move with
the fix.

## Backlog

- `q-fn-definitions-in-end` (P4 question): should SB5071 reject a definition
  in END? This is the plan's simplification candidate, with what (b)
  removes and costs.
- `bug-sb5041-text-for-arithmetic-uses` (P3), above.
- `sweep-private-duplicates-of-shared-walkers`: a section on
  `function_defs` and the scans it could replace.

## Narrative

DD#56 gained "A function defined in END is a function everywhere"; DD#81 a
note that Part A's execution order includes END. Ledger rows H011, H068,
H199 (F3 half) and H218 are `ported`; findings B8 and B16 are ticked.
11 of the 20 Order items are done. Next is F5 (item 12), on DG's go.
