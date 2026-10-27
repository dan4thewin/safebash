title: DD#66 — `[[ ]]` gains a dispatch (cond) form; `case`/`esac` removed
status: accepted

`[[ ]]` becomes sash's single match construct, in **two forms**: the
familiar **boolean test** (yields `$?`) and a new **dispatch statement**
(runs one of several command groups). The dispatch form replaces `case`
entirely. The match mode is the `[[ ]]` operator — `==` (exact) or `=~`
(regex) — stated once, so exact-vs-regex dispatch is a first-class,
per-construct choice. From the 2026-06-19 discussion.

## The two forms

```
# Boolean (test) form — UNCHANGED. Yields $? (the test result).
[[ SUBJECT OP OPERAND ]]            # ==, !=, =~, -z, -n, -eq…, &&, ||

# Dispatch (statement) form — runs ONE matching group.
[[ SUBJECT (== | =~)
     PAT  { group }
     PAT  { group }
     …
     || { group }                  # optional default
]]
```

**Disambiguation:** a `{` following the first pattern ⇒ dispatch form;
otherwise the boolean form. `{` cannot follow a pattern in a boolean
`[[ ]]` today, so the split is unambiguous and back-compatible.

## Semantics

- **`==` is strict exact equality, in BOTH forms.** No alternation, no
  pattern language. `[[ $x == a|b ]]` does *not* mean "a or b" — to match
  alternatives, use `=~`. (Allowing `|` only in the dispatch arms would
  force the boolean form to honour it too, re-introducing a degenerate
  glob-like pattern language in `==`. Rejected.) A pattern containing
  shell metacharacters is quoted: `[[ $x == 'a|b' { … } ]]` matches the
  literal string `a|b`.
