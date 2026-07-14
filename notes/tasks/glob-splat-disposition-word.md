---
title: "glob-splat Part 2: `@{a:-WORD}` / `@{a:?WORD}` external-splat empty-disposition (DD#69)"
status: done
priority: P2
kind: feature
depends: [array-splat-command-arg]
gates:
  - "bare `@a` into an EXTERNAL command is a compile error directing to a disposition; builtins (echo/printf/…) and for-in are EXEMPT"
  - "`@{a:-WORD}` — empty array → the single arg WORD; bare `@{a:-}` → zero args. WORD is a full word (interpolation + concat + quoting), identical to the scalar `${x:-WORD}` default; it substitutes as a SINGLE element (not re-split)"
  - "`@{a:?WORD}` — empty array → runtime abort, exit 1, stderr `<program-name>: WORD` (the program's own argv[0]/basename, NOT `sash:`)"
  - "only `:-` and `:?` valid after `@{a` (no `:=`/`:+`), consistent with the DD#65 scalar prune"
  - "taint composes: disposition = cardinality-only, bless = value/taint-only; no diagnostic collision"
  - "native + c + janet; spec tests"
---

# glob-splat Part 2: external-splat empty-disposition (`@{a:-WORD}` / `@{a:?WORD}`)

`array-splat-command-arg` is done (`@a` expands per-element). DD#69 Part 2
is the rule that a bare `@a` into an **external** command is a compile
error and must carry an empty-disposition `@{a:-WORD}` / `@{a:?WORD}`.
The WORD semantics were a genuine design choice; **DG resolved them on
2026-06-21** (below), so this is now a ready implementation task.

## Decided (DD#69, recorded)

- Bare `@a` to an **external** command → compile error; must use a
  disposition. Builtins (echo/printf/…) and for-in are EXEMPT.
- `@{a:-WORD}` — empty array → the single arg `WORD`; bare `@{a:-}` →
  zero args. `@{a:?WORD}` — empty → runtime abort with `WORD` on stderr.
- No static-non-empty exemption; no dominating-guard recognition; no new
  keyword. Maps 1-to-1 onto bash `"${a[@]:-WORD}"` / `"${a[@]:?WORD}"`.

## Resolved (DG, 2026-06-21)

- **WORD = full word** — interpolation + concat + quoting, identical to the
  scalar `${x:-WORD}` default (option c). When substituted it is a *single*
  element (not re-split).
