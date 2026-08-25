title: DD#75 — Named/keyword forms are canonical; `$`-sigils are sugar; collections are uniform
status: accepted

Decided with DG 2026-08-11. Codifies a **pivot** that has been implicit for
several DDs: sash no longer optimizes for bash compatibility but for being
**unambiguous, self-consistent, symmetrical, and transparent**. Where those
conflict with bash habit, they win. This DD makes that principle explicit and
applies it uniformly to special variables, length/keys intrinsics, positional
parameters, and array splats.

Supersedes the journal-050 "magic variable policy" (never ratified as a DD).
Amends DD#24 (taint sources), DD#41 (array syntax), DD#46/#60 (positionals),
and the splat clauses of DD#60/#69. Supersedes the tasks
`length-operator-int-typing` and `len-nonarray-operand-reject`, and constrains
`intrinsics-lengths-in-arithmetic` (length/`len` must evaluate in `$(( ))`).

## The principle

The **named/keyword form is canonical**; the `$`-punctuation form, where one
exists, is **sugar that lowers to it**. This is the opposite of journal 050,
which kept `$?`/`$!`/`$$` as canonical "punctuation specials." DG's intent has
evolved since 050 (see pivot above): a keyword (`status`, `getcpid`, `len`,
`join`, `keys`) is self-documenting; a punctuation sigil is not.

Consequences, made concrete below: `$?`/`$!`/`$$` become sugar; a length is
always the `len` intrinsic (with `${#x}` its operator sugar); positional
parameters become one real array (`args`) reached through the ordinary
collection surface; and there is exactly **one** spelling for a splat (`@a`).

## 1. Special variables are sugar for keyword forms

| Sugar (accepted on input) | Canonical (emitted, taught) | Value |
|---------------------------|------------------------------|-------|
| `$?`                      | `$(status)`                  | last exit status |
| `$!`                      | `$(getcpid)`                 | last background child pid |
| `$$`                      | `$(getpid)`                  | current pid |

The lowerer already collapses `$(status)`→`$?` and `$(getcpid)`→`$!`
internally (one AST node); this DD only reframes **which spelling is
canonical** (converter output, docs, diagnostics) — the keyword form — and
adds `$$`↔`$(getpid)`. `$LINENO` is exempt: it is a compile-time literal, not
a runtime value, and has no keyword form.

**Taint (DD#24):** the clean status previously attached to `$?`/`$!`/`$$` now
attaches to `$(status)`/`$(getcpid)`/`$(getpid)`; the sugar inherits it. No
value changes taint class.

**Implemented (compiler + specs), 2026-08-14.** Both spellings already lower to
one node and are accepted; this session pinned the equivalence and reframed the
canonical spelling in the compiler surface:
- `$$`≡`$(getpid)`, `$?`≡`$(status)`, `$!`≡`$(getcpid)` verified equal in all
  three backends; specs `misc.sh` spv.1–4 (native/c/janet).
- Diagnostics lead with the keyword form: SB5012 now reads "$(status) is not
  available … (the punctuation sugar $? reads the same value)".
- Fixed a latent bug the reframing surfaced: `$$`/`$?` silently read 0 as a
  *compound-arith* operand (`$(( $$ + 1 ))`); native `lower_value_as_int` now
  computes them (specs arithmetic.sh 31.20–24). DD#24/DD#31 clean-operand rule.

Deferred (follow-ups, not blocking): sash-convert emitting the keyword forms on
output (`converter-special-var-sugar`); the public GUIDE/SUBSET prose reframe
(DG voice). The keyword pid/random *intrinsics as arith operands* remain a
separate gap (`bug-arith-keyword-pid-intrinsic-zero`).

## 2. `len` is type-polymorphic (Go semantics)

`len x` yields an **int**, dispatching on `x`'s type (the transform already
knows collection-ness via `state.collections`):

- `len <array>` / `len <map>` → element / entry count.
- `len <string>` → **byte** length (DD#12 counted strings; matches Go's
  `len(string)`, which is bytes, not runes).
- `len <undefined / unknown name>` → **compile error** (no runtime garbage).

`${#x}` is **Int-typed sugar for `$(len x)`** — the only surviving `${#…}`
length form (see §4). Both are int, so `int n=${#a}`, `int n=$(len a)`, and
`$(( (len a) + 1 ))` all type-check and evaluate. This retires the syntactic
`$`-sigil dispatch of the old `len` (`len a` = array vs `len $a` = string),
which was the source of the scalar-`len` garbage bug.

## 3. `keys` is map-only

- `$(keys <map>)` → a **splat of the map's string keys** (each key a string).
  Like `$(argv)`, it is an array splat subject to the DD#60 context rule: legal
  as the **sole source of a for-in header** (or snapshotted into an array),
  and **SB5025** anywhere else (command-arg / scalar position). Today it wrongly
  bypasses that guard — see §7.
