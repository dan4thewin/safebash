---
date: 2026-09-16
---

# The unterminated `$(`

The branch-coverage loop turned up a bug the last round of the *same*
bug did not reach.

## What the miss looked like

`parse_status_word` had four cold branches, and they were all four
branches of one `while` condition — which means the loop was never
entered, not that some arm of it was unexercised. `$(status)` is the
only spelling any spec used, and that returns from the `RParen`
fast-path above the loop.

So I went looking for a spelling that gets past it. `$(status
/bin/true)` does, and rejects with SB4045. `$(status; echo b)` does,
and rejects with SB5073. And then:

```
$ printf 'echo $(status\n' > t.sh && sash t.sh -o t.bin
$ ./t.bin
sash: bug: bare command passed to sash_exec (expected absolute path)
./t.bin: status: command failed (exit 127)
```

An unterminated `$(` **compiled**. The binary then tripped the
runtime's own assertion — the one whose message begins with the word
"bug".

## It was not about `status`

`status` made it loud, but the hole was general:

| input | before |
|---|---|
| `echo $(echo hi` | compiled, printed `hi` |
| `echo $(echo a` ⏎ `echo b` | compiled, printed `a` then `b` |
| `echo $(status` | compiled, runtime assertion |
| `echo $(getcpid` | compiled, runtime assertion |
| `echo "$(status` | SB4057 — correct |

The quoted spelling was right the whole time. Only the unquoted path
was blind, and the missing `)` was simply discarded.

## Why the previous fix missed it

Journal 313 fixed exactly this failure mode: every scan-to-closer
helper in the lexer ended with

```rust
if self.pos < self.source.len() { self.pos += 1; }   // skip closing X
```

where the guard *is* the run-off-the-end detection and silently did
nothing. That became specs 57u.1–57u.12.

`parse_command_substitution` is not a lexer helper, so the audit did
not cover it — but it has the identical shape one layer up:

```rust
loop {
    if scanner.at_eof() { break; }      // <- the same silent guard
    ...
    Tok::RParen => { scanner.next(); break; }
    ...
}
Ok(Expr { kind: ExprKind::CommandSubstitution(stmts), span })
```

Both exits produce a well-formed node. Nothing downstream can tell
"closed" from "ran out of input".

The fix is a `closed` flag set only on the `RParen` arm, and SB4057
when the loop ends without it — the same code and the same wording the
quoted path already used. Specs 57u.13–57u.18, deliberately filed next
to the lexer family they belong with.

**The lesson is about the shape, not the layer.** "Break out of a scan
when input runs out, then return success" is the pattern; auditing it
in one module found eleven instances and left this one because it
lived somewhere else.

## A second find, not yet acted on

Chasing `brief_arith`'s cold arms established that
`ArithExpr::PreIncDec` **never survives the parser**.
`reject_prefix_incdec` runs at both arithmetic entry points
(`parse_arith_expansion`, `parse_arith_slot`) *before* every other
check, and it recurses through the same node set as the walkers that
follow, so `++x` is rejected everywhere it can be written — including
the two entry points that bypass it (`emit_string_part` and
`parse_expansion_atom`), which have their own rejection.

The variant therefore exists only to carry a good diagnostic, and its
handling in `transform/`, `emit_ir.rs`, `validate.rs` and `brief_arith`
is unreachable. Filed as `notes/tasks/purge-preincdec-dead-node.md`
rather than acted on: it spans five files across four passes, and
whether a parse-only IR node *should* keep defensive downstream arms is
DG's call, not mine.

## Sweep

5189 → **5207**, 0 failing, 83 units green. Predicted exactly (9 new
specs, both files dual-mode).