- **`=~` is unanchored regex** (DD#63), in both forms. Whole-string match
  is the user's `^…$`. **Alternation is `^(a|b)$`, not `^a|b$`** — `|` has
  the lowest regex precedence, so `^a|b$` parses as `(^a)|(b$)` ("starts
  with a OR ends with b"). Document the parenthesised form.
- **First match wins.** Arms are tested top-to-bottom; the first matching
  arm's group runs; the rest are skipped. **No fall-through** (`;&`/`;;&`
  are gone — DD#66 drops them).
- **`|| { group }`** is the optional default — runs iff no arm matched.
  `||` is the marker *because* it is an operator (never a bare string), so
  it cannot collide with a pattern; a literal `||` pattern is `'||'`. It
  also reads like the (corrected) `cond && this || that`: each
  `pat { group }` is an implicit `&&`, the trailing `|| { group }` the
  explicit "otherwise."
- **No default + no match ⇒ no-op.**

## `$?` — statement vs test (the clean separation)

The two forms differ deliberately on exit status:

- The **boolean form is a *test*** — it *produces* `$?` (the match
  result), for `if [[ … ]]`, `[[ … ]] || cmd`, etc.
- The **dispatch form is a *statement* and a *catching construct*** —
  exactly like `if`/`while`/`for`/`case`. After it, **`$?` is reset to 0**;
  it does *not* expose "which group's status," removing that ambiguity. A
  group that needs its own status handles it *internally*
  (`[[ $x == foo { cmd || die; } ]]`), the same way you would inside an
  `if` body.

Mechanically the dispatch merge block joins the catching-construct set in
`is_catching_construct_exit` (`codegen/entry.rs:263`, mirrored
`emit_c.rs:462`: `if.merge | while.exit | for.exit | case.merge | …`) and
the SB5012 `$?` state machine (`validate.rs` ~763-811). This is the same
machinery `case.merge` already used — it transfers, it is not new.

**Composition: statement-only (decided (b), 2026-06-19).** Because the
dispatch form is a *statement*, not a test, it may **not** appear where a
boolean is expected — it is a **diagnostic** (new SB code) to use a
dispatch `[[ … ]]` (groups present) as the condition of `if`/`while`/
`until`, as the operand of `!`, or as an operand of `&&`/`||`. Otherwise
its always-`0` status would make such chains silently always-succeed —
exactly the footgun this construct removes. The fix the diagnostic points
to: make the dispatch a standalone statement (move the conditional logic
*inside* a group, or use `if BOOLEAN; then DISPATCH; fi`). The boolean
form (no groups) remains usable as a condition everywhere, unchanged.

## Removed

- The `case`/`esac` surface (`parse_case`, `parse_case_item`), the
  `in`/`)`/`;;` syntax. **The `case` keyword is dropped**; `case` becomes a
  reserved-word diagnostic pointing at the `[[ … ]]` dispatch form.
- `;&`/`;;&` fall-through/resume: the `SemiAmp`/`DblSemiAmp` tokens,
  `CaseItem.fallthrough`/`resume`, and the resume/fall-through wiring in
  `lower_case` (`build_check_blocks` resume targets, `wire_case_exits`).
- The `=glob` test op and `emit_glob`; **`sash_fnmatch`/`fnmatch_impl`**
  (string glob-match) — `case`/`[[ ]]`'s only user (after DD#62 removes the
  `[[ ]]` `==` unquoted-glob).

## Kept

- **Filename globbing is untouched** — it is a *separate* function
  (`sash_glob`, getdents readdir, `sash-rt/src/lib.rs:2354`) used by
  `arr=(*.sh)`. DD#66 only removes glob as a *string-matching* mechanism;
  glob remains for *filenames* (`*`/`*.ext`), which was always its intent.

## Lowering (reuses the case machinery)

`ir::Case` is already `Vec<(patterns, body)>` and `build_dispatch_chain`
(`transform/control_flow.rs:651`) already emits, per arm,
`TestExpr{ value, OP, pattern } → Branch{ body, next }`. DD#66 reuses this
wholesale, carrying `==` or `=~` as `OP` instead of `=glob`. The IR may be
renamed `Cond`/`Dispatch`, but the SSA dispatch chain, the per-arm test,
the merge/phi handling, and the catching-construct `$?` reset all survive.
The work is: generalise `[[ ]]` *parsing* (recognise the arm/`||`/`{…}`
grammar), swap the op, delete the `case` surface + `=glob`/`fnmatch` +
`;&`/`;;&`.

## Converter

`sash-convert` rewrites bash `case $x in a) … ;; b|c) … ;; *) … esac` into
the dispatch form, **split by risk** (DG, 2026-07-06 — see
`notes/tasks/converter-case-to-dispatch.md`):

- **All-literal case → auto-rewrite, compilable.** Single-literal arms
  and literal `a|b|c` alternation arms → a `==` dispatch (multi-exact
  `== a b c` for alternation, DD#71); `*)` → `|| { … }`; arm bodies get
  their inner conversions; an informational `# sash-convert:` comment is
  emitted.
- **Any glob-metachar arm (`*`/`?`/`[…]`) or `;&`/`;;&` fall-through →
  comment + keep original.** The converter does NOT auto-write regex: it
  emits the best-effort anchored-`=~` dispatch (`*.sh`→`^.*\.sh$`, etc.)
  as a `# sash-convert:` **comment** and leaves the original `case` in
  place, which fails to compile (SB4029). This deliberate fail-loud
  forces a human to review the converter-written regex rather than
  silently trust it.
- MANUAL annotation stays only for genuinely unparseable input.

One dispatch states a single op, so a compilable dispatch never mixes
`==`/`=~`; under split-by-risk any glob presence routes the whole
construct to the comment path anyway.

## Supersedes / relates

- **Supersedes** the case-as-regex idea and the `case`-glob P0 bug fix-(a)
  (`bug-case-pattern-glob-not-validated`): the bug dissolves because
  `case` (and its glob) no longer exist.
- **Subsumes** `bug-janet-case-multipattern`: the Janet case emitter is
  rewritten for the dispatch form (it becomes a regression gate).
- **Closes** DD#63's open "desugar `case =glob` to a glob/match builtin."
- **Builds on** DD#62 (`==` exact) and DD#63 (`=~`/`match` regex,
  unanchored).

## Resolved (DG, 2026-06-21)

- **Numeric dispatch dropped entirely.** `==` and `=~` are the **only**
  dispatch ops — there is no `-eq`/`-lt`/… dispatch form, now or later.
  Exact numeric dispatch still works through `==` (string-exact: `[[ $n ==
  1 { … } 2 { … } ]]`, with an `int n` quoted as `"$n"` — see the
  2026-10-25 addendum); numeric *ranges*/comparisons use an `if`/`elif`
  chain with `(( ))`. This keeps the construct to two clean modes.
- **Merge-label name → `dispatch.merge`** in `is_catching_construct_exit`
  (siblings: `case.merge`, `if.merge`, `while.exit`). Implementation detail.
- **SB codes assigned at implementation** (next-free; validate-level is at
  SB5028, so SB5029+). Pre-numbering now would risk colliding with
  intervening work. The two codes: the `case` reserved-word diagnostic and
  the "dispatch `[[ … ]]` used as a condition" (statement-only) diagnostic.

See task `dd66-cond-form-dispatch` for the spec-first implementation.

## Considered and declined — no glob-in-`==` diagnostic (DG, 2026-09-30)

Raised after `cov.345` was first written as `[[ $p == /* ]]`, i.e. bash. Under
this DD `==` is strict exact equality, so that compares `$p` against a literal
two-character string. The divergence is silent: with `p=abcd`,
`[[ $p == ab* ]]` is true in bash and false in sash, `!=` inverts the same way,
and nothing is emitted. SB5006 already rejects an unquoted `?` or `[…]` in the
same position for precisely this porting hazard, so the asymmetry was the
argument for extending it to `*`.

**DG declined.** Two reasons, in his words: the project does not carry warnings
or lint messages, so the only available mechanism is *rejection* — and that
would reject an entirely legal expression solely because bash reads it
differently. He agreed with the intent (avoid our own footguns) but judged this
application too aggressive. He reserves the right to revisit on user feedback.

The asymmetry with SB5006 therefore stands, and it is deliberate. Note that
SB5006's cases are not the same shape: `?` and `[…]` in an unquoted word are
diagnosed for reasons of their own, whereas a literal `*` is a perfectly
ordinary string to compare against. **Do not re-propose this without new
evidence that a real user was bitten.**

## Addendum (2026-10-25) — an int subject or pattern is quoted

Dispatch matches strings, so DD#30's rule (addendum of the same date) applies
to it as to the boolean form: a whole-int subject or pattern is SB4030, "int
value in a `[[ ]]` dispatch, which matches strings", with the same advice to
quote it. The numeric dispatch in *Resolved* above is unchanged for a string
`n`; an `int n` is written `[[ "$n" == 1 { … } 2 { … } ]]`, and an int
pattern `$m` as `"$m"`. Matching is still exact string equality, so `"$n"`
matches `1` and not `01`, as before. Specs `spec/types.sh` `30.57`, `30.58`
and `30.65`; journal 535.

## Addendum (2026-10-26): every boolean position, not only a header's list

*Composition* above makes the dispatch form statement-only: SB4028 for a
dispatch as a condition, as the operand of `!`, or as an operand of
`&&`/`||`. It was enforced for a dispatch standing as a statement of an `if`,
`while` or `until` condition, and directly under `!`. Everywhere else it
compiled, and its always-`0` status made the chain succeed, which is the
footgun the rule exists to remove. Each of these printed the dispatched `d`,
then ran the body or the rest of the chain:

- behind a `-` in a condition (`cnd.8`);
- as an `elif` condition, in the brace spelling (`cnd.9`) and the keyword one
  (`cnd.19`);
- as an `&&`/`||` operand, in a condition (`cnd.10`; `while a || [[ … ]]`
  entered its body) or at statement level (`cnd.15`, and behind a `-`,
  `cnd.16`);
- as a `return` producer (`cnd.14`) or an operand in one's list (`cnd.17`).

`! - [[ … ]]` was refused, but by the `-` (SB4019, `cnd.13`). These are
enforcement bugs, not a new rule. One total walker, `dispatch_in`, finds a
dispatch under a `-`, a `!`, a redirect or a `return`. The `!` parse, the
`return` parse and the `&&`/`||` operand parse call it, and the shared
`condition_shape_fault` applies it to every statement of a header's list,
through `&&`/`||` operands and pipeline stages, for `if`, both spellings of
`elif`, `while` and `until`. That function carries DD#68's `{ }` rule too.

*Decide-and-report:*

- A `return` producer is a boolean position. Its status becomes the
  function's, which the caller's condition then tests, so
  `fn f { return [[ $1 == a { … } ]] }; if f a { … }` is the same footgun one
  call away.
- The diagnostic now points at the dispatch's `[[`. The positions enforced
  before pointed at the header's keyword, so `if [[ $x == a { … } ]] { … }`
  moves from column 1 to column 4. Directly under `!` it keeps the `!`'s
  position, as before.

Unchanged: a dispatch on the left of `&&`/`||` is SB4023 and a dispatch as a
pipeline stage SB4022 (both are compound-command rules). A dispatch that is a
capture's whole body is a statement in a body (DD#86) and stays legal
(`cnd.18`). The same capture as a condition is SB4025, as before, because a
capture condition takes only a single external command or `$(getenv NAME)`.

Specs: `spec/errors.sh` `cnd.8`-`cnd.10`, `cnd.13`-`cnd.19`. Journal 537.
