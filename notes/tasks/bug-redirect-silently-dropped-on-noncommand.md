---
title: A redirect on a non-command statement is silently dropped
status: open
priority: P1
depends: []
gates: [DG decides reject vs. implement]
---

# A redirect on a non-command statement is silently dropped

Found by the branch-coverage asymmetry query (journal 312 method):
`wrap_with_redirects` at `parse/parser.rs:2226` took its non-empty arm 0 times
in 18,467 calls. The first guess was dead code — `parse_command` consumes a
command's own redirects inline and wraps them itself, so by the time
`wrap_pipeline_part` runs there is usually nothing left to find.

It is not dead. It is reachable, untested, and the path it feeds throws the
redirect away.

## The divergence

```
$ cat t.sh
(( 1 + 1 )) > /tmp/f

$ bash t.sh   ; ls /tmp/f   →  /tmp/f          (created, empty)
$ ./t         ; ls /tmp/f   →  No such file
```

sash compiles it, exits 0, and never touches the file. No diagnostic, no
warning. The parser builds a correct `Redirected` node — `--emit-ir` shows it
— and the transform drops it on the floor.

## Where

`transform/stmt_handlers.rs:373`, `attach_redirects_to_last_inst`:

```rust
match inst {
    SsaInst::Command { redirects: r, .. }
    | SsaInst::ReadLine { redirects: r, .. }
    | SsaInst::ReadArray { redirects: r, .. } => { *r = redirects.to_vec(); }
    SsaInst::Pipeline { stages, .. } => { ... }
    _ => {}                      // ← here
}
```

Only four inst kinds can carry a redirect list. An `ArithTest` body lowers to
arithmetic insts, none of which is one of the four, so the `_` arm runs and
the list is discarded. Same shape as SB4056 and the `scan_via_logos`
fall-back: an arm that handles a case it cannot handle by doing nothing.

## The full surface

Compound bodies are already handled — `lower_redirected_stmt` brackets them
with `__redirect_push`/`pop`, and the parser rejects the ones that should not
have redirects at all (SB4021). Function definitions, declarations and
assignments are rejected at parse time (SB4046). What is left:

| form                                    | result            |
|-----------------------------------------|-------------------|
| `(( 1 + 1 )) > f`                       | dropped silently  |
| `- (( 1 + 1 )) > f`                     | dropped silently  |
| `return (( 1 + 1 )) > f`                | dropped silently  |
| `return [[ -f /etc/passwd ]] > f`       | dropped silently  |
| `[[ $s == x ]] && (( 1 + 1 )) > f`      | dropped silently  |

Every one of these compiles clean and exits 0.

## The fork — needs DG

**(a) Reject it.** Extend the existing "a redirect attaches to a command"
family. `noncommand_redirect_target` already returns a noun for
`Function`/`Declaration`/`Assignment` and the parser raises SB4046; adding
`ArithTest` and a non-command `ReturnProducer` fits the same slot, and SB4021
already says the same thing about compounds. Two existing diagnostics point
this direction.

Cost: `(( x = 1 )) > log` stops compiling. Nothing in the suite uses it.
Behaviour change at the language surface, so it is DG's call, not mine.

**(b) Implement it.** `lower_redirected_stmt` already has
`bracket_compound_with_redirects`, which opens the fds, runs the body and
restores. Widening the bracket condition from "body is compound" to "body does
not lower to a redirect-carrying inst" makes these forms behave like bash. The
machinery exists and works.

Cost: `(( 1 + 1 )) > f` creating an empty file is a bash wart with no obvious
use, and sash has been rejecting exactly this class of construct elsewhere.

**Recommendation: (a).** SB4021 and SB4046 both already tell the user that
redirects attach to commands, and `(( … ))` is documented as an arithmetic
test, not a command. Option (b) would make sash *more* bash-compatible in a
corner where sash has deliberately chosen not to be twice already.

Either way the silent drop has to go — that part is not in question.

## Test debt regardless of the answer

`wrap_with_redirects`'s non-empty arm has no spec coverage at all. Whichever
option wins, the five forms in the table above need specs, because right now
nothing in 4,627 tests reaches that line.
