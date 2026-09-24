# 394 — The lookup that never succeeded

`transform/capture_purity.rs::command_is_pure` had eight cold lines, and a
comment explaining why seven of them were cold:

> Measured (journal 365): this lookup never succeeds in a full sweep — 156
> calls reach here and every one falls through to the `true` below.

That is a measurement, it was honest, and it was taken correctly. It is also
the single most dangerous kind of comment to leave in a codebase, because
what it actually says is *"this suite has no test for this"* while sounding
like *"this code does not run"*. Anyone auditing for dead code — which DG and
I have been doing deliberately for several sessions — reads the second
sentence.

## What the walk is for

The module analyses a `$( … )` site to decide whether its output can be
collected in a userspace buffer (cheap) or needs a kernel object (correct
when anything forks with our stdout). The disqualifying condition is a fork
that inherits fd 1.

`command_is_pure` reads the command head, rejects anything with a slash (DD#23
means a slash is an external), and then — the cold part — looks the name up in
`function_bodies` and recurses into that body.

The old comment said inlining runs first, so the lookup can't fire. That is
true of the captured function's *own* body: `$(emit)` where
`fn emit { /bin/cat f; }` arrives with the slash already there and dies on the
slash test. But it is not true one level down. `capture_func_into` does this:

```rust
let body = state.function_bodies.get(fname).cloned()?;
…
let buffered = super::capture_purity::capture_body_is_pure(&body, &state.function_bodies);
…
let exit_block = lower_stmts(&body, ssa, state, call_block);
```

The body is handed to the analysis **unlowered** — inlining happens at
`lower_stmts`, three lines later. So a capture of a function that *bare-calls*
another function reaches the walk:

```sh
fn inner { /bin/echo hi }
fn outer { inner }
str t=$(outer)
```

No spec in the suite had that shape. Not "it can't happen" — nobody wrote it.

## Answering by execution, including the part I got wrong

The observable is `--emit-c`: `sash_capture_start_buf()` for a buffered site,
`sash_capture_start()` for a memfd one. The in-process variant
(`fn inner { echo hi }`) emits the buffered form; the external one emits the
memfd form. So the walk runs and is decisive.

Then I nearly shipped a vacuous test. My first version ended `echo "$t"`, and
under a *wrong* purity decision that prints:

```
hi        ← the child's bytes, leaked straight to the real fd 1
          ← the capture came back empty
```

which the harness's own `out=$(…)` strips back to `hi` — byte-identical to
the correct result. cap.18 carries a comment about exactly this trap from the
other direction, which is the only reason I looked.

`echo "[$t]"` makes the leak positional, and I checked it rather than assuming
it: I took the emitted C for the impure program, hand-patched the two capture
calls onto the buffered path, linked it against `libsash_rt.a`, and ran both.

```
correct    : [hi]
forced-buf : hi
             []
```

Distinguishable. So cap.20 fails if the walk is removed — which cap.19 does
**not**, and that asymmetry is worth stating plainly. cap.19's program is pure
either way: if the lookup failed it would fall through to the trailing `true`
and still be buffered, still print `[hi]`. cap.19 covers the lines; cap.20 is
what makes them defended. A coverage test and a regression shield are not the
same artifact, and I have been sloppy about saying so.

Three specs: cap.19 (bare call, pure), cap.20 (bare call, disqualified — load
bearing), cap.21 (the module doc's headline case — a body that *consumes*
external output and yields it with `echo` stays pure, because the inner `$( )`
hands its child a pipe of its own).

## The other two cold lines are backstops, and I proved it

- **The non-literal head** (`_ => return false`): SB5011 rejects every
  spelling — a quoted name `$("echo" hi)`, a bare variable `$($c hi)`, a
  braced expansion `$(${cmd} hi)`. All three checked.
- **The cycle guard** (`if !visiting.insert(…)`): SB5037 rejects mutual
  recursion, checked with `fn a { b }` / `fn b { a }`.

Both now name the code that guarantees them. That is the distinction DG asked
about a few sessions ago — a shield is worth keeping when a *validator*
guarantees unreachability and the cost of being wrong is silent
miscompilation rather than a crash. Here the cost of guessing is a capture
that mis-buffers, which is a wrong answer, not an error. Keep both.

## A harness bug fell out of it

Running `bash misc.sh` standalone, test 6 (`104.5`) failed. It had nothing to
do with my change, and it is recorded `passing` in spec.db.

`misc.sh` defines its own helper:

```sh
run_sash_emit_ir() {
    …
    sash --emit-ir .input.sash > out 2> err
```

A bare `sash`. Every other helper in the suite uses `${SASH:-sash}`. Under
`run.sh` it works by accident, because `run.sh` prepends `target/release` to
PATH. Standalone it runs `~/.local/bin/sash` — and mine is stale enough to
still emit `:end-block nil` and a synthetic `__sash_end__` function, which is
*precisely the regression 104.5 exists to catch*.

So the test did its job perfectly on a binary that was not under test. Eight
tests were affected. AGENTS.md already warns that a standalone run picks up
whatever is on PATH and tells you to pass `SASH=` explicitly — that advice
silently did not work for these eight. Fixed to `${SASH:-sash}`, and given the
`timeout`/`</dev/null` discipline the other helpers have, since an `--emit-ir`
hang would otherwise hang the sweep forever.

## Gates

`mk test` 51/27/20. Sweep **6141**, predicted +6 (three specs in a dual-mode
file), 0 failing, 0 flips against `265b5dd`.

## The lesson

A measurement of coverage is a fact about the *suite*. Writing it into a
comment next to the code converts it, silently and without anyone deciding to,
into a claim about the *code*. Journal 386 already recorded the inverse
mistake — a `panic!` and a green sweep proving "not reached by these inputs",
not "unreachable". This is the same error wearing a comment instead of an
assertion.

If a cold line is cold because the suite has a hole, the comment should say
*the suite has a hole*, and the fix is a spec, not a note.
