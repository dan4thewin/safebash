---
title: "regex builtins match/sub/gsub + thin sugar + captures (DD#63)"
status: ready
priority: P2
kind: feature
depends: [DD63-regex-quoting-interpolation]
gates:
  - "TDD: spec tests below written first and red before code"
  - "match/sub/gsub are builtins taking ordinary string args (interpolation = normal arg eval)"
  - "[[ $x =~ RE ]] desugars to `match $x RE` (boolean); RE is just an argument"
  - "var =~ s/RE/SUB/ -> var=$(sub $var RE SUB); .../g -> gsub; ${var:s/...} -> $(sub|gsub ...)"
  - "captures: $(match x p)=whole match; `match arr x p` fills arr=[whole,g1..g9]; sub/gsub replacement \\0..\\9"
  - "\\xnn (2 hex) in regcomp; \\x + non-hex = error"
  - "native spec clean (only cf.c7/cf.c8); rust suite no net regression; complexity gate green"
---

# regex builtins (match / sub / gsub) + thin sugar (DD#63)

Implements DD#63. Regex becomes a builtin family; the syntaxes are thin
sugar. Quoting/interpolation falls out of ordinary argument evaluation,
so the currently-unimplemented field interpolation is fixed for free.

Spec-first. Use `if/else` for booleans (the `&& X || Y` chain after a
false test is separately broken — `dd62-test-expr-ssavalue` F1).

## Spec tests to write first

### A. Function forms (the primitives)

- **A1** `$(match …)` scalar = whole match: `echo $(match axbxc "x")` → `x`;
  `echo $(match aXXb "X+")` → `XX`.
- **A2** boolean: `if match abc "^a"; then echo y; else echo n; fi` → `y`;
  `if match abc "^z"; then echo y; else echo n; fi` → `n`.
- **A3** capture out-array: `match g "aXXb" "a(X+)b"; echo ${g[0]} ${g[1]}` → `aXXb XX`.
- **A4** `$(sub …)` first: `echo $(sub abab "b" "B")` → `aBab`.
- **A5** `$(gsub …)` all: `echo $(gsub abab "b" "B")` → `aBaB`.

### B. Interpolation = normal arg eval (function form)

- **B1** interpolated pattern: `p="a.*b"; echo $(match axxb "$p")` → `axxb`.
- **B2** single-quote disables: `echo $(match 'a.b' 'a.b')` → `a.b`
  (literal `.`; matches the literal string `a.b`).
- **B3** literal slash, no delimiter: `echo $(sub "a/b/c" "/" "_")` → `a_b/c`.

### C. Captures in the replacement (\0 / \1..\9)

- **C1** group ref: `echo $(sub "John Smith" "(\\w+) (\\w+)" "\\2 \\1")` → `Smith John`.
- **C2** whole-match \0: `echo $(gsub "a1b2" "[0-9]" "<\\0>")` → `a<1>b<2>`.
- **C3** literal backslash `\\`: `echo $(sub "x" "x" "\\\\")` → `\` (one backslash).

### D. Sugar desugarings

- **D1** match sugar: `x=axxb; if [[ $x =~ a.*b ]]; then echo y; fi` → `y`.
- **D2** match sugar interpolates: `p="a.*b"; x=axxb; if [[ $x =~ $p ]]; then echo y; fi` → `y`
  (today: literal `$p`, no match).
- **D3** match sugar quoted arg w/ space: `x="a b"; if [[ $x =~ "a b" ]]; then echo y; fi` → `y`
  (today: SB4014).
- **D4** mutation: `v=abc; v =~ s/b/B/; echo $v` → `aBc`.
- **D5** mutation global: `v=abab; v =~ s/b/B/g; echo $v` → `aBaB`.
- **D6** expression: `v=abc; echo ${v:s/b/B/}` → `aBc`.
- **D7** s/// sugar interpolates pattern: `x=b; v=abc; v =~ s/$x/Y/; echo $v` → `aYc`
  (today: literal `$x`, no-op).
- **D8** s/// sugar interpolates replacement: `r=Y; v=abc; echo ${v:s/b/$r/}` → `aYc`
  (today: literal `$r`).

### E. `\xnn` + anchors + counted-rep regression

- **E1** `\xnn`: `if match "a b" "a\\x20b"; then echo y; fi` → `y` (space via hex).
- **E2** `\x` + non-hex → error: `match x "a\\xZZ"` → rejected/runtime error (not a silent literal).
- **E3** anchor survives: `if match foo "o$"; then echo y; fi` → `y`.
- **E4** counted repetition (DD#39 stale): `if match aaa "^a{2,3}$"; then echo y; fi` → `y`.

### F. Engine facts to lock

- **F1** empty replacement (delete): `echo $(sub abc "b" "")` → `ac`.
- **F2** no case-insensitive flag: there is no `i` — `match ABC "abc"` does NOT match
  (assert `n`), documenting the limitation.

## Implementation outline (after specs are red)

1. **Builtins** `match` / `sub` / `gsub` (BuiltinCmd variants, like
   `keys`/`join`): args are ordinary lowered values. `sub`/`gsub` print to
   stdout; `match` sets exit status, prints `resub[0]` for `$(…)`, and in
   the 3-arg `match ARR S P` form fills `ARR` (mapfile/read-a-style
   out-array) with `[whole, g1..g9]`.
2. **Captures**: extend `sash_regex_match` to surface `resub[0..9]`
   (currently discarded). Confirm the `Resub` stride first (32 vs 16).
3. **Replacement backrefs** `\0..\9` in `sash_regex_replace` (template
   substitution from `resub`); `\\` literal.
4. **`\xnn`** in `regcomp` (vendored `regcomp.c`): two hex digits → byte;
   non-hex → error.
5. **Desugaring** (lower/transform): `[[ =~ RE ]]` → `match SUBJ RE`
   (RE is the RHS word/arg); `var =~ s/RE/SUB/[g]` → assignment of
   `$(sub|gsub SUBJ "RE" "SUB")`; `${var:s/RE/SUB/[g]}` → the same expr.
   The `s///` field split is the only bespoke parsing: `/`-delimited,
   fields emitted as double-quoted (interpolating) args.
6. **Retire** `parse_regex_rhs`'s flat-Literal handling and the
   `=~`/regex paths in the test codegen (they move to the `match`
   command). Coordinate with `dd62-test-expr-ssavalue`.

## Open (DD#63)

`case` `=glob` desugaring to a glob/match builtin; `\` + non-digit in the
replacement (literal vs error); the `Resub` stride.

## Cross-refs

- `dd62-test-expr-ssavalue`: the `=~`-as-`match`-command desugaring is
  the seam — once `=~` leaves the `TestExpr`, that task's value-operand
  refactor is cleaner. Sequence/coordinate the two.
- Mirror in `emit_c.rs` for c-backend parity.