- `keys <indexed array>` / `keys <scalar>` → **compile error**. An indexed
  array's keys are trivially `0..len-1`; iterate positions with a C-style
  `for (( i=0; i < $(len a); i++ ))` (supported) or values with `@a`.

Map access is symmetric and both forms already work: **`$(keys m)`** splats the
string keys, **`@m`** splats the values; `${m[$k]}` reads one value by key. No
`values` companion intrinsic is needed. (Retiring `keys` on indexed arrays also
removes a current bug where it returns blanks.)

## 4. One splat spelling; retire the bracket-subscript collection forms

`@a` is the **sole** array/map splat. The bracket-subscript collection forms
are retired (compile error, pointing at the canonical form):

| Retired | Role | Canonical replacement |
|---------|------|-----------------------|
| `${a[@]}`, `$a[@]`   | splat        | `@a` |
| `${a[*]}`, `$a[*]`   | splat/join   | `@a` (splat) / `$(join a)` (join) |
| `${#a[@]}`, `${#a[*]}` | length     | `${#a}` / `$(len a)` |
| `${!a[@]}`, `${!a[*]}` | keys       | `$(keys m)` (maps only, §3) |

The implicit-join `*` forms (`${a[*]}`, and `$*` in §5) are dropped outright:
an implicit separator is exactly the ambiguity this DD rejects. Join is always
the explicit `$(join a)`.

