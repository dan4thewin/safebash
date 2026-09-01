# 223 — DD#86 Phase 1: the `str` keyword

DD#86 (variable scope: no `local`, no shadowing, use-before-and-after) is a
five-phase change. This is Phase 1 — the `str` type keyword — plus a janet fix
it surfaced. The remaining phases (drop `local` + structural scope,
no-shadowing/redeclaration, dead-variable rejection, converter/specs) are filed
in `notes/tasks/dd86-scope-phases-2-5.md`.

## `str` — declare a string, meaningful-zero `""`

Globals must be introduced at top level (DD#86), so each type needs a
declare-without-initializer form. `int n` already declares an int (→ 0); `str s`
is the missing scalar: a string, meaningful-zero `""` (`str s = "x"` also works).
Wired as a keyword (lexer `Tok::Str` + `classify_word_token`, logos token,
`dispatch_statement` → `parse_declaration(scanner, "str")`, `declaration_axes`
`"str" => (Declare, String, [])`).

DG picked `str` over the floated `var` — `str` names the *type* (parallel to
`int`/`array`/`map`) and collides with **0** spec identifiers, vs 50 for `var`.

## janet no-init int fix

`int n` (no initializer) emitted malformed `(var n )` in janet:
`int_literal_to_janet("")` returned empty (a vacuous `chars().all`). Now an empty
int literal → `"0"` (meaningful-zero). Pre-existing; surfaced by scope.sh 86.5.

## Verified

specs `spec/scope.sh` 86.1–6 (all three modes): `str` no-init `""`, init,
global-write-from-function, `+=`, `int`/`str` parallel, and the read-in-one-fn /
write-in-another method pattern. Full 3-mode sweep vs HEAD `031b5f5`: **0
regressions, +18 passing** (86.1–6 × 3).

Note: `str s` no-init read only became clean once the DD#87 runtime
NUL-termination bug (found while testing this) was fixed — the two are linked.
Phases 2–5 remain.
