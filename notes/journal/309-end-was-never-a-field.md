---
title: END was never a field
---

# END was never a field

`ir::Program` has had an `end_block: Option<Vec<Statement>>` field for as
long as DD#56 has existed. Fourteen validator passes branch on it. The
`--emit-ir` capture renders it as `:end-block`. Nothing ever set it.

`parser.rs` bound it with a non-`mut` `let` to `None` and never came back.
`END { }` was parsed into a `Function` literally named `__sash_end__`,
pushed onto `program.statements`, and identified downstream by string
comparison on that name.

## How it surfaced

Branch coverage. `if let Some(ref end) = program.end_block` was uncovered at
twenty sites — 3.4% of every missed branch in the codebase concentrated in
one shape. My first instinct was that `if let` branch data was unreliable, so
I read both arms directly: `resolve.rs:60` arm 0 had **0** hits against 3632
for arm 1, while `codegen/entry.rs:397` — also an end-block test — had 11.
Codegen saw END. The validator never did.

`spec/signals.sh` has thirty-five passing DD#56 END tests, so "END is
untested" was not the explanation. `--emit-ir` gave the answer in one line:
`:end-block nil`, with the body sitting in `:statements` as a function.

The same investigation is what turned up the duplicate-diagnostic bug
(journal 308). Coverage did not find a gap in the tests. It found two lies
in the compiler.

## What the encoding cost

I filed this as vestigial and was wrong. DG's question — *should
`:end-block` show a call to `__sash_end__` when present and nil otherwise?*
— forced the re-read. The fourteen guards are not leftovers from a removed
design. They are a **complete consumer side**: every top-level pass that
takes a `Program` already handles an end block. It is the **producer** that
was never written. Half a feature, and the half that was there was the
harder half.

Four user-visible defects fell out of the name encoding, all of them
invisible until the field was interrogated:

1. **`:end-block nil` lied.** A `--extract ir` consumer reading the capture
   would conclude the program had no exit handler. DD#104's whole premise is
   that the capture is structural enough to be trusted by a consumer that
   lags the compiler.
2. **SB5071 leaked `fn __sash_end__` into user diagnostics.** Nesting an END
   inside a function produced a message about a function the user never
   wrote.
3. **Two END blocks were silently accepted, second wins.** DD#56 says
   "exactly one per program". Nothing enforced it; the first block simply
   never ran.
4. **A user function named `__sash_end__` compiled**, then died at runtime
   with `sash: bug: bare command passed to sash_exec (expected absolute
   path)` and exit 127.

A name is not a type. Every one of these is the cost of encoding a
structural fact as a string that something downstream has to recognise.

## The wiring

`parse()` now intercepts `Tok::End` at the top level before
`parse_statement` ever sees it, and assigns `program.end_block`. Two new
diagnostics enforce the DD#56 sentence directly: **SB4054** (an END block
must be at the top level) and **SB4055** (a second END block). Both were
previously unenforced. `transform/entry.rs` reads the field for
`state.exit_trap`; `transform/statements.rs` lost its `__sash_end__`
early-return; `validate.rs` stopped name-matching in `check_dq_function`.

## The scope change, and the pass it caught

END is now **top-level scope**, where the function encoding had made it
function scope. That is the correct reading of DD#56 — "END is a top-level
declaration" — and it is what the fourteen guards already assumed:
`check_no_shadow` calls `collect_top_level_decls(end, …)`.

It is still a behaviour change, so I ran a rejection battery: the same
program at top level and inside `END { }`, comparing the diagnostic sets.
One rule differed. `$@` was **accepted** inside END and rejected (SB5045)
outside it.

`check_dollar_at` walks `program.statements` and nothing else. It has no
end-block guard at all. While END was a function, the pass reached the body
with `in_fn = true` and treated `$@` as that function's arguments — wrong
for the wrong reason, but it did reject nothing. Moving the body out of
`statements` removed the pass's only path to it.

That is a lost rejection, which is the failure mode that actually matters
here: a compile error silently becoming a binary. So I audited all
thirty-five program-level passes in `validate.rs` mechanically rather than
by eye, classifying each as handling `end_block` directly, reaching it via
the `program_blocks` / `program_scopes` / `function_bodies` helpers, or not
at all. Seventeen had no direct reference; thirteen of those go through a
helper. `check_taint` looked missing and is not — `taint_top_level` uses
`program_blocks`. `slot_usage_map` and `check_dead_variables` delegate.

Exactly one pass was genuinely blind, and it is the one the battery found.
`check_dollar_at` now iterates `program_blocks(program)` at `in_fn = false`,
so the END body gets the top-level rejection and the top-level advice
(`$(argv 1)`, not `@args`).

The battery is now fourteen rules and all fourteen produce identical
diagnostics in both positions.

## Specs

- `signals.sh` 56.36 — nested END is SB4054, and the message does not
  mention `__sash_end__`
- `signals.sh` 56.37 — a second END is SB4055
- `signals.sh` 56.38 — `$@` inside END is SB5045, the regression guard
- `misc.sh` 104.5 — `:end-block` renders the body; `__sash_end__` appears
  nowhere in the capture

Sweep: 4585 passing, 10 skipped, 0 failing. All thirty-five pre-existing
DD#56 specs green in both modes.

## Left open

`:ir-version` is still 1. The `:end-block` shape did not change — the
field was always documented to hold the body — but its *inhabited* value is
new, and DD#104 says the stamp exists precisely so a consumer can refuse a
capture it does not understand. A consumer written against the old compiler
would have had no reason to write a `nil` branch, because in practice it
was the only value. Worth DG's call.

Separately, the scope change means `str x` at top level plus `str x` in END
is now accepted, where it used to be SB5049. That is not a new hole — it is
the *existing* top-level redeclaration hole, now reachable from one more
place. DG's open question ("should SB5049 forbid all redeclaration
regardless of scope?") would close it everywhere at once.
