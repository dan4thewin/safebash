title: DD#69 — Globs are array sugar; external-command array splats require an empty-disposition
status: accepted

Decided with DG 2026-06-20; ratified 2026-06-21. Reframes the glob lines
of DD#20 and adds a compile-time rule for array splats passed to external
commands.

## 1. A glob is sugar for an array

A glob expands to an **array**, with **one lexically normalized absolute
path per matching directory entry** (DD73). In `/work`, `array a=(*)`
contains paths such as `/work/foo`, not cwd-relative names. This preserves
referential stability across `chdir` and closes glob-sourced
option-injection (`-rf` becomes `/work/-rf`). There is no word splitting
(DD22): spaces and newlines in a filename stay within one element.

DD39's 2026-11-09 rule permits **at most one `*` in the final filename
component of each glob**, with a literal directory prefix. Leading-dot entries
require an explicit leading dot in that component; `.` and `..` are
never wildcard results. Each expansion is sorted bytewise.

**Globs compose in list positions** (DG, 2026-11-09):

- **Array literal:** `array f=(.* *)` concatenates the dotfile expansion
  and the ordinary expansion. A glob is not restricted to being the
  literal's sole element; other legal elements retain their order.
- **Array append:** `array f=(.*); f+=(*)` appends the expanded ordinary
  entries, not a table handle, joined string or literal `*`.
- **for-in:** `for f in .* * { ... }` iterates the concatenated expansions,
  just as materializing that list into an array and iterating it would.
  The single-glob form remains `for f in *` ≡
  `array tmp=(*); for f in @tmp`.
- **Ordinary command-argument position:** inline `*` (e.g. `echo *`,
  `ls *.sh`) is a **compile error** — "globs go through arrays"
  (`array a=(*); … @{a:?}`).
  There is no inline glob expansion and no runtime nullglob/failglob.
  DD131's explicit selectors instead interpret ordinary string pattern
  operands; that is not argument expansion.

Each glob contributes its sorted elements **at its source position**.
There is no sort of the whole initializer/append/iteration list, no
deduplication of overlapping patterns, and no new ordering of elements
already in an append target. The one-directory limit is per expansion,
not per statement: `.* *` may enumerate the same directory separately.
This selects no caching or filesystem-snapshot guarantee.

**Successful expansion with zero matches contributes zero elements**;
if the entire `for` list is empty at runtime, its body never runs. This
is for-in's built-in empty handling, so §2 does not apply there. It does
not legalize a syntactically empty `in` list (DD68). Nor does multiple-glob
composition change the separate rules for named splats or `$(keys ...)`.

An open/enumeration failure is not a successful zero-match expansion.
DD73/DD130 govern its error handling. No assignment-level `-`, fabricated
status or empty-success fallback is introduced here.

New glob elements are **clean**: a directory entry is a compiler-trusted
value like `$(getppid)`, so a fresh glob result carries no taint and needs
no value-bless. Splat use still declares cardinality (§2). Under DD131's
provisional accumulation model, adding a clean batch does not resolve
existing destination taint or its owners; DD128 still governs that state.

**The 2026-11-09 package is provisional and not implemented (DD131).**
`glob ARRAY PATTERN...` is the proposed canonical operation. Its
operational status is separate from the array; successful zero-match
enumeration succeeds. Normal fill targets accumulate, while a leading-
underscore target requests a fresh immutable scratch binding. The companion
`rglob ARRAY DIRECTORY REGEX...` matches basenames with ordinary regexes;
runtime patterns require DD132's successful validation proof.

Assignment sugar still replaces and append sugar still appends. A
replacement needs a fresh result before assignment, not a direct
accumulating fill into its old destination. Immutable scratch cannot be
an append/mutation target. DD131 preserves an ordinary existing target
on operation error, but a failed scratch construction leaves the fresh
empty instance, never the previous batch. It owns scope/re-execution
and sugar/quoting questions in the separate-branch trial.

`q-glob-semantics` owns that trial, coordinating the directory-prefix,
multi-star-rejection and file-failure tasks. The historical single-glob
implementation below is not evidence that these new forms already work.

## 2. External-command array splats state their empty behavior

The hazard "an array expands to nothing, so an external command silently
runs with too few/zero args" is general — it applies to any runtime-sized
array (glob, `read -a`, `mapfile`, `$(argv)`, or an array mutated to
empty), so the rule lives on the **splat site**, not on globs.

**Rule:** a bare `@a` splat passed as an argument to an **external
command** is a compile error. The splat must carry an empty-disposition:

- `@{a:-WORD}` — when `a` is empty, the splat is the single arg `WORD`;
  bare `@{a:-}` yields **one empty arg** (empty-word default — bash
  `"${a[@]:-}"` parity, and consistent with the scalar `${x:-}`; corrected
  2026-09-04, was "zero args").
- `@{a:?WORD}` — when `a` is empty, **abort** at runtime with the nonempty
  literal `WORD` on stderr. If omitted or empty, use a generated diagnostic
  naming the collection and its emptiness (DD77; accepted 2026-11-10,
  implementation pending). The explicit, local replacement for failglob.
