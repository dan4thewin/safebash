title: DD#69 — Globs are array sugar; external-command array splats require an empty-disposition
status: accepted

Decided with DG 2026-06-20; ratified 2026-06-21. Reframes the glob lines
of DD#20 and adds a compile-time rule for array splats passed to external
commands.

## 1. A glob is sugar for an array

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

- **External commands only.** The rule in §2 (empty-disposition) applies to
  external commands. The **inline builtins are exactly `echo`/`true`/`false`/
  `:`** (`transform/resolution.rs`); `printf` and everything else resolve to
  an absolute path (DD#23) and are therefore **external** — a bare `@a` to
  `printf`/`ls`/etc. requires `@{a:?}` like any external command. (Earlier
  drafts wrongly listed `printf` as an exempt builtin.) for-in is exempt (its
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

Ratified with DG 2026-07-04. §2's *cardinality* exemption for the inline
builtins does **not** license `echo` to implicitly flatten an array.

`echo`'s arg list is a **string-output sink** — its whole job is to
space-join its arguments onto stdout. So `echo @a` is an *implicit
array→string join*, exactly what DD#22 forbids for `"$a"`, and it is
lossy: `a=("a b" "c"); echo @a` prints `a b c`, indistinguishable
downstream from `("a" "b" "c")` — reintroducing the word-splitting
ambiguity sash eliminates, right at echo's output (worst in a pipeline).

**Rule:** an array splat (`@a`) passed to `echo` is a **compile error**;
use an explicit join — **`echo $(join @a)`** (the `join` intrinsic, DD#41/
DD#60). Literal args are unaffected: `echo a b c` is fine (three literals,
no array structure to lose). `true`/`false`/`:` ignore their args, so this
is moot for them. (New SB code in the splat family — SB5025–5027 — assigned
at implementation; keep adjacent.)

Contrast: a splat to an **external** command (`myprog @{a:?}`) is *not* a
flatten — the program receives N **structured argv entries** (boundaries
preserved), so the splat stands (with the §2 disposition). The flatten
hazard is specific to a builtin whose *output* joins its args, i.e. `echo`.

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
