---
title: Tilde expansion produces SB5018-rejected $HOME
status: done
priority: P3
kind: bug
depends: []
gates:
  - "compiler: unquoted leading `~` / `~/path` → focused diagnostic mentioning `~` (not the misleading $HOME SB5018), directing to $(getenv HOME)"
  - "literal `~` stays available (quoted '~'/\"~\", mid-word a~b — already works)"
  - "sash-convert: rewrite bash `~`/`~/path` → $(getenv HOME) (+ bless) so converted sash compiles"
  - "new e2e_tests case for `echo ~` (compiler error) and a sash-convert test for the rewrite"
---

# Tilde expansion produces SB5018-rejected $HOME

> **DONE (2026-08-10).** Both halves shipped.
>
> **Compiler half (2026-07-07):** `parse_word_arm` rejects an unquoted
> leading `~` / `~/path` with a focused **SB4036** ("unquoted leading `~`
> is not expanded — use $(getenv HOME) …") instead of lowering to `HOME`
> and hitting the misleading `$HOME` SB5018. Quoted (`'~'`/`"~"`) and
> mid-word (`a~b`) tildes are untouched. spec/taint.sh 53.1-53.10.
>
> **Converter half (2026-08-10):** `sash-convert` now rewrites bash
> `~`/`~/path` → hoisted `home=$(getenv HOME); bless home [[ … ]]` +
> `$home`/`$home/path` (mirrors the `$HOME` env-var hoist). Forms it can't
> mechanically resolve are left with a MANUAL note so the SB4036 compile
> error forces review: `~user` (needs a passwd lookup), multi-tilde words
> (`~/bin:~/lib` — no partial rewrite), and command-position tildes
> (`~/bin/prog` — DD#23 rejects a dynamic command path). `~/$x` composes
> correctly (`concatenation(word "~/", $x)` → `$home/$x`). `convert_tilde_word`
> in `sash-convert/src/convert.rs`; spec/convert.sh 57.47-57.55 (+ end-to-end
> convert→compile→run verified byte-identical to bash); Rust
> `sash-convert/tests/convert_tests.rs` tilde_* (4).

## Decision (DG, 2026-06-21): split compiler vs converter

Fits the DD#57 converter/compiler division:

- **Compiler (sash): reject unquoted leading `~`/`~/path`** with a
  **focused diagnostic** (new SB code) that mentions `~` and directs to
  `$(getenv HOME)` — not the current misleading SB5018 "$HOME" message.
  Strict, no magic. A literal `~` remains available via quoting (`'~'`,
  `"~"`) or mid-word (`a~b`) — all already work; only unquoted *leading*
  `~` is at issue.
- **sash-convert: expand `~`/`~/path` → `$(getenv HOME)` and bless it** so
  a bash script using `~` translates to working sash. The converter does
  the bash-compat translation (with the explicit bless); the compiler
  stays strict.

## Implementation notes

- Compiler: `parse/expr.rs::parse_word_arm` (~138) currently lowers `~` →
  `SimpleExpansion("HOME")` and `~/path` → `Concat([HOME, "/path"])`,
  which the validator then rejects via SB5018 — losing the tilde origin
  and producing the `$HOME` message. To give a focused message, preserve
  the tilde origin to a rejection point. Options: (a) a dedicated
  `ExprKind::Tilde(Option<String>)` marker rejected in `check_rejected`
  (parser.rs) — principled but touches ~12 files that match `ExprKind`
  exhaustively (codegen, emit_c, emit_janet, transform, type_check,
  validate); (b) thread a parse-time diagnostic through `parse_word_arm`;
  (c) a sentinel the validator special-cases. Pick during implementation.
- sash-convert: rewrite at the tree-sitter CST level (bash `~` →
  `$(getenv HOME)`), inserting a bless for the captured value (HOME is a
  known external; the converter already emits bless/CHECK hints).

## Cause (original)

`parse/expr.rs:138 parse_word_arm` lowers `~` → `SimpleExpansion("HOME")`
and `~/path` → `Concat([SimpleExpansion("HOME"), Literal("/path")])`.
`validate.rs:388 ENV_VARS` lists `HOME` as SB5018-rejected (DD#53). So the
parser produces an expression the validator always rejects, with a
message about `$HOME` rather than `~`.

## DD#53 context

Per DD#53, `$HOME` is a plain sash variable, empty unless assigned; the
only path to the user's home is `$(getenv HOME)` + bless. So `~` has no
clean compile-time meaning in sash — hence the compiler rejects it and the
converter rewrites it to the explicit form.

## Related

- Journal 109 (the handle_cd discovery); DD#53 (env isolation); DD#24
  (taint/bless); DD#57 (converter/compiler split).
- `sash/src/parse/expr.rs:138`, `sash/src/validate.rs:388`.
