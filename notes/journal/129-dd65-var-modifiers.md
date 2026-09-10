# 129 — DD#65 var modifiers: builtins, sugar, and the `:-` word

DG-directed, spec-first. The DD#65 prune is an atomic drop+rebuild; this
journal covers the **rebuild** half (the kept/moved constructs). The
**drops** (`:=`/`:+`/`:?`/`#`/`##`/`%`/`%%`/`/`-family/`^`/`,`) are the
next and final stage.

## Stages shipped

1. **toupper/tolower builtins** (896a424) — `$(toupper s)` / `$(tolower s)`,
   the awk-named replacements for the moved `^^`/`,,`. `BuiltinCmd` +
   `SsaValue::CaseConvert { subject, upper }`, reusing the existing
   `sash_toupper_all`/`sash_tolower_all` runtime and `emit_case_conversion`.
   Janet uses `string/ascii-upper`/`-lower`.

2. **basename/dirname sugar** (bdcac8f) — rewritten at compile time to a
   fixed-pattern `sub` (`.*/` and `/[^/]*$`, empty replacement), reusing the
   whole `RegexReplace` path. No fork, no taint, no DD#23 dependency.

3. **`${var:-DEFAULT}` full interpolation** — this session's substantive
   piece. Detailed below.

## The `:-` word — `:- ≡ "…"`

The literal-only gap (`${x:-$y}` yielded literal `$y`) is closed. The
design went through several DG refinements and landed on the simplest
consistent rule:

> The default is **exactly a double-quoted-string body**. `${var:-X}` ≡ `"X"`.

- `$name`/`${name}`/`$(…)`/`$((…))`/`$'…'`/`` `…` `` interpolate and
  concatenate; **every other byte is literal**, including whitespace (kept
  verbatim: `${x:-a  b}` → `a  b`) and quotes (`'` and `"` are ordinary
  literal bytes — no strip, no group: `${x:-'a'}` → `'a'`).

### Why quotes don't strip — the C-string is the any-byte hatch

The only unique job single-quote stripping did in bash's `:-` was to embed
a literal `}` (`${n:-'a}b'}` → `a}b`). But `$'…'` already decodes
`\xNN`/octal/`\n` and is recognised inside `"…"` (DD#64 addendum), so
`${x:-$'a\x7db'}` → `a}b` — **and** the `\x7d` carries no literal `}` byte
to confuse the parser. With the any-byte need already covered, stripping
earns its keep only as bash-familiarity, which sash doesn't owe. So DG
chose "quotes are plain," collapsing the rule to one sentence and
preserving whitespace exactly.

### Consequence — quotes are plain in the brace matcher too (DG's catch)

DG spotted the inconsistency in the first cut: if `'` is a plain byte in
the value, it can't also be a *quote* to the lexer's `${…}` brace matcher.
Absorption doesn't rescue it — `AbsorbMode` is a parser/token-stream rule
for `[[ ]]`/assignment words, but `${…}` is brace-matched by the **lexer**
into one token before any parsing. So for `'` to be genuinely plain, both
brace matchers (`scan_brace_expansion` and the in-`"…"`
`consume_dq_brace_expansion`) drop their quote/`\`-skip: the `${…}` extent
is now **pure brace-depth**, and the first unnested literal `}` closes.

    ${n:-'a}b'}   ==>   ${n:-'a}   (default 'a)   +   literal  b'}

Blast radius is only an expansion with a literal `}` *inside* quotes (grep
found zero such specs). `${m["key"]}` and friends are unchanged — no
interior `}`. The C-string route survives untouched (`$'a\x7db'` has no
`}` byte, so the matcher walks straight past it).

## Lowering

`parse_default_word` re-scans the operand as a `"…"` body (a shared
`scan_interp_body(stop_at_dquote)` factored out of `scan_double_quoted`)
and yields an `ExprKind::DoubleQuoted`. The transform builds
`SsaValue::DefaultValue { var, default }` (default is a full interpolating
value, unlike the flat-string `ParamExpansion` operand). Native + c lower
var and default to `(ptr,len)` and `select` on the var's length (reusing
`sash_default_value`); janet already interpolated at the IR level
(`emit_pe_default`), so it needed no change. The new variant is covered in
`child_values`, `scan_value_for_vars`, `value_is_tainted` (tainted if the
var **or** the default is), and both backends.

## Verification

- spec/var_modifiers.sh = 24 (tu1-7, bn1-6, dv1-11), green in
  native/janet/c.
  - dv2 = the C-string `}` hatch; dv8 = quotes literal; dv9 = exact
    whitespace; dv10 = first `}` closes; dv11 = bare env var in a default
    rejects SB5018 (negative of dv3, confirms it's a real interp context).
- Authoritative latest-run-per-test spec.db diff (repo-root `spec.db`, not
  `spec/spec.db`): **native 0, c 52, janet 74 — unchanged**. 0 regressions
  from the shared brace-matcher change.
- Edge checks: `${foo:-$(echo hi)}` → `hi`; `${foo:-${bar:-x}}` → `x`.

## Method notes

- Spec-first throughout (DG reminded twice mid-session).
- `spec.db` is at the **repo root**; querying `spec/spec.db` hits a stray
  0-byte file the sqlite shim leaves behind — use the absolute repo-root
  path, and python3/`sqlite3 <abs>` for the diff.
- DD#65 doc rewritten to the `:- ≡ "…"` model (the earlier "interpolating
  word context, `'…'` literal" framing described the bash word model and is
  now superseded).

## Next

DD#65 **drops** — reject `:=`/`:+`/`:?`(scalar)/`#`/`##`/`%`/`%%`/
`/`-family/`^`/`,`, remove them from `split_operator` + all backend
dispatch, spec-first, one commit (no transitional diagnostics).

