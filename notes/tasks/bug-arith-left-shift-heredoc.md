title: "BUG — left-shift `<<` in `(( ))` is mis-lexed (heredoc), gives wrong result; `<<=` no-op"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "[DONE] `int x=3; (( y = x << 2 )); echo $y` → 12"
  - "[DONE] `int x=3; (( x <<= 2 )); echo $x` → 12"
  - "[DONE] native + c + janet (spec 31.1j-m)"

# BUG — `<<` / `<<=` in arithmetic mis-lexed via heredoc  [FIXED 2026-06-26]

**Fixed** (journal 123). Root cause confirmed: `scan_lt` (lexer.rs)
tokenized `<<` as a heredoc sentinel (`__HEREDOC__`) regardless of
context, so the arith content collector saw `x __HEREDOC__ 2`. The arith
re-lexer already supported `<<`/`<<=` → Shl; the fix was purely to stop
the shell scanner from mangling `<<` inside arith.

Fix: added `Scanner.arith_depth` (set by `collect_arith_content` around
its pull loop); `scan_lt` emits a verbatim `Word("<<")` when
`arith_depth > 0` instead of the heredoc sentinel. `tok_to_text(Word)`
round-trips it, and `needs_separator_space`'s `<op>=` rule recombines
`<<=`. Heredoc rejection outside arith, `>>`, `<=`/`>=`, and redirects are
all unaffected (verified; zero spec regressions).


## Repro

```bash
$ printf 'int x=3\n(( y = x << 2 ))\necho $y\n' | sash …   # → 3, want 12
$ printf 'int x=12\n(( x <<= 2 ))\necho $x\n' | sash …      # → 12 (no-op), want 48
```

`>>` and `>>=` work correctly (12 >> 2 → 3; `x >>= 2` applies). Only the
left-shift forms are wrong.

## Cause (suspected)

The shell lexer tokenizes `<<` as a **heredoc** operator (it has no arith
context), so the `(( … << … ))` content is collected wrong by
`collect_arith_content` / `step_arith_collect` — the `<<` does not survive
as a shift operator the way `>>` (append-redirect) does. The sibling fix
`bug-compound-assign-non-plus` handled the `<op>=` split in
`needs_separator_space`, but `<<`/`<<=` are a heredoc-vs-shift
tokenization problem one layer up.

## Fix direction

Teach the arith content collector (or the scanner, in `((`-depth context)
that `<<` is a left-shift, not a heredoc — mirror whatever makes `>>`
work. Add spec cases to `spec/arithmetic.sh` (31.1 shift family + a `<<=`
compound-assign) once fixed.

## Note

`bug-compound-assign-non-plus` (the `-=`/`*=`/`/=`/`%=`/`&=`/`|=`/`^=`
no-op) is **fixed** (commit 2026-06-26); `<<=` is explicitly the one
compound-assign left failing, tracked here.
