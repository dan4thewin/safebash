# 373 — the flags slot that was one character wide

*2026-09-19*

Two findings from the coverage queue, one of which is a bug that was never
cold at all.

## The cold lines that could not be reached

Four functions came off the cold-function list as *error closures* nested
inside hot functions: `parse_regex_mutation`'s two at `parser.rs:2054` and
`:2084`, `parse_at_brace_disposition`'s at `expr.rs:227` and `:230`. Every one
is an error path, so the question was the usual one — **what input reaches
this?**

Guessing from the diagnostic text would have been wrong. I marked all four
with unique `eprintln!`s and fed them malformed input. Every malformed `=~`
payload landed on a *fifth* mark, in the scanner: `take_sed_stmt_payload`
rejects first and its doc comment says why — it *"requires `s/` at that
point"*. Which makes `parser.rs:2054`'s `strip_prefix("s/")` provably dead:
the parser cannot be entered with a payload that lacks the prefix the scanner
already demanded.

That is the third provably-unreachable backstop this stretch, after
`intern_or_declare`'s miss branch (journal 372 — measured, zero hits across
5747 specs). **They cannot be covered by any input**, and they are now the
main structural obstacle to the 100% target. Question batched for DG below.

## The diagnostic that named a sigil the user had not written

While probing those paths, `@{a:-W` with no closer reported:

    unterminated `${`

`scan_by_brace_depth` is shared between `${…}` and the DD#69 `@{…}` splat
disposition and had the opener hardcoded. It now reads the actual byte at
`inner_start - 2` and names what the source used. Small, but a diagnostic that
points at a sigil you did not type is worse than no diagnostic — it sends you
looking for a second bug.

## The bug: every sed flag except `g` was ignored

The marking run produced a result I was not looking for. `x =~ s/a/b/x` —
deliberate junk, expected to hit a mark — hit **nothing**. No marks, no error,
clean compile.

`build_one_sed_sub` had exactly one line of flag handling:

```rust
let cmd_name = if flags.contains('g') { "gsub" } else { "sub" };
```

That *is* the implementation. Measured against real `sed`, sash silently
accepted `x`, `zzz`, `G`, `gg` and `i`, doing nothing with any of them. Two
of those are actively dangerous:

- `s/a/b/i` reads as a promise of case-insensitivity and delivers a
  case-sensitive substitution. DD#63 is explicit that the engine has **no
  `REG_ICASE`** and the sanctioned idiom is `$(tolower …)` on both sides — so
  the one flag a bash author is most likely to reach for was the one most
  likely to produce wrong output silently.
- `contains('g')` is a substring test, so `s/a/b/Xg` quietly became a gsub.

**This did not need a decision.** DD#63 says "`g` is therefore the
`sub`/`gsub` choice, not a flag argument. There are **no other flags**", and
DD#96 documents this exact shape — *"a path that silently accepted junk
because no spec pinned the rejection"* — fixed for the tail-junk slot and
never for the flags slot next door. A stated rule with an unimplemented
consequence is not a fork. SB4032 now rejects anything that is not `""` or
`g`, names the offending flags, and points at `$(tolower …)`.

One fix covers both surfaces: the `=~` statement form and the
`${v:s/RE/SUB/flags}` expansion form share `build_one_sed_sub`. The DD#96
chain `${v:s/a/1/,s/b/2/g}` is unaffected.

I rejected `gg` as well — "exactly `g`" is the cleanest reading of "there are
no other flags". Reported, not asked; open to veto.

## Complexity

The lexer fix un-grandfathered `scan_by_brace_depth`, which was already at
cog 6 against a 5.85 bar before I touched it. Split the brace-counting loop
into `advance_to_unnested_close`; the scan is now 3/1 and the loop 6/5. The
loop also reads better as `while let Some(&b) = self.source.get(self.pos)` —
the bounds check and the `depth > 0` guard were doing the same job twice.

## Gates

`cargo fmt`; units 92 green; sweep **5765, 0 failing, 0 flips vs `4a93503`**
(5747 + 9×2, predicted exactly). Nine new specs: `63.fl1`–`63.fl7` in
`regex.sh`, `gs.38`/`gs.39` in `glob_splat.sh`. Collision check clean.

## For DG

**Policy on provably-unreachable defensive backstops.** Three concrete
instances now: `intern_or_declare`'s miss branch (deliberate, commit
`e4c7422`, task `eliminate-ice-arms`; measured zero hits), `parser.rs:2054`,
`parser.rs:2084`/`expr.rs:227`. Options: leave them cold and accept a ceiling
below 100%, delete them, make them loud (ICE), or exclude them from the
target. There is a live tension with the redirect fix you just ratified —
though the cases differ materially: the redirect skip was silently *wrong*,
these are silently *correct*.

**Eight bugs.** The method held again, but not in the shape I expected: the
cold lines turned out to be unreachable, and the bug was on a surface that
coverage had always reported as hot. Asking *what input reaches this?* is
what found it — the answer was "nothing does", and the input I used to prove
that was the one that should have failed.