## Drops stage — SB4034 uniform reject (2026-07-04)

DG-decided shape (three questions answered): **one** uniform diagnostic
(not per-op, not multiple codes); the message points at the string-builtin
families **generically**; **sash-convert owns the replacements** (filed
`converter-var-modifier-migration`); all pruned ops are treated as one
class of parse-time reject.

- **SB4034** in `check_rejected_param_expansion` (now split into
  `rejected_param_operator_diag`, the SB4003/SB4015/SB4034 chokepoint that
  runs over the whole program from `parse`). `is_dropped_param_operator` =
  every `split_operator` op except `:-`. Message: *"the 'OP' parameter
  operator (in ${x…}) left the safe subset — use the string builtins
  (toupper/tolower, sub/gsub, basename/dirname) or the kept forms :- /
  :off:len / :s/"*.
- **`split_operator` still recognises the dropped ops** — removing them
  there would silently degrade `${x#a}` → `$x` (a silent miscompile). They
  parse into a `ParameterExpansion` and reject explicitly instead.
- **The `:s/` collision** (the one real subtlety): the kept `:s/` sed op
  and the dropped `/`-family glob-replace both lowered to operator `/`//`.
  Rejecting `/`//` would kill `:s/`. Fix: remap the **dropped** glob path
  to an internal `glob_replace` operator at parse (`parse_standard_operator`),
  leaving `:s/`'s `/`//` untouched for codegen. (First tried renaming the
  *kept* `:s/` → `sed`/`sed_g`; that cascaded into the literal-collection
  pass — `intern` keys off the operator — and ICE'd `:s/`. Renaming the
  dropped side, which never reaches codegen, is the clean separation.) The
  diagnostic maps `glob_replace` → `/` for display.
- Fixed the stale **SB4015** message (it named `^/^^/,/,,`, now dropped →
  points at the `$(toupper)`/`$(tolower)` builtins).
