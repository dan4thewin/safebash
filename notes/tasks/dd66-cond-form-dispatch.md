---
title: "[[ ]] dispatch (cond) form + remove case/esac/glob-match (DD#66)"
status: ready
priority: P2
kind: feature
depends: [DD66-cond-form-dispatch, DD62-test-expr-simplification, DD63-regex-quoting-interpolation]
gates:
  - "TDD: spec tests below written first and red before code"
  - "boolean [[ ]] unchanged; dispatch form selected by a { after the first pattern"
  - "== is exact equality only (no alternation) in BOTH forms; =~ unanchored regex"
  - "dispatch is a statement + catching construct: $? resets to 0 after (no group-status leak)"
  - "|| { } is the optional default; first-match-wins; no fall-through"
  - "case/esac removed (reserved-word diagnostic); =glob/fnmatch_impl/;&/;;& deleted; sash_glob (filenames) untouched"
  - "converter rewrites bash case -> [[ ]] dispatch"
  - "native spec clean; rust suite no net regression; complexity gate green"
---

# `[[ ]]` dispatch form + remove `case` (DD#66)

Implements DD#66. `[[ ]]` gains a multi-way **dispatch statement**
alongside the existing **boolean test**; `case`/`esac` and the
`=glob`/`fnmatch` string-match path are removed. Reuses the `ir::Case`
dispatch-chain lowering with the op carried from the `[[ ]]` operator.

Spec-first. Use the function/`if` forms for booleans where needed.

## Spec tests to write first

### A. Exact dispatch (`==`)

- **A1** first arm: `x=foo; [[ $x == foo { echo a; } bar { echo b; } || { echo c; } ]]` → `a`.
- **A2** middle arm: `x=bar; [[ … ]]` (same) → `b`.
- **A3** default: `x=zzz; [[ … ]]` (same) → `c`.
- **A4** no default, no match → no-op: `x=zzz; [[ $x == a { echo a; } ]]` → (no output, rc 0).
- **A5** single arm = `if`: `x=a; [[ $x == a { echo y; } ]]` → `y`.
- **A6** single arm + default = `if/else`: `x=q; [[ $x == a { echo y; } || { echo n; } ]]` → `n`.

### B. `==` is exact only (no alternation, no patterns)

- **B1** literal pipe matches literally: `x='a|b'; [[ $x == 'a|b' { echo lit; } || { echo no; } ]]` → `lit`.
- **B2** `a|b` is NOT "a or b": `x=a; [[ $x == 'a|b' { echo hit; } || { echo miss; } ]]` → `miss`.
- **B3** boolean `==` still exact (regression): `x=a; if [[ $x == a ]]; then echo y; fi` → `y`;
  `x=a; if [[ $x == 'a|b' ]]; then echo y; else echo n; fi` → `n`.

### C. Regex dispatch (`=~`, unanchored — DD#63)

- **C1** unanchored prefix: `x=apple; [[ $x =~ ^a { echo A; } || { echo o; } ]]` → `A`.
- **C2** unanchored suffix: `x=cab; [[ $x =~ b$ { echo B; } || { echo o; } ]]` → `B`.
- **C3** alternation needs `^(a|b)$`: `x=b; [[ $x =~ '^(a|b)$' { echo m; } || { echo n; } ]]` → `m`;
  `x=c; [[ … ]]` → `n`.
- **C4** the `^a|b$` precedence gotcha (documents, does not fix): `x=ax;
  [[ $x =~ '^a|b$' { echo hit; } || { echo no; } ]]` → `hit` (`^a` matched the prefix).