- **`@{a:?WORD}` abort** — exit 1; stderr is **`<program name>: WORD`** (the
  running program's own name, i.e. argv[0]/basename — NOT `sash:`).
- **Parser** — only `:-` and `:?` after `@{a` (no `:=`/`:+`).
- **Taint compose** — orthogonal: the disposition is cardinality-only, the
  bless is value/taint-only; they compose without collision.

## Ready to implement

The shared **word machinery** the resolution couples to is **already
built**: the scalar `${x:-WORD}` default now interpolates fully (commit
`6ffe982`, DD#65 stage). So `@{a:-WORD}` reuses that same word lowering
(interpolation + concat + quoting), substituting the result as one element.
No blocker remains.

Remaining implementation surface:
1. **Parser** — accept `@{a:-WORD}` / `@{a:?WORD}` (only `:-`/`:?`), reusing
   the scalar `:-` word body for WORD.
2. **Validator** — bare `@a` in an external-command arg position → compile
   error directing to a disposition (builtins/for-in exempt, per DD#69).
3. **Codegen/runtime** — empty-array → `@{a:-}` yields zero args, `@{a:-WORD}`
   yields the one WORD element; `@{a:?WORD}` aborts exit 1 with
   `<progname>: WORD` on stderr.
4. **c/janet** parity + spec tests.

Part 1 of glob-splat (globs-are-arrays: for-in glob per-element +
inline-`*` reject) is tracked in `glob-as-array-splat-disposition.md`.

## Implementation-ready code map (2026-07-08)

Concrete anchors from a code investigation, so the next session executes
without re-mapping. `@a` today → `parse_word_arm` (sash/src/parse/expr.rs
~219): `if w.starts_with('@') && w.len() > 1 { Expr::param(&w[1..],
Some("arrvals"), None, span) }`. This becomes `SsaValue::ArraySplat { array
}` (transform/expressions.rs `command_arg_splat`/`arg_to_value`; ssa.rs
~599) and is expanded per-element by the argv builder (codegen/commands.rs
~197-360: `sash_table_len` for argc, `sash_table_value(tbl,i)` to fill).

1. **Lexer/`@{…}` tokenization (the new bit).** Today `@{a:-foo}` does NOT
   scan as one token: `{` starts a brace group, so `@{a:-foo}` parses as `@`
   + a `{ a:-foo }` command group → "command 'a:-foo' not found". Fix: in
   the word scanner, when a word begins `@{`, scan through the matching `}`
   (respecting quotes, like `scan_braced_content`) into a single
   `Tok::Word("@{a:-WORD}")`. Then `parse_word_arm` handles `w.starts_with("@{")`.
   (Alternatively add an `AtBrace` token mirroring `DollarBrace`.)
2. **Parser.** In `parse_word_arm`, for `@{name:OP WORD}`: split `name`,
   require OP ∈ {`:-`,`:?`} (else a focused error; no `:=`/`:+` per DD#65),
   parse WORD with `parse_default_word` (the SAME interpolating word used by
   scalar `${x:-WORD}`). Emit a ParameterExpansion with operator
   `arrvals_default` (`:-`) / `arrvals_abort` (`:?`) and operand = the WORD
   expr. (Mirror how `${x:-WORD}` → `SsaValue::DefaultValue`.)
3. **Validate (SB5032, next free code).** In `check_argv_splat`
   (validate.rs ~1599): a **bare** `@a` (operator `arrvals`, no disposition)
   that is an ARG of an EXTERNAL command is SB5032. External detection is
   `is_external_source` (validate.rs ~1845: name starts `/`, or non-empty &
   not in `BUILTINS`). Runs in `run_semantic_checks` AFTER `resolve_commands`
   (lib.rs), so heads are resolved. Builtins (echo/…) + for-in exempt (a
   for-in word is not a command arg). `@{a:-…}`/`@{a:?…}` (the disposition
   operators) are allowed.
4. **Transform.** Lower `arrvals_default`/`arrvals_abort` to an
   `SsaValue::ArraySplat` variant carrying the disposition (extend the
   variant with `Option<{word: SsaValue, abort: bool}>`, or add
   `ArraySplatDisposition`). WORD lowers via `expr_to_value` (interpolating),
   like `DefaultValue.default`.
5. **Codegen (codegen/commands.rs argv builder).** Per splat: read
   `sash_table_len`. If len>0 → per-element (today's path). If len==0:
   `:-` with a WORD → push the one WORD value (already a string) as a single
   argv entry; `:-` empty (`@{a:-}`) → push nothing; `:?` → abort. Note WORD
   substitutes as ONE element (never re-split).
6. **Runtime abort (`:?`).** exit 1, stderr `<progname>: WORD` using the
   program's own argv[0] — reuse the DD#55 `sash_emsg`/`sash_emsg_status`
   path (sash-rt/src/lib.rs) which already prefixes `<argv0>:` (NOT `sash:`).
7. **Specs** (native + c; janet if the splat path exists there): bare-@a-
   external → SB5032; `@{a:-W}` empty→W / non-empty→per-element; `@{a:-}`
   empty→zero args; `@{a:?W}` empty→abort `<prog>: W` (native, rerr);
   `@{a:+W}`/`@{a:=W}` → error; `@a` to a builtin (echo) allowed; for-in `@a`
   allowed; WORD interpolates + is a single element. SB5032 is free
   (SB5031/5030 taken). Complexity gate green.

Non-goals (DG): external ONLY; no static-non-empty exemption; no
dominating-guard analysis; no new keyword.
