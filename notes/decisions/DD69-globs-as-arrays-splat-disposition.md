title: DD#69 — Globs are array sugar; external-command array splats require an empty-disposition
status: accepted

Decided with DG 2026-06-20; ratified 2026-06-21. Reframes the glob lines
of DD#20 and adds a compile-time rule for array splats passed to external
commands.

## 1. A glob is sugar for an array

> **Amended by DD#73 (2026-07-29):** a glob element is no longer the
> *verbatim* directory-entry name but a **lexically normalized absolute
> path** (`array a=(*)` in `/work` → `/work/foo`). Rationale: referential
> stability across `chdir`, and closing glob-sourced option-injection
> (`-rf` → `/work/-rf`). Everything else in this section is unchanged.

`*` (star-only, DD#39 — no `?`/`[]`/`**`) expands to an **array**, with
**one verbatim element per directory entry**: no word splitting (DD#22),
so a filename containing spaces or newlines is exactly one element. The
array is the *only* representation of a glob.

- **Array literal:** `array a=(*)` / `(*.sh)` — globs (already so).
- **for-in:** `for f in *` ≡ `array tmp=(*); for f in @tmp` — per-element
  iteration; **zero matches → the loop body never runs** (for-in's
  built-in empty handling). for-in is therefore exempt from §2.
- **Command-argument position:** inline `*` (e.g. `echo *`, `ls *.sh`) is
  a **compile error** — "globs go through arrays" (`array a=(*); … @{a:?}`).
  There is no inline glob expansion and no runtime nullglob/failglob.

Glob results are **clean**: a directory entry is a compiler-trusted value
like `$(getppid)`, so glob arrays carry no taint, do not trip SB5021, and
need no value-bless. Their only obligation is cardinality (§2).

## 2. External-command array splats state their empty behavior

The hazard "an array expands to nothing, so an external command silently
runs with too few/zero args" is general — it applies to any runtime-sized
array (glob, `read -a`, `mapfile`, `$(argv)`, or an array mutated to
empty), so the rule lives on the **splat site**, not on globs.

**Rule:** a bare `@a` splat passed as an argument to an **external
command** is a compile error. The splat must carry an empty-disposition:

- `@{a:-WORD}` — when `a` is empty, the splat is the single arg `WORD`;
  bare `@{a:-}` yields zero args (empty is explicitly fine).
- `@{a:?WORD}` — when `a` is empty, **abort** at runtime with `WORD` on
  stderr. The explicit, local replacement for failglob.

A non-empty array splats to one verbatim arg per element. These forms map
1-to-1 onto bash `"${a[@]:-WORD}"` / `"${a[@]:?WORD}"` (DD#67 sugar).

## Scope (deliberately minimal)

- **External programs only.** The rule in §2 (empty-disposition) applies to
  external programs (anything resolved to an absolute path — DD#23). The
  **sash-language constructs that splice their argv in-process are exactly
  `echo` and `:`** (`transform/resolution.rs`); `printf`, `true`, `false`
  and everything else resolve to an absolute path and are therefore
  **external programs** — a bare `@a` to `printf`/`ls`/`true`/etc. requires
  `@{a:?}` like any external program. (Earlier drafts wrongly listed
  `printf` — and `true`/`false` — as exempt.) for-in is exempt (its
  zero-match handling is built in).
- **No static-non-empty exemption.** Even `array a=(x y z); ls @a` errors;
  use `@{a:?}`. (May revisit if friction warrants.)
- **No dominating-guard recognition.** A surrounding `(( ${#a[@]} ))`
  test does not discharge the obligation — only the `@{a:…}` form does.
  Keeps the check purely local, with no flow analysis.
- **No new keyword and no array "trait."** The disposition lives in the
  splat syntax; nothing is tracked through the type system. (Rejected
  alternatives: a `gate`/`check`/`qualify` statement, or a bless-style
  cardinality obligation — both add cognitive load for no gain here.)

## 3. `echo` array splat requires an explicit join

Ratified with DG 2026-07-04. §2's *cardinality* exemption for the
argv-splicing constructs (`echo`/`:`) does **not** license `echo` to
implicitly flatten an array.

`echo`'s arg list is a **string-output sink** — its whole job is to
space-join its arguments onto stdout. So `echo @a` is an *implicit
array→string join*, exactly what DD#22 forbids for `"$a"`, and it is
lossy: `a=("a b" "c"); echo @a` prints `a b c`, indistinguishable
downstream from `("a" "b" "c")` — reintroducing the word-splitting
ambiguity sash eliminates, right at echo's output (worst in a pipeline).

**Rule:** an array splat (`@a`) passed to `echo` is a **compile error**;
use an explicit join — **`echo $(join @a)`** (the `join` intrinsic, DD#41/
DD#60). Literal args are unaffected: `echo a b c` is fine (three literals,
no array structure to lose). `:` ignores its args, so this is moot for it
(`true`/`false` are now external programs — a bare `@a` to them is SB5032
like any external program). (New SB code in the splat family — SB5025–5027
— assigned at implementation; keep adjacent.)

Contrast: a splat to an **external** command (`myprog @{a:?}`) is *not* a
flatten — the program receives N **structured argv entries** (boundaries
preserved), so the splat stands (with the §2 disposition). The flatten
hazard is specific to a sash-language construct whose *output* joins its
args, i.e. `echo`.

## Dependencies / interactions

- **Prerequisite:** `@a` must first expand to N verbatim args in
  command-argument position (today it collapses to one joined string).
  Tracked separately (`array-splat-command-arg`).
- **Taint composes orthogonally:** a tainted-source array splat needs the
  cardinality disposition here AND its elements blessed for taint
  (Q5 / DD#61). Disposition is cardinality-only; bless is value-only.
- **Size is arithmetic:** `${#a[@]}` / cardinality tests belong on the
  `(( ))` path, not `[[ ]]` (see the for-loop-arith-condition cleanup).

## Tasks

- `array-splat-command-arg` — per-element splat (prerequisite).
- `glob-as-array-splat-disposition` — §1 + §2 implementation.

## Implemented (2026-07-08)

Part 1 (globs-are-arrays: for-in glob per-element, inline-`*` → SB5029) and
Part 2 (external-splat empty-disposition) are both done.

Part 2 (`glob-splat-disposition-word`): a bare `@a` into an external command
(post-DD#23 resolution, a `/`-prefixed head) is **SB5032**; `@{a:-WORD}`
(empty → the single arg WORD; bare `@{a:-}` → zero args) and `@{a:?WORD}`
(empty → abort, exit 1, `<progname>: WORD`) carry the disposition. Only
`:-`/`:?` (SB5033 otherwise). WORD is a `${x:-WORD}`-style interpolating
word, substituted as ONE element. The argv-splicing constructs (echo still
SB5030), user functions, value functions (join/len/…), and for-in are exempt.

Lexer scans `@{…}` into one Word (`scan_at_brace`); parser decodes it
(`parse_at_brace_disposition` → `arrvals_default`/`arrvals_abort`);
`SsaValue::ArraySplat` gained a `disposition`; the argv builders (native
`resolve_splat_tables`, c `cache_splat_tables_c`, janet `janet_splat_splice`)
apply it via the runtime `sash_splat_disposition`. spec/glob_splat.sh gs.1-13
native/c/janet; wordsplit 22.14-19 migrated to dispositions; 0 regressions.

## Addendum (2026-07-30): the empty-disposition is command-argument-only; a splat is not a scalar

Ratified with DG. Clarifies where the `@{a:…}` disposition and the bare
`@a` splat are legal, closing enforcement gaps found via an ICE
(`echo @{a:?}`) and DG's review of DD#22 / DD#60.

**A splat is a distinct syntactic category, not a scalar.** A named-array
splat (`@a`, or a dispositioned `@{a:-WORD}` / `@{a:?WORD}`) is legal
**only** in a splat position:

- as an **argument to a command** (external: bare `@a` is SB5032, must
  carry a disposition; `@{a:?}` / `@{a:-}` OK. `echo` still SB5030), and
- as the **sole source of a for-in header**, where it must be the **bare**
  `@a` (for-in has its own zero-match handling — DD#69 §1).

Every other position is a compile error — mirroring DD#60's rule for
`$(argv)`, now extended to the native `@a` form:

- **scalar assignment** `x=@a` / `x=@{a:?}` → **SB5025** (a splat is not a
  scalar; use `$(join @a)` for a joined string, or `${a[i]}` for one
  element). *(Previously silently flattened to a joined string.)*
- **the empty-disposition outside a command argument** → **SB5034**. In
  particular `for f in @{a:?}` is rejected — a for-in source is the bare
  `@a` (the `:-`/`:?` cardinality behavior only means something when
  splatting into a command's argv). (A splat in a test / arith context is
  already rejected by those contexts' own checks — SB4018 / SB4035 — no
  new code needed there.)

**Quotes are literal (bash-compatible), NOT a splat.** In sash-native the
splat sigil is the *unquoted* word `@a`; inside `"…"` a `@` is an ordinary
literal character (`"user@host"`, `"@handle"`). So `echo "@a"` prints `@a`
and `echo "@{a:?}"` prints `@{a:?}`, exactly as bash does — these are **not**
quoted splats and are **not** rejected. DD#22's "quoting an array splat is a
compile error" targets the *recognizable* bash-style spellings
(`"${a[@]}"` / `"$(argv)"`, which sash-convert coerces), not the native
literal `"@a"`. For an array's contents as a string, use `$(join @a)`.

Codes: SB5025 (splat in scalar context — reused), **SB5034** (empty-
disposition outside a command argument — new).

## Addendum (2026-07-30): `true`/`false` are external programs; "builtin" retired

Directed by DG. Two coupled cleanups.

**`true`/`false` are externalized.** They were inline status-setters (a
`BuiltinCmd::True`/`False` that just set `$?`, never forking). They now
resolve to `/usr/bin/true`/`/usr/bin/false` (DD#23) like any other program.
Rationale (DG, least surprise): "if there is a program named `false`, you
should be able to run it." There is no sash-language boolean type; the
argv-splicing set shrinks to exactly `echo` and `:`. An internal always-fail
is `! :` (or a function that `return 1`s); a bare `false` is a deliberate
invocation of `/usr/bin/false` and aborts (DD#55) like any failing command.
sash-convert emits a CHECK comment on bash `true`/`false` so a reader can
confirm the externalized semantics (`sash-convert-check-comment-true-false`).

**"Builtin" terminology retired.** The old split (builtin vs external
command) obscured what these names are. Reframed: `echo`, `break`, `:`,
`bless`, `read`, `[[ … ]]`, the value functions (`toupper`, `len`, `keys`,
`join`, `sub`, `gsub`, `match`, `basename`, `dirname`, `argv`, `getenv`,
`epochseconds`, …), etc. are **parts of the sash language**; anything
resolved to a path is an **external program**, *not* part of the language.
The value functions are legal only inside `$( )` (rejected as a command —
SB5011). This addendum's wording (and the §Scope / §3 edits above) use
"sash-language construct" and "external program" throughout.