- Spec: `spec/var_modifiers.sh` vm.dr1-15 (one per operator string, so the
  reject set can't silently miss one), plan 24→39. Migrated the two
  accept-tests that now reject: `misc.sh` 52.6a/b (`${x^^}`/`${x,,}`),
  `wordsplit.sh` 39.4 (`${x/world/earth}`).
- Verification: native **0 regressions**; janet **0**; **c improved by 1**
  (c:39.4 was failing — c never supported `/`-glob in value position — now
  it's a reject-test c passes). vm.dr1-15 green all three modes; `:s/`
  (39.3) still green native/janet.

Deferred to a **follow-up**: the now-dead backend dispatch for the dropped
ops (native `emit_strip_prefix`/`emit_strip_suffix`/`emit_alternate_value`,
the c/janet equivalents, `emit_pattern_replace`'s dropped `/#`//%` arms)
is unreachable (parse rejects first) but still present. Removing it is a
fail-loud + cleanliness improvement; done as its own commit to keep the
reject diff reviewable.

## Dead backend dispatch removed (2026-07-04, follow-up commit)

Done (was "deferred" above). The dropped ops reject at parse, so their
backend arms were unreachable. Removed, build-guided by `pub(crate)`
dead-code detection (remove arm → build → remove the helper it orphaned):

- **native** (`codegen/value_handlers.rs`): `emit_string_op_expansion`
  reduced to `substring` + `/`//` (`:s/`) + an ICE fallback (fail-loud if a
  reject ever regresses); dropped the `pat_ptr`/`pat_len` params and the
  caller's now-needless `intern_literal_value`. Removed `emit_strip_suffix`,
  `emit_strip_prefix`, `emit_alternate_value`. Kept (shared): `emit_default_value`
  (DefaultValue), `emit_case_conversion` (CaseConvert), `emit_pattern_replace`
  (`:s/`), `emit_substring`.
- **c** (`emit_c.rs`): `emit_param_expansion` reduced to the array/glob/
  strlen/substring set; removed `emit_strip_suffix_c`, `emit_strip_prefix_c`,
  `emit_default_value_c` (old operand-string form), `emit_alternate_value_c`.
  Kept `emit_default_value_expr_c` (DefaultValue) and `emit_case_conversion_c`
  (CaseConvert). c `:s/` stays a TODO stub (pre-existing gap, c:39.x).
- **janet** (`emit_janet.rs`): dropped the `%`/`%%`/`#`/`##`/`^`/`^^`/`,`/`,,`
  arms; removed `emit_pe_suffix_strip_longest`/`_shortest`,
  `emit_pe_prefix_strip_longest`/`_shortest`, `emit_pe_case_op`. Kept
  `emit_pe_default`, `emit_pe_substring`, `emit_pe_replace` (`:s/`).

0 behavior change (all removed code unreachable): native 0 / c 51 / janet
74, `:s/` (39.3) still green. Dispatchers simplified well under the
complexity gate.

## Per-element array forms dropped too (2026-07-04, DG decision)

While removing the dead backend dispatch I found `apply_per_element_op`
still using the strip runtime — which surfaced that the **per-element
array** forms `${arr[@]#p}`/`${arr[@]%%p}`/`${arr[@]/p/r}` (operator
`arrvals_<op>`, `parse_subscript_operator`, formerly "DD#33 array ops")
**bypassed the scalar reject** and still worked. Same operators, now
asymmetric. Flagged to DG (a genuine scope question DD#65's vet didn't
cover); DG chose to **extend the drop to arrays for consistency**.

- `is_dropped_param_operator` now also matches `arrvals_<op>` (the array
  parse path keeps the raw `/`//`, so a separate `strip_prefix("arrvals_")`
  arm handles it); plain `arrvals`/`arrkeys`/`arrjoin` (join) stay. The
  diagnostic shows the user-facing `[@]<op>`.
- Replacement = a **`for x in @a` loop with `$(sub …)`/`$(gsub …)`**
  (verified: strip-prefix `^p`, strip-suffix `\.txt$`, replace all work).
  Migrated `types.sh` 33.16-33.18 to that form; added `var_modifiers.sh`
  vm.dr16-18 reject tests.
- Removed the now-dead per-element codegen (`apply_per_element_op` +
  `per_elem_regex_replace`/`_strip_prefix`/`_strip_suffix` +
  `intern_literal_string`); `emit_array_join_expansion` copies items as-is.
- **The migration *fixed* c and janet**: 33.16-33.18 were in the failing
  baseline there (c/janet never supported per-element array ops); the
  loop+sub form works, so c 52→48 and janet 74→71 (native still 0). 0
  regressions.

Follow-up filed: the `sash_strip_prefix`/`sash_strip_suffix` runtime
functions + their `ExternFuncs` fields / setup declarations / emit_c
extern decls are now fully dead (both scalar and array strip gone) — a
runtime-ABI cleanup (task `remove-dead-strip-runtime`).

## Follow-up: removed the orphaned glob→regex helpers (2026-07-05)

DG asked "is `glob_replace` dead code?" — it is **not** (it's a live
reject-only sentinel: `${x/a/b}` parses to operator `glob_replace`, the
reject pass consumes it → SB4034, shown as `/`). But the question surfaced
real leftover: `parse_subscript_operator` (the array path) still ran
`glob_to_regex`/`find_unescaped_slash` to build a regex operand for
`arrvals_/`, which is now rejected before the operand is ever used — the
array parallel of the scalar `build_slash_operand` I'd already dropped.
Simplified `parse_subscript_operator` to a plain operand and removed the
now-orphaned `glob_to_regex` + `find_unescaped_slash` (no other callers).
Behavior-neutral (identical spec.db: native 0 / c 48 / janet 71).