- **C5** interpolated pattern (DD#63): `p='^a.*'; x=abc; [[ $x =~ "$p" { echo y; } || { echo n; } ]]` → `y`.

### D. `||` default marker

- **D1** literal `||` pattern via quoting: `x='||'; [[ $x == '||' { echo bar; } || { echo def; } ]]` → `bar`.
- **D2** default fires only on no match: `x=hit; [[ $x == hit { echo h; } || { echo d; } ]]` → `h`.

### E. `$?` — statement vs test (the core semantic)

- **E1** dispatch resets `$?` (catching construct): `x=foo; [[ $x == foo { false; } ]]; echo $?` → `0`
  (group ran `false`, but the dispatch catches/resets).
- **E2** dispatch with default + failing group still resets:
  `x=q; [[ $x == foo { true; } || { false; } ]]; echo $?` → `0`.
- **E3** boolean form still YIELDS the test result (regression):
  `x=a; [[ $x == a ]]; echo $?` → `0`; `x=b; [[ $x == a ]]; echo $?` → `1`.
- **E4** group-status handled internally: `x=foo; [[ $x == foo { false || echo caught; } ]]` → `caught`.

### F. Removal of `case` + glob-match + `;&`/`;;&`

- **F1** `case` is a reserved-word/parse error: `case $x in a) echo a ;; esac` → rejected
  (`rc != 0`, err points at `[[ … ]]` dispatch).
- **F2** `;&`/`;;&` no longer lex/parse: any input containing them in a (now-removed) case → rejected.
- **F3** filename glob still works (untouched): `mkdir d; : > d/x.sh; arr=(d/*.sh); echo ${arr[0]}` → `d/x.sh`
  (or the existing filename-glob spec — assert `sash_glob` path is intact).

### G. Disambiguation (boolean vs dispatch)

- **G1** no `{` ⇒ boolean: `x=a; if [[ $x == a && $x != b ]]; then echo y; fi` → `y` (compound boolean intact).
- **G2** `{` after pattern ⇒ dispatch: A1 above.
- **G3** outer `||` after `]]` is the normal list-OR, not the default:
  `x=q; [[ $x == a { echo hit; } ]] || echo outer` → `outer`?  **NOTE:** dispatch resets `$?`=0,
  so the external `||` does NOT fire — assert output is empty (the dispatch "succeeded" as a statement).
  (Lock this with DG — it is the statement-form consequence.)

## Implementation outline (after specs are red)

1. **Parser**: in `[[ ]]`, after `SUBJECT (==|=~)`, peek for `PATTERN {`.
   If present, parse the dispatch grammar — repeating `PATTERN { group }`,
   optional trailing `|| { group }` — into `ir::Case` (or a renamed
   `Cond`). Otherwise parse the boolean `TestExpr` as today. `{ group }`
   reuses the existing brace-group parse.
2. **Lowering**: reuse `build_dispatch_chain`, carrying `==`/`=~` as the
   per-arm op instead of `=glob`. Anchoring/quoting per DD#62/DD#63.
3. **`$?`**: add the dispatch merge label to `is_catching_construct_exit`
   (`codegen/entry.rs`, `emit_c.rs`) and the SB5012 state machine.
4. **Remove**: `parse_case`/`parse_case_item`, `Tok::SemiAmp`/`DblSemiAmp`,
   `CaseItem.fallthrough`/`resume` + their wiring, the `=glob` op +
   `emit_glob`, and `sash_fnmatch`/`fnmatch_impl`. Add a `case`
   reserved-word diagnostic (new SB code).
5. **Backends**: rewrite the Janet/C case emission for the dispatch form
   (subsumes `bug-janet-case-multipattern`).
6. **Converter**: bash `case … esac` → `[[ … ]]` dispatch (exact → `==`;
   `a|b|c`/glob → `=~` regex; `*)` → `|| { }`).
7. **Docs**: SUBSET.md / GUIDE.md — replace `case` with the dispatch form.

## Cross-refs

- `dd62-test-expr-ssavalue` — the `[[ ]]` operand→SsaValue refactor; the
  dispatch form's operands are values too. Sequence/coordinate.
- `regex-interpolation-delimiters` — `=~`/`match` builtin the dispatch
  desugars onto.
- Supersedes `bug-case-pattern-glob-not-validated` (close on landing).