**Unaffected** (these are not `[@]`/`[*]` subscripts): indexed element access
`${a[$i]}` / `${a[1]}` / `a[$i]=…`; the `@`-splat sigil and its DD#69 disposition
`@{a:-W}` / `@{a:?W}`; `*` as a glob (DD#69) and as arithmetic multiply. After
this DD, `@` means **only** "splat," and `*` has **no** array/positional
meaning anywhere.

**Subscript-index invariant (pre-existing, restated):** a subscript index is a
numeric literal (`a[1]`) or a **`$`-sigilled** value (`a[$i]`, `a[${b[$j]}]`).
A **bare word** `a[i]` is the *string key* `"i"` — valid only on a map; SB4030
on an indexed array. The `$1..$n → ${args[0]}..${args[n-1]}` sugar (§5) uses
literal indices; a computed positional is `${args[$i]}`.

This retires the DD#60 "parser convenience" that accepted `${a[@]}` on input:
one true spelling over paste-from-bash friendliness (the converter rewrites
bash forms — §6).

## 5. Positional parameters are the implicit `args` array (Model B)

A function's arguments are one real, per-function-scope array named `args`,
reached through the ordinary collection surface. The bash positional sigils
become **sugar** for that surface — each a plain array *operator*, never an
intrinsic call:

| Sugar | Canonical | Operator |
|-------|-----------|----------|
| `$1`..`$n` | `${args[0]}`..`${args[n-1]}` | subscript |
| `$@`       | `@args`                    | splat |
| `$#`       | `$(len args)`              | length (`${#args}` is operator sugar) |
| `$*`       | *(removed)* → `$(join args)` | — |

- **Amended 2026-08-25 (DG): `$#`'s canonical form is `$(len args)`, not
  `${#args}`.** The table originally placed the `${#args}` *operator* in the
  canonical column, contradicting §1 (the `len` intrinsic is canonical; `${#x}`
  is its operator sugar, and `$#` is bash sugar on top). Length is the one row
  where the operator form is itself sugar — the splat (`@args`) and subscript
  (`${args[i]}`) operators *are* canonical, but `${#}` desugars to `$(len …)`.

- **`args` is 0-indexed** (`args[0]..args[n-1]`, keys `0..n-1` in the DD#33
  table; `args[0]` is the first argument). **Amended 2026-08-20 (DG): 0-indexed,
  not 1-indexed.** The original §5 chose 1-indexed to make `$1 ≡ ${args[1]}` a
  literal digit match, but sash's named arrays are 0-indexed, so a 1-indexed
  `args` would be the *only* inconsistent array in the language (off-by-one for
  `${args[$i]}` computed indexing, iteration, `keys args`). Prior art confirms
  the rule "args-indexing matches the language's general array indexing": the
  1-indexed-args languages (fish, zsh, Lua) are 1-indexed *everywhere*, while
  every 0-indexed-array language keeps args 0-indexed — Elvish (a 0-indexed list
  literally named `args`, `$args[0]` = first arg), Oil/YSH (`ARGV[0]`), Python,
  Ruby, Perl, Go, Node, Tcl. So `$1 ≡ ${args[0]}`; the `$1..$n` → `${args[0..]}`
  sugar absorbs the off-by-one at desugar time. `$# ≡ ${#args} = n` is unchanged
  (length is base-independent).
- **No bare `$args`.** `args` is only reached via `@args`, `${args[$i]}`,
  `${#args}`, `$(join args)` — the named-array surface. `args` is a reserved
  name inside function bodies.
- **`$0` is not part of `args`.** Inside a function `$0` is still the program
  name (`$(arg 0)`, DD#60), unrelated to the function's own arguments. (This
  matches Perl/Ruby, where the program name is separate from the args array —
  `args[0]` is the first *user* argument, not the program name.)
- `$*` is gone; there is no implicit positional join.

This resolves the `@` double-duty (splat operator in `@a` vs. special-name in
`$@`): `@` is now always the splat operator, and `$@` is sugar for `@args`.
It also makes function args uniform with named arrays and with DD#60's
program-arg model (`argv`/`arg`/`argc`).

### Implemented (2026-08-21) — core

The **core** args model shipped native + c (0 spec regressions, all 3 modes):
`$1..$9` → `${args[0]}..${args[8]}`, `$@` (via pre-expansion) and `$#` →
`${#args}`, resolved through the current function's `args` array. Both the
expression path (`simple_expansion_to_value`) and the arithmetic path
(`arith_leaf_to_ssa`) route positionals through a shared
`expressions::positional_to_value`, so `$1`/`$#` mean the same inside and
outside `$(( ))`. User-written `${args[i]}`, `@args`, `${#args}`, and
`for x in @args` all work (native + c; janet keeps its own `_args` model).
Specs: `types.sh` 75.17–75.22.

**Unique-name model (the key mechanism).** Because `args` is a compiler-managed
abstraction (DG, 2026-08-21), each function inline mints a **fresh unique array
name** (`__args0`, `__args1`, …) materialized once from the call args
(`setup_callee_args`). Nested/recursive calls therefore never collide — the
callee's `__args1` is a different array from the caller's `__args0` — so there is
**no SSA save/restore** (only the current *name* is saved/restored, a cheap
string). This fixed a **pre-existing bug**: at HEAD (and any name-reused scheme)
a function reading a positional *after calling another function* got the
callee's value (arrays are name-slotted in codegen, not SSA-versioned). A
user-written `args` is remapped to the current unique name via
`expressions::resolve_args_name` at each array-name resolution site (subscript,
splat, `${#…}`, `for`-in).

**`shift` dropped** (see §5 amendment note and `research-array-slice`): it was a
degenerate slice. Removed the opcode, handler, codegen, and spec 48.2.

**Deferred** (see `dd75-args-model-polish`): `$(len/join/keys args)` (intrinsic
recognition still gates on `collections.contains("args")`); `$*` removal (still
pre-expands); `$@`→`@args` splat for >9 args (pre-expansion caps the tail —
pre-existing); a recursion guard (recursive inlining overflows the compiler —
pre-existing); a friendlier `shift`-removed diagnostic.

## 6. Migration (sash-convert)

The converter emits the canonical forms and accepts the bash forms as input:

- `${a[@]}` / `${a[*]}` → `@a`; `"${a[*]}"` / `"$*"` → `$(join a)` / `$(join args)`.
- `${#a[@]}` / `${#a}` → `${#a}` (or `$(len a)`).
- `${!m[@]}` → `$(keys m)`; `${!a[@]}` on an indexed array → a counting `for (( ))`.
- `$?` / `$!` / `$$` → `$(status)` / `$(getcpid)` / `$(getpid)`.
- Positionals per DD#46/#60 and §5.

## 7. Follow-on bugs (not part of the decision; fix separately)

- `join <arr> <sep>` (explicit separator) currently **segfaults** — must work,
  since explicit-separator join is the replacement for every `*` join.
- `${#a}` on a bare array currently returns `0` — must become `len a` (§2).
- `keys` on an indexed array currently returns blanks — resolved by §3 (reject).
- `$@`-into-array-literal (`array t=($@)`) was broken — moot under `@args`.
- **Subscript-sigil enforcement asymmetry:** the *read* `${a[i]}` rejects a
  bare string key (SB4030), but the *assignment* `a[i]=…` on an indexed array
  is accepted silently. Both sides must apply the invariant in §4.
- **`$(keys m)` bypasses the SB5025 splat-context guard.** `check_argv_splat`
  (`validate.rs`) recognizes only `$(argv)`; `$(keys m)` in command-arg / scalar
  position silently **joins** to one space-separated string, and with an
  unquoted format string emits **garbage bytes** (a `%s` reading a bad pointer —
  memory-unsafe). Fix: generalize the guard to every array-returning
  command-sub (`keys` as well as `argv`), so `$(keys m)` is for-in / snapshot
  only and SB5025 elsewhere (§3).

## See also

- DD#12 (counted strings — `len string` is bytes), DD#24 (taint sources move
  to the keyword forms), DD#33 (unified table — `args` 1-indexed keys),
  DD#41 (`@arr`, `len`/`keys`/`join`), DD#46/#60 (positionals; now the `args`
  model), DD#65 (`${var#pat}` strip already dropped, so `${#…}` owns `#`),
  DD#69 (glob arrays; `@` splat disposition).
