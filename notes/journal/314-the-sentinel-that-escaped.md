# 314 — The sentinel that escaped

The branch-coverage ranking pointed at `validate.rs:1222`, the `name ==
"__HEREDOC__"` arm of `check_heredoc`: 0 hits against 7,523 on its sibling.
That is the *command-name* arm; cov.14 already covered the *args* arm
(`cat <<EOF`), which is why the sibling was so busy.

It is reachable. A heredoc with no command in front of it —

```sh
<<EOF
hi
EOF
```

— puts the sentinel in the head position. So this was a plain coverage gap,
not dead code. But running the probe printed this:

```
h2.sh:1:1: error [SB5002] sash has no heredocs or here-strings — …
h2.sh:1:1: error [SB5011] command '__HEREDOC__' not found — …
h2.sh:2:1: error [SB5011] command 'hi' not found — …
h2.sh:3:1: error [SB5011] command 'EOF' not found — …
```

Three things wrong. `__HEREDOC__` is an internal lexer marker
(`scan_heredoc_sentinel`, `lexer.rs:518`) and had no business in a message
aimed at a user. The heredoc *body* came back as two unknown commands. And
`lib.rs` says, in as many words, that structural validation runs first
specifically so that "a rejected heredoc leaks its body lines as bare
commands" does not happen.

## Why the existing guard did not help

`validate_command` already does the right thing:

```rust
if check_heredoc(name, c, span, diags) {
    return;
}
```

But SB5011 does not come from `validate_command`. The command-head check is a
separate walk over the same program, and `check_heredoc`'s early return has no
reach into it. The guard suppressed the checks *next to it* and nothing else.
Same shape as the last five finds: a guard that computes the right condition
and then does not get to act on it.

## The fix

Batch-level, in `run_structural_checks`, matching the first-wins policy set
for SB4057 in journal 313:

```rust
if vdiags.iter().any(|d| d.code == "SB5002") {
    vdiags.retain(|d| d.code == "SB5002");
}
```

SB5002 is the real error; everything else in that batch is downstream of the
tokens the heredoc left behind. Reporting it alone is what the `lib.rs`
comment already promised.

Two specs, `cov.101` (the name arm) and `cov.102` (asserting the message
contains neither `__HEREDOC__` nor `SB5011`). Sweep 4665 passing, 0 failing.

## Also settled: `resolve.rs`

Two more zero-hit arms in the same ranking — `is_resolvable_external`'s
`name.is_empty()` (0 / 8180) and `resolve_command`'s `let ExprKind::Literal(…)
else` (0 / 8188). Instrumenting `resolve_command` showed it is never called at
all for a bad head: `run_structural_checks` runs at `lib.rs:165` and exits
before resolution at `lib.rs:170`.

Unreachable, but deliberately *not* converted to panics the way the lexer arms
in journal 312 were. That invariant was local — the byte dispatcher owns every
break byte, provable from the dispatch table. This one is cross-pass: it holds
only because one caller happens to sequence structural checks first. A panic
would couple the two passes and turn any future standalone resolution into an
abort. Recorded in `notes/tasks/resolve-guards-unreachable-by-earlier-pass.md`
so the ranking does not keep re-surfacing them.

The distinction worth keeping: **a local invariant earns an assertion; a
cross-pass one earns a comment.**