- `@{a:~}` — **elide** (DD#84): when `a` is empty, the splat contributes
  **zero args** — the sole drop-if-empty disposition (the array analog of the
  scalar `${x:~}`). Like the scalar form it takes **no word** (`@{a:~WORD}` is
  SB5033). Added 2026-09-04 (DG: "for array splat to command, we allow `:-`
  and `:?`; `:~` should work too") — and the same day `@{a:-}` was corrected
  to one empty arg, so `:-` (one element) and `:~` (zero) are now distinct,
  uniform with the scalar `${x:-}`/`${x:~}` pair.

A non-empty array splats to one verbatim arg per element. These forms map
1-to-1 onto bash `"${a[@]:-WORD}"` / `"${a[@]:?WORD}"` (DD#67 sugar) — incl.
bare `@{a:-}` ≡ `"${a[@]:-}"` → one empty arg.

`default-require-diagnostics` owns the new omitted/empty-message behavior.
The current splat runtime still emits only the program prefix in that case.
Nonempty custom literal text, cardinality, abort status and handler routing
are unchanged; exact diagnostic wording and implementation remain deferred.

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

## Implemented (2026-07-08; original single-glob forms)

Part 1 (globs-are-arrays: for-in glob per-element, inline-`*` → SB5029) and
Part 2 (external-splat empty-disposition) were completed for that scope.
The later composition and bounded-pattern policy in §1 remains pending.

Part 2 (`glob-splat-disposition-word`): a bare `@a` into an external command
(post-DD#23 resolution, a `/`-prefixed head) is **SB5032**; `@{a:-WORD}`
(empty → the single arg WORD; bare `@{a:-}` → one empty arg), `@{a:?WORD}`
(empty → abort, exit 1, `<progname>: WORD`), and `@{a:~}` (empty → zero args,
elide) carry the disposition. Only `:-`/`:?`/`:~` (SB5033 otherwise). WORD is a
`${x:-WORD}`-style interpolating word, substituted as ONE element. The
argv-splicing constructs (echo still SB5030), user functions, value functions
(join/len/…), and for-in are exempt.

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

## Addendum (2026-09-28) — the disposition is now load-bearing at a call site

This DD chose the dispositions to settle what an *external* command sees when
an array is empty. They acquired a second job on 2026-09-28: they are what
lets a splat occupy a **fixed** argument position of a user function.

DD#81's rule is that a splat has a run-time length, so nothing written after
one lands at a position the compiler can count on — required arguments go
before the splat, or SB5087. The exception is a splat that is *guaranteed* to
deliver an element, and that guarantee is exactly this DD's disposition:

| form | guarantees | can fill a fixed position? |
|---|---|---|
| bare `@a` | nothing (empty → zero args) | no |
| `@{a:~}` | nothing (the elide) | no |
| `@{a:-WORD}` | one element (DD#84, 2026-09-04) | **yes**, the first one |
| `@{a:?WORD}` | one, or the program aborts | **yes**, the first one |

Only the *first* such position: `@{a:-W}` guarantees one element but may
deliver twenty, so it pins position `p` and nothing after it.

DG (2026-09-28): "I grant that `@{e:-fallback}` can be guaranteed to provide
1 argument."

Consequence for this DD: the `has_word` / `abort` distinction is no longer
only a run-time behaviour, it is read by the arity check
(`transform/cmd_handlers.rs::arg_min_tokens`). Changing what an empty array
yields for any of these four forms changes which calls compile. Specs
`optional_args.sh` 84.59, 84.60, 84.70.

## Addendum 2026-10-28: `:` evaluates its arguments

§3's "`:` ignores its args" is about the join rule only: `:` writes nothing,
so it flattens nothing. It never meant that the arguments go unevaluated, but
both backends read it that way and lowered no argument of `:`. So
`: @{a:?}`, `: ${s:?msg}` and `: $(( 1 / z ))` never aborted, and neither
did bash's own parameter-assertion idiom after conversion
(`notes/tasks/bug-noop-drops-argument-effects.md`). Settled by §2 here,
DD#77 ("`:?` un-prune scope — everywhere") and DD#86 (c) (`: $v` is a real
read). `:` now evaluates every argument for its effects, in the order the
argv builder uses (splat dispositions first, then each value left to
right), and discards the values. A disposition aborts on `:` exactly as on
`echo`, in a statement, a condition, under a redirect and as a stage.

Decide-and-report: `:` builds no argv, so the lowering reuses the builder's
leaves without the builder. That means no heap argv per `:` (the bump
allocator never frees) and no DD#99 interior-NUL check, which guards the
execve(2) boundary that `:` never crosses. A NUL-bearing argument to `:` is
not an error (noop.13). Specs: commands.sh noop.1-noop.13, convert.sh
57.192.

DD70's statement-effect rule (DG, 2026-11-08) preserves explicit `:` as
the intentional no-op, including an otherwise legal `- :`. This does
not make `:` an observable effect: a result-only `[[ ... ]] || :` at
statement level is still pointless. Argument effects remain evaluated;
`- : >optional.log` also retains its real file operation and DD130
recoverable-open handling. Enforcement of the new statement check is
queued in `reject-pointless-statements`.
