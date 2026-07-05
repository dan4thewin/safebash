# 129 — DD#65 var modifiers: builtins, sugar, and the `:-` word

DG-directed, spec-first. The DD#65 prune is an atomic drop+rebuild; this
journal covers the **rebuild** half (the kept/moved constructs). The
**drops** (`:=`/`:+`/`:?`/`#`/`##`/`%`/`%%`/`/`-family/`^`/`,`) are the
next and final stage.

## Stages shipped

1. **toupper/tolower builtins** (bff8f82) — `$(toupper s)` / `$(tolower s)`,
   the awk-named replacements for the moved `^^`/`,,`. `BuiltinCmd` +
   `SsaValue::CaseConvert { subject, upper }`, reusing the existing
   `sash_toupper_all`/`sash_tolower_all` runtime and `emit_case_conversion`.
   Janet uses `string/ascii-upper`/`-lower`.

2. **basename/dirname sugar** (e755fc4) — rewritten at compile time to a
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
