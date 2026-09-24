# 404 — Two redirects, one of them clobbering a file bash never touches

Coverage item 4 was `codegen/redirects.rs::emit_redirects_guarded`, six cold
lines. The cold region was the `Some(prev)` arm — the path taken when a command
carries a **second** file-open redirect.

The reachability question answered itself quickly. `cmd > out 2> err` compiles
and runs, so the arm is obviously live; a `panic!("PROBE-REACHED-SECOND-OPEN")`
inserted into it fired on the first probe. A pure coverage gap, not dead code.

That should have been the end of it — write two specs, close the gap, move on.
Instead the probes disagreed with bash twice.

## The diagnostic named the wrong file

```sh
cmd > good.txt 2> /nonexistent_d/bad
```

bash says `/nonexistent_d/bad: No such file or directory`. sash said
`good.txt: No such file or directory` — about a file that had opened perfectly
well. DD#70's contract is `<argv0>: <path>: <reason>`, and the `<path>` was a
lie.

The accumulator kept `prev.path_ptr` unconditionally. The comment beside it
called that "the first failing-candidate path"; it was the first
*open-candidate* path, and the two coincide only when there is exactly one
redirect. A comment that is true of the tested case and false of the general
one reads exactly like a correct comment.

## The second open ran anyway

Worse, and the reason this is a P2 rather than a cosmetic fix:

```sh
cmd > /nonexistent_a/x 2> made.txt
```

bash does not create `made.txt`. sash did. The redirect list was being treated
as a set of independent operations when it is a **sequence with side effects** —
`open` with `O_CREAT|O_TRUNC` is not something you get to perform speculatively.
`cmd > /bad > important.txt` truncated `important.txt`.

The emitted C carried the identical bug, both `sash_redirect` calls
unconditional, with the same misleading comment copied across.

## The precedent decided the fix

`notes/tasks/native-ternary-eager-eval.md` is the same shape: native evaluating
eagerly where bash short-circuits. DG took that as a P2 and fixed it with real
control flow to reach bash+C parity, and it too came out of coverage grinding.

DG chose the same here — short-circuit in both backends rather than a smarter
choice of reported path. That subsumes the first bug: once only one redirect can
fail, there is no "which path?" left to get wrong. The right fix made the other
bug stop existing, which is usually the sign you have found the actual defect
rather than its symptom.

Native branches on the accumulated failure flag (`emit_guarded_redirect`:
`brif` into do/skip blocks, three-wide block-param phi carrying
`failed`/`path_ptr`/`path_len`). Cranelift's `select` was not an option — it
evaluates both operands, so it cannot short-circuit a call. C needed no blocks
at all: `?:` short-circuits natively, so a guarded open is simply never called.

DD#70 gained the ordering addendum: left to right, stop at the first failure,
report the redirect that actually failed.

## What the failure counts were worth

Non-vacuity by revert gave **native 2 failures, C 3**. The counts differing is
the whole finding — C also failed `rd.3`, where the failing redirect comes
*first* and there should have been nothing to short-circuit.

Chasing that: the eager `2> rd3_ok.txt` open succeeded, so stderr pointed into
that file, and the C backend's `sash_emsg` — emitted *before* the fd restore —
wrote the diagnostic there. `$rerr` was empty. The short-circuit fixes rd.3 by
removing the second open entirely.

But it exposed a third divergence the fix does **not** address. Put the
succeeding redirect first:

```sh
/bin/echo hi 2> capt.txt > /nonexistent_d/bad
```

| | stderr | capt.txt |
|---|---|---|
| bash | empty | `bash: /nonexistent_d/bad: …` |
| **C** | empty | `./cbin: /nonexistent_d/bad: …` |
| **native** | `./q.bin: /nonexistent_d/bad: …` | empty |

bash writes the diagnostic through the redirections applied so far. C matches;
native restores the fds first and does not. Pre-existing, an oracle divergence,
and not a one-line move — `emit_open_failure_abort` fuses message, status and
block termination, so the restore cannot simply be resequenced around it. Filed
as `notes/tasks/native-redirect-diag-after-restore.md` rather than smuggled into
this commit.

Three lessons worth keeping. **The count of failures per mode is data**, not
just a pass/fail on non-vacuity — 2 vs 3 was the only thing that pointed at the
third bug. **An empty `$rerr` can be a bug rather than a quiet success**; that
is precisely how this one hid. And **coverage keeps finding eager evaluation**:
this is the second instance of native performing work bash short-circuits past,
which is starting to look like a category rather than a coincidence.

## Gate

6243 → 6251 (rd.1–rd.4, dual-mode, exactly as predicted). 0 failing, 0 flips
against `651d699`. `mk test` 59/27/20.

Extracting `emit_next_redirect` and `fold_redirect_failure` was not tidying:
both `emit_redirects_guarded` landed at cognitive 6 against a 5.64 bar and the
gate rejected them. They are now 1 and 3.
