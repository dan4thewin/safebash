title: DD#65 — Prune the ${var:OP} modifier family
status: accepted

1. `:-` defaults, `:?` messages and `arg N DEFAULT` take literal operands.
   A dollar in a disposition operand requires whole-operand single quoting;
   `arg` rejects expansions structurally.
2. `:-` selects its default for the subject's meaningful zero: empty string
   or integer zero. Integer defaults are integer literals. Arithmetic string
   conversion uses DD77's validity-based disposition instead.
3. An omitted or empty `:?` message identifies the subject and applicable
   failure; a nonempty custom literal keeps its text.
4. Length, substring and regex substitution remain distinct operations.
   Substitution fields are interpolating words, not disposition defaults;
   `basename`/`dirname` are compiler sugar without an external dependency.
5. Unknown parameter suffixes and nameless braced expansions are errors,
   never silently discarded text. Scalar and cell spellings share this rule.

---

The `${var:OP}` family had accumulated bash creep. Vetted 2026-06-18
(task `vet-var-modifiers`). Kept only what earns its place; the rest is
dropped or moved into the awk-style string-builtin family
(`match`/`sub`/`gsub`/…, DD#63).

## Kept

- **`${var:-default}`** — default if unset/empty. Pure, useful, no glob.
  ~~The default is **exactly a double-quoted-string body** (`:- ≡ "…"`, DG
  2026-07-04): `$name`/`${name}`/`$(…)`/`$((…))`/`$'…'`/`` `…` `` interpolate
  and concatenate; **every other byte is literal**, including whitespace
  (kept verbatim) and quotes (`'` and `"` are ordinary literal bytes — they
  do **not** strip or group). The terminator is the closing `}`: the first
  unnested literal `}` closes the expansion (the brace scanner is pure
  depth-matching — quotes no longer protect a `}`), so `${n:-'a}b'}` is
  `${n:-'a}` (default `'a`) followed by literal `b'}`. To embed a literal
  `}` (or any byte) use a `$'…'` C-string, e.g. `${n:-$'a\x7db'}` → `a}b`
  (the `\x7d` carries no `}` byte, so it never trips the matcher). Taint
  flows normally: `${n:-$clean}` clean, `${n:-$tainted}` tainted (bless
  first); a bare env var is rejected (SB5018 — it is a real interpolation
  context, so use `$(getenv HOME)`); `$(external)` is tainted, `$(user_fn)`
  and `$((…))` are clean.~~ The final literal-only follow-on below governs.
- **`${var:off:len}`** — substring. Useful, no glob/regex.
- **`${var:s/re/sub/[g]}`** — regex replace; per DD#63 this is sugar for
  `$(sub|gsub var "re" "sub")` in the shipped model. DD132 provisionally
  adds a boolean beside substitution text: the sugar explicitly projects
  text and discards that boolean, while every dynamic pattern still needs
  proof for its final immutable value. Mutation sugar remains resultless
  and cannot write immutable scratch. The trial is not implemented.

(Implemented 2026-07-04: `${x:-$y}` now interpolates via the `:- ≡ "…"`
model above — a `DoubleQuoted` operand lowered to `SsaValue::DefaultValue`
across native/c/janet. The `:s/` operand interpolation is DD#63's
textual-first-pass; `:off:len` takes integer operands.)

## Dropped (reject with a diagnostic pointing at the replacement)

- **`:=` `:+` ~~`:?`~~** — broken today and each redundant: `:=` is
  `var=${var:-default}` (and mutates inside a read — a side-effect wart);
  `:+` is `[[ -n $var ]] && …`; ~~`:?` is murky under sash's no-unset model
  (undeclared already reads empty), collapsing to a plain emptiness test.~~
  DD77 restores `:?` in word and arithmetic contexts.
- **`#` `##` `%` `%%`** (prefix/suffix strip) — real ergonomic value, but
  the community reaches for **`basename`/`dirname`**, not the strip ops,
  so they don't earn the muscle-memory exception. Use `basename`/`dirname`
  (see open question) or `sub`/`gsub`.
- **`/` `//` `/#` `/%`** (glob replace) — redundant with `sub`/`gsub`,
  glob holdover (DD#62), and the only var-op that used `\`
  (`find_unescaped_slash`). Dropping it makes DD#64 fully clean for the
  family.
- **`^` `,`** (first-char case) — niche bash curio.

## Implemented — the drops reject SB4034 (2026-07-04)

All dropped operators now reject at parse with **SB4034**
(`is_dropped_param_operator` + `check_rejected_param_expansion`,
`sash/src/parse/parser.rs`). One uniform code, one message pointing at the
string-builtin families generically (DG 2026-07-04): *"…left the safe
subset — use the string builtins (toupper/tolower, sub/gsub,
basename/dirname) or the kept forms :- / :off:len / :s/"*. Per-op
migration is **sash-convert's** job, not the compiler's (task
`converter-var-modifier-migration`).

Dropped set = every `split_operator` op except `:-`: `:=` `:+` ~~`:?`~~ `#`
`##` `%` `%%` `^` `^^` `,` `,,`, plus the `/`-family glob-replace. The
`/`-family is remapped to an internal `glob_replace` operator at parse
(`parse_standard_operator`) so it is **distinguishable from the kept `:s/`
sed op**, which shares the raw `/`//` operator strings — SB4034 targets
`glob_replace`, leaving `:s/` (`/`//`) untouched. `split_operator` still
*recognises* the dropped ops (so they reject explicitly rather than
silently degrading `${x#a}` → `$x`). Spec: `spec/var_modifiers.sh`
vm.dr1-15 (one per operator string), plus migrated `misc.sh` 52.6a/b and
`wordsplit.sh` 39.4.

**Per-element array forms dropped too** (DG 2026-07-04, consistency): the
`${arr[@]OP}` forms (operator `arrvals_<op>`, `parse_subscript_operator`,
formerly DD#33) carried the same dropped ops mapped over elements — kept
would be an asymmetry (scalar rejects, array works). They now also reject
SB4034 (`is_dropped_param_operator` matches `arrvals_<op>`; the message
shows `[@]<op>`). The replacement is a **`for x in @a` loop with
`$(sub …)`/`$(gsub …)`** — migrated `types.sh` 33.16-33.18 to that form,
which also *fixed* those tests in the c/janet backends (they never
supported per-element array ops). Reject specs: `var_modifiers.sh`
vm.dr16-18. The now-dead per-element codegen (`apply_per_element_op`,
`per_elem_regex_replace`/`_strip_prefix`/`_strip_suffix`) was removed.

Journal 519 removes the leftover native interner's NUL-split support for that
retired family. Native dispatch now accepts only the actual joined-value
operators, `arrvals` and `arrjoin`, matching C: a malformed `arrvals_<op>` must
reach the existing loud refusal, not silently become an ordinary join.

## Moved to builtins

- **`^^` `,,`** (full upper/lower) → **`$(toupper s)` / `$(tolower s)`**
  builtins (awk names, consistent with `match`/`sub`/`gsub`). These also
  give case-insensitive matching, which the regex engine can't (no `i`
  flag): `match $(tolower $x) $(tolower $pat)`. `/usr/bin/tr` remains for
  arbitrary character translation.

## Resolved — basename/dirname (2026-06-18 → **revised 2026-06-21/07-04**)

- **basename/dirname are SUGAR** (DG 2026-06-21, reconfirmed 2026-07-04):
  the compiler rewrites `basename $p` → `$(sub $p '.*/')` and `dirname $p`
  → `$(sub $p '/[^/]*$')` (or equivalent) at compile time — **no fork, no
  taint, no DD#23 dependency**. This supersedes the earlier 2026-06-18 note
  below (no builtins / use external), which is retained only for history.
  ~~No `basename`/`dirname` builtins. Use `${var:s/…/…/}` / `sub`, or the
  external `basename`/`dirname` (DG: fork cost is fine).~~ **Caveat (moot
  under sugar):** DD#23 path-resolution is `partial`; the external route hit
  the `bare command` runtime bug + taint — the sugar rewrite sidesteps both.

## Resolved (DG, 2026-06-21)

- **`${#var}` length: KEPT.** Orthogonal to the pruned `:OP` family, O(1),
  essential. **`@X` transforms** (`${var@U}`/`@Q`/…): **stay rejected**
  (SB4015), unchanged.
- ~~**`${var:-default}` allows `$(cmd)`.** The earlier "command substitution
  is rejected in the default" was an over-restriction. Only an **external**
  capture is tainted (`$(getenv|argv|arg …)` or `$(/abs …)` —
  `cmdsub_is_taint_source`, validate.rs:1925); a **user-function** capture
  `$(my_fn)` is **clean** and useful. So `$(cmd)` is allowed; taint does
  its normal job (clean user-fn works; external needs bless, as
  everywhere). Arithmetic `$((…))` was already allowed.~~
- ~~**`:?` is rejected for SCALAR `${var:?…}` only.** The `:?` *empty-
  disposition* is reintroduced for the **array-splat** form `@{a:?WORD}`
  (DD#69) — a different construct. DD#65 drops only the scalar `${var:?}`.~~
- ~~**The default is an interpolating context** — see the 2026-07-04
  resolution below, which settled its exact shape.~~

## Resolved (DG, 2026-07-04) — the `:- ≡ "…"` model

The 2026-06-21 note framed the default as "an interpolating **word**
context … like any command argument / assignment RHS, `'…'` literal" —
i.e. the bash **word** model where single-quotes *strip*. ~~DG replaced this
with a simpler, fully-consistent rule: **the default is exactly a
double-quoted-string body** (`${var:-X}` ≡ `"X"`).~~

- **Quotes are ordinary literal bytes.** ~~`'` and `"` do not strip or group
  (`${x:-'a'}` → `'a'`).~~ This collapses the rule to one sentence and
  preserves whitespace verbatim (`${x:-a  b}` → `a  b`).
  **Superseded in part (2026-09-30 follow-on, below): a *fully*
  single-quoted operand now strips**, as the hatch for a literal `$` —
  `${x:-'a'}` → `a`, `${x:-'costs $5'}` → `costs $5`. A `'` anywhere
  else, and `"` everywhere, remain ordinary bytes, so `${x:-"7"}` is
  still the three bytes `"7"`.
- ~~**The C-string is the any-byte hatch.** Because `$'…'` decodes
  `\xNN`/octal/`\n` and is recognised inside `"…"` (DD#64 addendum), any
  byte is expressible: `${x:-$'a\x7db'}` → `a}b`. This is *why* stripping
  single-quotes is unnecessary — its only unique job (embedding a `}`) is
  already covered, without a literal `}` byte to confuse the parser.~~
- **Consequence — quotes are plain in the brace matcher too.** For `'` to
  be genuinely plain, the lexer's `${…}` brace matcher (both the bare
  `scan_brace_expansion` and the in-`"…"` `consume_dq_brace_expansion`)
  drops its quote/`\`-skip: the extent of `${…}` is pure brace-depth, so
  the first unnested literal `}` closes. `${n:-'a}b'}` = `${n:-'a}` +
  literal `b'}`. (Only affects an expansion with a literal `}` *inside*
  quotes; `${m["key"]}` and friends are unchanged, having no interior `}`.)
- ~~**Divergence from bash** (accepted): bash strips quotes in `:-` and
  quote-protects a `}`; sash does neither. The C-string replaces both.~~

Lowering: the operand lowers to `SsaValue::DefaultValue { var, default }`
and both backends select var-or-default. (Stale as written: the operand
parsed to an `ExprKind::DoubleQuoted` only until the 2026-09-30 follow-on
retired the interpolating operand — `parse_disposition_literal` now yields
an `ExprKind::Literal` unconditionally. The janet backend named here was
removed by DD#104.)

## See also
- DD#63 — regex/string builtin family (`match`/`sub`/`gsub`, +`toupper`/`tolower`).
- DD#64 — no shell backslash (the `/`-op `\/` escape goes away with it).
- DD#62 — drop glob (the `/` glob-replace is a holdover).
- DD#61 — only *restricts* `#`/`%` for taint; does not drop them (this DD does).

## Addendum (2026-08-29) — int `:-` = identity-element default, literal-only

DG ratified two coupled refinements to the `${var:-D}` default when the subject
is **int**-typed (a scalar `int` or an int-array/map cell):

1. **Identity-element trigger.** `:-` triggers the default on the *identity
   element* of the subject's type: `""` for strings (existing), **`0` for ints**
   (new). So `${int_x:-D}` ≡ `(x == 0 ? D : x)`. DD#62's "meaningful zero" is
   preserved — `:-` merely *chooses* 0 as its trigger (the additive identity, the
   int analogue of the empty string), op-locally. It does **not** make 0 "unset".

   This corrected a latent inconsistency: all three backends had been applying a
   *string-emptiness* test to an int. Native returned `0` for `int x=0;
   ${x:-99}` (int→string "0" has length 1 → "non-empty" → default never fired);
   c/janet failed to compile (`x_ptr`/`(length x)` on an int). Now all three test
   `int == 0`. (See `bug-cjanet-default-on-int-var`, resolved.)

2. **Literal-only default.** When the subject is int, `D` must be an **int
   literal** (decimal / `0x`/`0o`/`0b`) — not a string, an interpolation
   (`$y`, `$((…))`), or empty. Rejected at type_check (**SB4030**). This unifies
   with the arithmetic-context default rule (DD#77 D4): one mental model — "an
   int `:-` default is an int literal" — and keeps the disposition homogeneously
   int, so no mixed-type ternary and no string round-trip in the value.
   (Bare `-` — default-on-*unset* — is **not** an accepted alias: like the whole
   non-colon family it is rejected SB4034, see the 2026-08-29 non-colon
   addendum below.)

Lowering: an int source lowers to `SsaValue::Ternary{ ArithBinOp{src,Eq,0} ?
default : src }` (native + c via SSA); janet emits the `(if (= name 0) …)` form
directly from the IR. Specs: coverage.sh cov.78/80/81/82/83/84/85.

## Addendum (2026-08-29) — non-colon default family rejected (`-` `+` `=` `?`)

DG: "clarity favors a single spelling. DD65 gives the answer." The non-colon
forms `${x-D}` `${x+D}` `${x=D}` `${x?D}` are dropped, same as their colon
cousins — **rejected SB4034**, no compiler coercion. (Migration `-`→`:-` etc.
is sash-convert's separate job; the compiler does not silently rewrite.)

These had been a silent-degrade gap: `split_operator` didn't recognise them, so
`${x-4}` parsed as a bare `${x}` with `-4` **discarded** — and `${x+4}` returned
`$x` instead of the alternate (a silently *wrong* answer). This is exactly the
"reject rather than silently degrade" failure DD#65's machinery was built to
prevent; the non-colon family had simply slipped through. Fix: add `-`/`+`/`=`/
`?` to `split_operator`'s one-char ops (so they parse to an operator) and to
`is_dropped_param_operator` (so they reject). The per-element array path
(`parse_subscript_operator`) is self-guarded and unaffected.

The **kept** spelling is the colon forms only: `:-` (default) and `:?` (require,
DD#77). SB4034's message now points at `:- / :? / :off:len / :s/`. Specs:
`spec/var_modifiers.sh` vm.dr19-22 (one per non-colon operator).

**SUBSET.md deleted (same day).** It had claimed "non-colon forms are silently
coerced to colon forms" (contradicting this decision) and still listed the
DD#65-dropped `:=`/`:+` as supported — drifted from the DDs and actively
misleading. DG: "remove SUBSET.md, it's worse than useless." The safe subset is
defined by the DDs in `notes/decisions/`.

### Follow-on (2026-08-29) — no-ambiguity fallthrough (SB4047)

DG: "need to prevent ambiguity — if the compiler doesn't know what something
does, that's very bad." Generalised the non-colon fix: `parse_parameter_expansion`
used to fall through to a bare `${name}` whenever trailing content matched no
operator parser, **silently dropping** that content (`${x&y}`, `${x:foo}`,
`${x.y}`, `${x:}`, `${x y}` all degraded to `$x`). Now the fallthrough carries
the raw remainder on a sentinel `unrecognized` operator and rejects it —
**SB4047** ("the compiler does not recognise this parameter operator and will
not silently ignore it"). Two-layer model: SB4034 = a *known* dropped operator
(specific "use :- / builtins" guidance); SB4047 = *genuinely unknown* content.
The kept forms (`:-`/`:?`/`:off:len`/`:s/`, subscripts, `@`-transform SB4015,
`!`-indirect SB4003, strlen) are all recognised earlier, so only real garbage
reaches the reject. Specs `spec/var_modifiers.sh` vm.un1-4.

**Cell enforcement, 2026-10-16.** The cell path had retained its own silent
fallback: `${a[0]garbage}` and `${a[0]&junk}` read the cell after discarding the
suffix, including inside arithmetic. It now uses the same unknown-content
sentinel as the scalar path and rejects with SB4047. Specs `celop.1`-`celop.4`
cover arrays, maps, quoted words and arithmetic. Journal 512.

### Addendum (2026-10-24, batch-531 port step F14) — a braced expansion with no name is SB4047

The fallthrough above catches unknown content *after* a name; nothing caught a
*missing* name. A body's name is its leading `[A-Za-z0-9_]*` run, which may be
empty, so `${}` and every `:`-led operator were built on a variable called ``.
On the base binary (batch-531 B70, the word half):

- `${}` printed nothing and `${:-d}` printed `d`, in both backends;
- `${:?m}` compiled, then aborted at run time with `m`;
- `${:0:1}` reached a native codegen ICE ("no codegen slot"), and C printed an
  empty string;
- `${:~}`, `${:!}` and `${:s/a/b/}` read an empty value natively, and emitted C
  that gcc rejects;
- inside arithmetic, `$(( ${} + 1 ))` and `$(( ${:-1} ))` both printed `1`;
- as an index, `${a[${}]}` drew SB4030's advice to convert a variable called
  `$`.

**The rule.** A braced expansion whose content is empty or starts with `:`
names no variable. It takes the `unrecognized` sentinel and is **SB4047**,
"`${…}` names no variable — a braced expansion starts with a variable name".
A name run emptied by any other byte already reached the sentinel; `${&}` read
"unrecognized content `&` after ``" and now reads the same as the rest.

Empty-name spellings that were already rejected under their own codes keep
them, unchanged on both binaries: `${#}`, `${?}` and `${-}` are dropped
operators (SB4034), `${[0]}` has no collection to subscript (SB4047, its own
message), `${!}` is indirection (SB4003), and `${@}`/`${*}` are splats
(SB5045).

Code: `names_no_variable`, checked first in `try_leading_form`
(`parse/expr.rs`), and the empty-name arm of `unrecognized_operator_diag`
(`parse/parser.rs`). Specs: `vm.nn1`–`vm.nn8` (var_modifiers.sh),
`31.176`–`31.177` (arithmetic.sh) and `80.135` (subscript.sh). A stray `$` in
arithmetic is DD#72's addendum of the same date. Journal 533.

### Follow-on (2026-09-30) — `$` in a `:?` abort message is SB4070

The `:?` abort **message** is a literal, and a `$` in it is now rejected
(**SB4070**) unless the *whole* message is wrapped in `'…'`, in which case the
quotes are stripped and the `$` is kept as an ordinary byte:

```
str z=${y:?$m}           # SB4070
str z=${y:?pre $m post}  # SB4070 — anywhere in the message, not just leading
str z=${y:?plain text}   # ok
str z=${y:?'costs $5'}   # ok — aborts with:  ./prog: costs $5
echo @{a:?$(g)}          # SB4070 — the array form obeys the same rule
```

**Default diagnostic (DG, 2026-11-10; implementation pending).** An omitted
or empty `:?` literal message will get a generated diagnostic naming the
subject and its applicable failure (DD77). Nonempty custom messages retain
their text. This does not reintroduce interpolation or change `:-` defaults;
`default-require-diagnostics` owns the work.

**Why the message is not an interpolating word.** It was, briefly, and that was
a mistake worth recording. DD#78 groups `:-` and `:?` operands together, but it
groups them by *purpose* — a purity constraint on what the operand may contain.
The **parse** belongs to this DD, and the interpolating-word parse above is
defined for `:-` alone. Reading DD#78's grouping as settling the parse made
`${y:?$(g)}` a live user-function call site. Two consequences, the second
worse than the first:

- an abort message could run arbitrary user code on its way out, and `$(( … ))`
  is a call site too (DD#58's arith sexpr resolves `(g …)` to a user function),
  so "only pure reads" is not a rule that can be stated, let alone checked;
- because the message is materialized **eagerly**, `str y=val; str z=${y:?$(g)}`
  forked and exec'd `g` on the path where `y` was set and **no abort happened**.
  Exit 0, no output, and user code ran.

DG: "`:?` is an abort. The code is exiting. Treating MESSAGE as an
interpolating word means more code can run. Naming it a disposition default
doesn't equate it to `:-` but rather groups it by purpose." And on the remedy:
"a `:?` is a failsafe, so a literal is probably adequate. If I anticipate an
empty array, I can use a conditional to get whatever behavior I desire,
including verbose messages."

**Why the plain revert was not enough.** A literal message degrades *silently*:
`${y:?$m}` is valid bash, reads as an interpolation, and quietly emits the two
characters `$m`. That is exactly the failure SB4047 exists to prevent, so the
same doctrine applies — reject rather than silently degrade. `'…'` is the
explicit way to say "I meant the character".

**Both spellings, one helper.** `parse_abort_message` serves the scalar
`${x:?MSG}` and the splat `@{a:?WORD}`. The array form had been the *more*
permissive of the two the whole time — `optional_splat_word` called
`parse_default_word`, so `@{a:?$(g)}` ran `g` and printed its return value as
the message, and it kept doing so after the scalar form was reverted. A fix
that touches one spelling of a shared rule is half a fix.

Scope is `$` only, as DG specified. Note the brace scanner is pure
depth-matching (above), so quotes do not protect a `}`: `${y:?'a}b'}` still
terminates at the first `}`.

Specs: `vm.req4`-`vm.req10` (var_modifiers.sh), `dd78.8`-`dd78.12` (taint.sh),
`gs.56`-`gs.58` (glob_splat.sh, plan 58 -> 61).

## Follow-on (2026-09-30) — `:-` is a literal too, and so is `arg N DEFAULT`

The rule above is no longer `:?`-specific. DG, on reading the `:?` account:

> the `:-` default, let's change it to match the `:?` so eager/side-effects are
> a non-issue.

So **all three disposition defaults take a literal operand**, and a `$` in one
is SB4070 unless the whole operand is single-quoted:

```sh
str v=${x:-plain}          # ok
str v=${x:-'costs $5'}     # ok — the value is:  costs $5
str v=${x:-$y}             # SB4070
str v=${x:-$(f)}           # SB4070
echo @{a:-$t}              # SB4070 — the array form, same rule
x=$(arg 5 $t)              # SB4070 — the `arg` form, same rule
```

**The `arg` form is checked structurally, not textually.** The `${ }` and
brace-less spellings are rejected in the parser, on the operand's raw text.
Nothing knows a word belongs to `arg` until validate, so that form is rejected
there instead, by walking the parsed expression for any expansion
(`first_expansion`). The single-quote escape falls out for free: a
`SingleQuoted` carries no expansion to find. The code is the same, SB4070,
because the rule is the same rule.

**What this retires.** The interpolating-word operand was the *only* reason
these existed, and all of them are now deleted:

- the eager/lazy hybrid lowering of `:-` (`lift_default_value`'s capture
  detection, and the phi-merged control flow added for bash parity), and its
  specs `lift.4`-`lift.10`;
- **SB5035** — the `$(random)` ban. It existed because an eagerly-materialized
  default advanced the PRNG on the path where the default was not used. A
  literal cannot invoke `random`, so the ban is unreachable by construction;
- DD#78's read discipline for `:-` (see that DD's final amendment). The taint
  hole it closed with a *checker* is now closed by the grammar.

DG named the underlying cause: "I think the eager business came as a way to
avoid a taint bypass." Close — the eagerness was pre-existing branchless-select
codegen, and SB5035 and the read discipline were both consequences of it. Once
the operand cannot contain an expansion, "when is it evaluated" and "what may
it do" both stop being questions.

**What is deliberately *not* in scope.** `${x:s/RE/SUB/}` remains a full
interpolating word. It is a *pattern*, not a disposition default: it is
evaluated unconditionally, so neither the laziness question nor the
conditionally-taken-side-effect question arises. `parse_interpolating_word` (the
old `parse_default_word`) survives for it alone.

**Known expressiveness loss: a literal `}` in a default is unrepresentable.**
The `$'a\x7db'` C-string hatch contains a `$`, so it is SB4070; and the brace
scanner is pure depth-matching, so quoting does not protect a `}` either
(`${y:-'a}b'}` terminates at the first `}`). This was already true of `:?` and
now extends to `:-`. Use a conditional. Pinned by `vm.dv2`.

Specs: `vm.dv*`/`vm.bl*` (var_modifiers.sh), `61.1`-`61.7` / `dd78.1`-`dd78.7` /
`91.24`-`91.33` (taint.sh), `bkstp.5`/`7`/`11`/`12` (taint_switch.sh), `gs.11`,
`gs.45`, `av.d4`, `av.d7`, `ud.13`, `31.47`, and `cov.92`/`190`/`220`/`223`/
`224`/`225`/`254`.
