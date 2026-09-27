# 425 — The oracle cannot see a bug both backends share

`native-redirect-diag-after-restore` was filed as one bug in one backend: a
redirect setup-failure diagnostic that native wrote *after* putting the fds
back, so it landed on the terminal where bash puts it in the capture file.
Fixing it turned up a second copy of the same inversion in the C backend —
and the reason nobody had noticed the second one is the interesting part.

## The bug as filed

Bash performs redirections left to right and writes the setup-failure
diagnostic through the ones that have already succeeded:

```sh
/bin/echo hi 2> capt.txt > /nonexistent_d/bad
```

| | stderr | `capt.txt` |
|---|---|---|
| bash | empty | `bash: /nonexistent_d/bad: No such file or directory` |
| C backend | empty | `./cbin: …` |
| native | `./q.bin: …` | empty |

`emit_redirects_guarded` already stops at the first failed open, so at the
fail block every earlier redirect *is* in effect. Native simply reported too
late:

```rust
emit_restore(saved.clone(), …);                           // fds put back
if in_condition || emit_open_failure_abort(&rf, …) {      // …then the message
```

`emit_open_failure_abort` did three things in one — wrote the message, set
the failing status, and terminated the block — so the restore could not just
be moved after it without landing on an unreachable edge. I split it into
`emit_open_failure_msg` and `emit_open_failure_terminate`, and the restore now
sits literally between the two calls at both native sites. The flat form was
chosen over nesting precisely so the three-step order is visible in the
source:

```rust
if !in_condition {
    emit_open_failure_msg(&rf, builder, module, ctx);
}
emit_restore(saved.clone(), builder, module, ctx.externs);
if in_condition || emit_open_failure_terminate(builder, module, ctx) {
```

## The second bug, and why the oracle was blind to it

The task file had a one-line "also check" note about `emit_redirect_push`, the
function-redirect *scope* form. Bash treats it identically:

```sh
f() { echo hi; }
f 2> capt.txt > /nonexistent_d/bad     # bash: message goes in capt.txt
```

Native had the same inversion there, and I fixed it the same way. Then the
differential run reported the C backend failing that row.

**C had the bug too — in `emit_redirect_push` only.** Its
`emit_redirect_guarded_body` had the order right all along, which is why the
command form matched bash. So `emit_c.rs` contained two functions doing the
same job in opposite orders, and the wrong one was paired against a native
function that was *also* wrong.

That is the finding worth keeping. The `--emit-c` differential oracle is the
project's strongest tool, but **it compares the backends to each other**. A
mistake made once in a shared understanding of the semantics gets made twice,
and two identical wrong answers are indistinguishable from two right ones.
The oracle localises a bug to one backend; it cannot see a bug in the
*specification both backends were written from*. Only bash can, and bash is
only consulted when someone writes the differential by hand.

The command form was caught (journal 404, probing something else) exactly
because the two backends *disagreed* there. The scope form sat unnoticed for
however long it has existed, one line away, because they agreed.

## The specs, and a control I nearly left out

`rd.5`–`rd.8` in `errors.sh`. The task file warned about the trap and it was
a real one: in the *correct* case `$rerr` is empty — and `$rerr` is also empty
in the *buggy* case for the file, so an assertion resting on `-z $rerr` alone
passes either way. Every check pins both the empty stderr and the file's
contents.

`rd.6` is the control, and it is the one that makes `rd.5` mean something:
with the failing open *first*, the `2>` never runs (rd.2's ordering guard), so
the message correctly goes to the real stderr and the capture file is never
created. Without it, `rd.5` could be satisfied by a compiler that always wrote
to the last-named file.

`rd.8` covers a dup rather than an open (`2>&1 1> /bad`) to pin that the rule
is about the fds in effect, not about files.

## The neuter proof, which turned out to be a diagnosis

Reverting each fix separately:

| neuter | fails |
|---|---|
| native msg/restore order | rd.5, rd.7, rd.8 |
| C scope msg/restore order | rd.7 **only** |

That is not just non-vacuity evidence — it is an independent confirmation of
the shape of both bugs. `rd.6` passes under both neuters, correctly, because
it is the case with no successful redirect to capture into. And C failing
*only* `rd.7` is exactly the claim that C's command path was already right and
only its scope path was wrong.

## Not a bug: silence in condition position

`cmd 2> capt.txt > /bad || echo fellback` puts the diagnostic in `capt.txt`
under bash and prints nothing under sash. That is DD#55, deliberate: a failure
caught by `if`/`&&`/`||` decides the branch and is not reported
(specs 55.25/55.50). Checked before chasing it.

## Gate

Predicted 6558, swept 6558 (6545 passing + 13 skipped), zero failing, zero
flips, exactly eight new ids. `mk test` 107/0, clippy clean. Every touched
function is far under the complexity gate — the worst is 5/4 against bars of
11.51/5.58.
