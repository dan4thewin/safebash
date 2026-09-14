---
title: The operator with nothing after it
---

# The operator with nothing after it

```
$ sash -c 'echo a &&' && ./a.out
a
```

bash calls that a syntax error. sash compiled it, ran it, and exited 0. Same
for `echo a ||` and `echo a |`.

## How coverage pointed at it

After the END wiring I re-ran branch coverage and asked a different question
than "which function has the most misses". The useful ranking is **asymmetry**:
a branch arm with zero hits whose *sibling* arm ran tens of thousands of times.
A cold function is just untested. A cold arm inside a hot line is a path the
compiler has structurally never taken, and that is either dead code or a lie.

```sql
WITH pairs AS (
  SELECT a.file, a.line, a.count AS dead, MAX(b.count) AS live
  FROM cov_branches a JOIN cov_branches b
    ON a.file=b.file AND a.line=b.line AND a.block=b.block AND a.branch<>b.branch
  WHERE a.count=0
  GROUP BY a.file, a.line, a.block, a.branch
)
SELECT file, line, live FROM pairs WHERE live > 2000 ORDER BY live DESC;
```

Fourth row: `parse/parser.rs:165`, 19,915 hits against 0. That is

```rust
pub fn parse_statement(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    skip_terminators(scanner);
    if scanner.at_eof() {
        return Ok(None);          // never taken, 19,915 times
    }
```

The `Ok(None)` arm is the function's "nothing here" answer, and no caller had
ever provoked it. So I asked which caller *could*. Six call sites; five check
`at_eof` before calling. One does not:

```rust
while matches!(scanner.peek().token, Tok::Pipe) {
    scanner.next();
    skip_terminators(scanner);
    if let Some(next_stmt) = parse_statement(scanner)? {
        append_pipeline_stage(&mut commands, next_stmt);
        break;
    }
}
```

On `None` the loop just re-tests `peek()`, finds EOF rather than a pipe, and
falls out — returning the pipeline as its left side alone. The `|` is
discarded. `echo a |` becomes `echo a`.

I went looking for that one input and got the compiler to accept it.

## The second one was worse

Probing the neighbouring operators found `&&` and `||` doing the same thing by
a different mechanism:

```rust
let right = parse_list_operand(scanner)?.unwrap_or_else(|| Statement::true_cmd(span));
```

A missing right operand was **substituted with `true`**. `echo a &&` parsed as
`echo a && true`.

I checked whether that was deliberate before touching it. No DD mentions it;
`git log -S` shows it predates the left-associativity fix (`dad1dfe`) and was
carried through that rewrite unchanged, from `.unwrap_or(...)` to
`.unwrap_or_else(...)`. It is a default that was never a decision.

The sibling `true_cmd` in `background_list_op` **is** deliberate and stays: a
trailing `&` genuinely has no right-hand side, and bash accepts `echo a &`.
That is the distinction — `&` is a terminator, `&&` is an infix operator.

## Fix

**SB4056**: a list or pipeline operator with no right-hand side. The
diagnostic points at the operator, not at EOF, because the operator is what
the user has to delete or complete.

Both sites now bind with `let … else` and return the error instead of
silently repairing the parse.

## Why the suite missed it

There was nothing to catch. The spec suite tests programs that *mean*
something; nobody writes `echo a &&` on purpose, so nobody wrote a test for
it. And the failure mode was silent acceptance — the compiler produced a
working binary, so no oracle comparison fired either. The only thing that
could point here was a structural measurement of what the parser had never
done.

That is twice now in three days that a zero-hit branch on a hot line was a
real defect rather than a coverage gap: `program.end_block` (journal 309)
and this. The pattern is worth keeping as a standing query.

## Also fixed

`transform/entry.rs:152` read

```rust
if !block.label.starts_with("__end__") && !block.label.contains("__end__") {
```

`starts_with` implies `contains`, so `!S && !C` is exactly `!C` — the first
test can never change the result. Coverage agreed: the second condition's
false arm had 0 hits against 19,624. Simplified to `!contains`, which is
logically identical for all inputs, not just the ones we measured.

## Specs

`errors.sh` 95.13–95.20: trailing `&&`, `||`, `|`, and a trailing newline
after `&&` (the terminator skip runs first, so it is the same error); plus
four controls — `echo a &` still accepted, a folded `&&` chain, a live `||`,
and a three-stage pipeline.

Sweep: **4627 passing, 10 skipped, 0 failing**.

## Still open from this coverage pass

Two hot-line zero-arms evaluated but not yet resolved, both in the lexer:

- `lexer.rs:235` — `next()`'s `else` arm (pull a fresh token rather than drain
  the peek buffer) has 0 hits against 86,406. Every `next()` in the codebase
  is preceded by a `peek()`. The path is real and reachable in principle, not
  dead; it needs either a unit test or a demonstration that the buffer is
  structurally always primed.
- `lexer.rs:1581` — `scan_word_chars` never returns `Some` in 43,028 calls.
  Its early-return exists for a case nothing produces. Worth deciding whether
  it is dead.

Filed rather than guessed at.
