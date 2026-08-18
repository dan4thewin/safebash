title: DD#60 — argv / arg / argc builtins (C semantics)
status: implemented
supersedes: DD42

## Addendum 2026-08-17 — `$(arg N DEFAULT)` optional default

`$(arg N)` out-of-range **aborts** (below). This addendum adds an *opt-in*
fallback: `$(arg N DEFAULT)` returns `argv[N]` if `N` is in range, else the
value of `DEFAULT`. The abort form is unchanged — a bare `$(arg N)` still
aborts, so a *forgotten* argument is still caught; the default is only for a
*genuinely optional* argument.

    dir=$(arg 1 .)         # argv[1], or "." if not supplied
    bless dir [[ ! $dir =~ \s ]]

This is the disposition `:?` (strict, abort) vs `:-` (default) split applied to
argv — the same idiom as DD#69 `@{a:?}`/`@{a:-}` and DD#77 `${s:?}`/`${s:-}`.
`arg` is a builtin in `$(…)` position, so it cannot wear the `${…:-}` syntax;
the default is a **positional** second argument (`arg` takes exactly one index,
so a second token is unambiguous).

- **Eager**: `DEFAULT` is evaluated whether or not the fallback is taken
  (mirrors `emit_default_value`'s `select`; DD#78). Safe because —
- **`DEFAULT` is constrained by DD#78**: a clean, non-`random` (side-effect-free)
  value. `$(arg 1 $(getenv HOME))` / `$(arg 1 $(date))` are rejected — capture
  into a variable and `bless` first.
- **Taint unchanged**: `$(arg N D)` is still a tainted source (the result may be
  `argv[N]`), so it is blessed downstream as before; the default's cleanliness
  is a DD#78 read-position rule, not a change to `arg`'s taint.
- **Negative index** stays a compile error (below), default or not.

## Original decision

Program-argument access in sash-native. Supersedes DD#42's `$(argv N)`
scalar shape with a three-builtin surface that follows C `argc`/`argv`
semantics and composes with the DD#24 taint model and DD#61 array rules.

## Revision 2026-06-17 — snapshot is an out-param (implemented 2026-07-08)

Implemented in `argv-snapshot-to-outparam`: `argv [N] a` (out-param fill,
auto-declares + taints the array; `array a=$(argv …)` now rejected SB5025).
Native/c/janet green, 0 regressions. spec/argv.sh av.n1-7/x5/rej1-3.

The **snapshot form changes** from `array a=$(argv N)` (an assignment
whose RHS yields an array) to an **out-param builtin** `argv [N] a` (the
array is the trailing positional, like `mapfile -m m` / `read -a a` /
`match S P arr`, DD#63):

    argv a        # fill array a with all program args (incl. argv[0])
    argv N a      # fill a with the slice argv[N:]   (array last; N optional)

This deletes the lone case in the language where a command substitution
*yields an array* (the `ArgvSnapshot` value + the `array a=$(argv)`
detection in `compute_declaration_value`), unifying bulk array population
on one out-param pattern. **Unchanged:** the splat `$(argv)` / `$(argv N)`
in a for-in header (the per-iteration body-bless source form, DD#60 Q5);
the scalars `$(arg N)` / `$(argc)`; and the taint/bless model (`argv a`
fills a tainted array — bless-all before reading, DD#61). The body below
still documents the as-implemented `array a=$(argv N)` snapshot; the task
`argv-snapshot-to-outparam` migrates it.

## The trio

`$(argc)` returns the program's argc following C semantics: 1 plus the
count of user-supplied arguments. The program name (`argv[0]`) is always
present, so `$(argc) >= 1` always holds. `$(argc)` is **clean** — it is a
count (process metadata, like `$#`), not external data.

`$(arg N)` returns the Nth program argument as a single string, 0-indexed,
and is **tainted**:
  - `$(arg 0)`  → argv[0]  = program name
  - `$(arg 1)`  → argv[1]  = first user arg
  - `$(arg N)`  for N >= `$(argc)` aborts at runtime with a clean error
    (consistent with DD#26 overflow-trap philosophy; no silent empty)

`$(argv)` returns the entire argv array, including the program name at
index 0. Equivalent to `$(argv 0)`. **Tainted**, splat-context only.

`$(argv N)` returns the argv slice starting at index N (Python-style
`argv[N:]`). N >= `$(argc)` yields the **empty array** (zero iterations),
NOT an abort — the slice form is the natural "iterate user args" idiom.
**Tainted**, splat-context only.

## Context restrictions

`$(argv)` / `$(argv N)` are array-returning substitutions, legal **only**
in:
  - a for-in header, as the **sole** source (no mixing — see below)
  - the RHS of `array a=$(argv)` / `array a=$(argv N)` (snapshot)

Any other appearance is a compile error:
  - `x=$(argv)`        rejected (scalar-context array splat)
  - `cmd $(argv)`      rejected (command-arg-position splat)
  - `echo "$(argv)"`   rejected (quoting an array splat — DD#22)
  - `[[ -n $(argv) ]]` rejected
  - `$(($(argv)))`     rejected

`$(arg N)` and `$(argc)` are scalar-returning, legal anywhere a scalar
substitution is, subject to DD#24 taint rules.

Negative indices (`$(arg -1)`, `$(argv -1)`) are rejected at compile time
— no negative indexing in v1.

## Consumption + taint

The governing rule (DD#24): **assigning from a source is allowed (the
target becomes tainted); reading a tainted variable is rejected except
inside `bless`.** `getenv`/`argv`/`arg`/`read`/`$(extern)` are sources;
`$a`, `${a[i]}`, `@a` are reads of a stored variable. For-in iteration is
therefore a **bind** (from a source) or a **read** (of a variable),
depending on the iterand:

- **Direct `$(argv)` / `$(argv N)` for-in** (no named array): each
  iteration binds one argv element to the loop variable *from the source*
  — assignment-from-source, parallel to the scalar `x=$(arg 1); bless x;
  use x`. Per-iteration body-bless is valid:

      for x in $(argv 1); do bless x [[ $x =~ … ]]; echo $x; done

  The read analog is `while read x; do bless x; done` (streaming,
  body-bless). Both are allowed because nothing reads a stored variable.

- **Named tainted array** (a snapshot, or any `read -a`/`mapfile` array):
  iterating it — `for x in @a` — *reads* its stored tainted elements (the
  `b=$a` case), which DD#24/DD#61 reject. Materializing a stream into an
  array turns it into a stored value; once stored, bless-all first, then
  iterate the clean array:

      array a=$(argv 1)              # tainted snapshot (DD#61 bulk-read)
      bless x in a [[ $x =~ … ]]     # DD#61 bless-all clears the array
      for x in @a; do echo $x; done  # @a is the canonical splat spelling

  Once blessed, the DD#41 collection builtins apply: `len a`, `keys a`,
  `join @a`. There is no direct `join $(argv)` — join operates on a named
  array, so snapshot first.

You can body-bless a read *stream* (`while read` / `for x in $(argv)`) but
not a read *array* — because the array is a stored value.

## No-mixing in for-in headers

A for-in header must have exactly one taint-uniform source. Rejected:

    for x in head $(argv) tail; do …; done    # clean + tainted
    for x in $(argv) $(argv 1); do …; done     # two tainted sources

This is a corollary of DD#24 (use-site rejection): the body's bless on the
loop variable can only certify uniform-source input. (Two **named clean
arrays** — `for x in @a @b` — are allowed; both clean → uniform.)

## Canonical array-splat spelling

`@a` (= `@arr`, DD#41: bare, no braces, no quotes) is the canonical
sash-native array-splat spelling and the only form the converter emits.
The brace `${a[@]}` and `$a[@]` forms are accepted on input as a parser
convenience (semantically identical) but are never produced.

## DD#46 at top level

`$@`, `$*`, `$1`..`$N`, `$#`, `$0` are **function-local only** (DD#46).
Top-level use is a compile error directing to the appropriate builtin:
  - `$@` / `$*` → use `$(argv)` for iteration or `$(arg N)` for one arg
  - `$1`..`$N` → use `$(arg N)`
  - `$#`       → use `$(argc)` (C argc; includes the program name —
    subtract 1 for the user-arg count)
  - `$0`       → use `$(arg 0)`

Inside a function body these forms stay **clean** — call sites are visible,
so the caller's arguments are statically known.

## Migration from DD#42

  - `$(argv N)` old-scalar semantics  →  `$(arg N)`
  - `$(argv N)` new slice semantics    →  same syntax, different shape
  - `$(argc)` was equivalent to `$#`   →  now C argc (1 + user-arg count)
  - `$(argv @)` (never-shipped spelling) → `$(argv)`

## See also

- DD#22 — no word splitting (quotes yield one string; array splats unquoted)
- DD#24 — taint quarantine (argv/arg sources are tainted; argc clean)
- DD#26 — integer overflow traps (same abort philosophy for `$(arg N)`)
- DD#41 — sash-native array syntax (`@arr`; `len`/`keys`/`join`)
- DD#46 — positionals function-local (top-level redirects to here)
- DD#61 — array taint construction + bless-all (`array a=$(argv)` is a
  bulk-read source)
- DD#42 — getenv/argv (superseded by this DD for the argv half)
