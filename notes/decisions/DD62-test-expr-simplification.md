title: "DD#62 — [[ ]] test simplification: exact-only ==/!=, =~ quoting, drop -v"
status: accepted

1. `==` and `!=` compare strings exactly, independent of quoting. String
   ordering by `<` and `>` uses unsigned bytes, without locale collation.
2. `=~` uses an unanchored regex; quoting groups its ordinary string operand,
   rather than literalizing the regex (DD63).
3. Known bindings have meaningful zeros on unassigned paths: `""`, `0`, or
   an empty collection. Names must be in lexical scope; body entry initializes
   newly owned private bindings (DD86).
4. File-age tests compare nanosecond mtimes strictly; `-ef` compares device
   and inode identity.

---

Language decisions for `[[ ]]` test expressions, captured from the
2026-06-17 discussion. (SUBSET.md is stale and not authoritative; this
DD + the implementation task `dd62-test-expr-ssavalue` are.)

## Motivation

Auditing `[[ ]]` surfaced three bash holdovers that add surface area
without value, plus an architectural payoff for removing them. The
test-path keeps operands as raw `ir::Expr` and lowers them through a
*duplicate* codegen subsystem (`lower_expr_value` / `lower_subscript`)
separate from the SSA path (`lower_value` / `lower_subscript_value`).
The only thing forcing *value* operands to stay raw IR is the
quoted-ness check `right_is_quoted` — and the only features needing it
are the three holdovers below. Remove them and value operands can be
lowered to `SsaValue`, retiring the duplicate lowering (and the
`[[ ${m[$k]} ]]` map bug it causes).

## Decisions

### 1. `==` / `!=` / ~~`=`~~ are exact string comparison only (no glob)

DD118 makes `==` the sole equality spelling.

Today the unquoted RHS of `==`/`!=` is glob-matched (`emit_eq`/`emit_ne`
call `sash_fnmatch`); a quoted RHS is exact. **Drop the glob branch:**
`[[ $x == foo ]]` and `[[ $x == foo* ]]` both become exact compares
(the latter against the literal three chars `foo*`). Quoting no longer
changes `==`/`!=` semantics. **Pattern matching is done with `=~`**
(regex), which is strictly more expressive and explicit.

~~`case` is unaffected — it lowers patterns to its own `=glob` operator
(`emit_glob`), which keeps `sash_fnmatch`. This decision is only about
`[[ ]]` `==`/`!=`/`=`.~~ DD66 replaces that surface with dispatch.

### 2. `=~` quoting is grouping, not literalization

Bash treats a quoted `=~` RHS as a *literal* match (`[[ $x =~ "a.b" ]]`
matches the literal string `a.b`). **Reject that semantics.** A quoted
`=~` RHS is matched **as a regex**; quoting exists only to *group* —
e.g. to let a space (or other delimiter-ish character) be part of the
pattern. The quoted content becomes part of the regex.

(Today this branch is already dead/broken: `parse_regex_rhs` produces a
`Literal`, so `right_is_quoted` is never true for `=~`, and a quoted
regex doesn't even parse — `[[ $x =~ "a b" ]]` → SB4014. So this
decision both removes dead code and fixes the inability to write a
regex containing a space.)

### 3. Remove `-v`

sash has **no unset state**: ~~an undeclared scalar reads as the zero
value (empty string / `0`),~~ and `unset` of a scalar is rejected
(SB4030). So bash's `-v` (distinguish *declared-empty* from *undeclared*)
tests a distinction that does not exist. sash's actual `-v`
(`emit_var_set_test`) just tests non-emptiness — a misnamed alias for
`-n` — and is broken for the one case where bash's `-v` is useful
(array elements: `[[ -v a[0] ]]` returns false for a set element).
**Remove `-v`** (a use of `-v` becomes an unsupported-operator error).

## Architectural consequence (see the task for implementation)

With `==`/`!=` glob gone and the dead `=~` quoted-literal branch
removed, `right_is_quoted` has no reachable callers and is deleted. With
`-v` gone, ~~the only `[[ ]]` operands that are *not* plain values are the
`=~`/`case` **pattern** operands (raw regex/glob text — never a
subscript).~~ DD63 makes regex operands ordinary strings; DD66 removes case.
Every other operand (LHS of all ops; RHS of `==`/`!=`/`<`/
`>`; unary `-n`/`-z`/`-f`/… args; `case` subject) is a value, and value
operands can be lowered to `SsaValue` in transform. Then test
subscripts flow through the SSA path (`lower_subscript_value`), the
test-path `lower_subscript` has zero callers, and it is deleted —
closing the `dynamic-map-subscript-in-test` bug by construction.

## Open questions (resolve before the regex-quoting/sed work)

The regex quoting/interpolation/delimiter/`\` design **moved to DD#63**
(`regex-quoting-interpolation`), which records the decisions taken
2026-06-17 (interpolation is a textual first pass; `''` disables;
literal containment → `fgrep`) and its remaining open items (delimiter
escape rule, match-form `/`, `}` in the expression form). DD#62's
`==`/`!=`-exact and `=~`-quoting-is-grouping decisions feed it.

Remaining open here (zero-value model, from decision #3):

- **Undeclared scalar read** currently yields empty silently
  (`echo $foo` compiles). Adjacent to the zero-value model behind
  decision #3 — should an undeclared scalar read be a compile error
  instead of a silent empty? (Footgun in a "safe subset".)

## Resolved (DG, 2026-06-21): definite-assignment, `${x:-}` opt-out

<del>

**A bare scalar read (`$x` / `${x}`) is rejected if `x` can be reached
without having been assigned on *some* path** — i.e. **strict
definite-assignment**, not merely "declared somewhere." The explicit
escape is the default form **`${x:-WORD}`** (DD#65), which is always
allowed and states "maybe-unset is intended" (reads the default/zero value
when unset). Examples:

- `echo $foo` (assigned nowhere) → **reject** (a never-assigned name has no
  assigning path). Typos are caught.
- `if c; then x=1; fi; echo $x` → **reject** (the `c`-false path reaches the
  read unassigned). Fix: `x=` before the `if`, or read `${x:-}`.
- `x=1; echo $x` → ok (definitely assigned).
- `echo ${x:-default}` → always ok (explicit maybe-unset; the only way a
  scalar zero-value is reached).

Consequence: the scalar **zero-value model survives only behind `${x:-}`**;
every bare read is definitely-assigned. Feasibility: sash already builds
SSA with phi at branch joins, so the join/dataflow machinery
definite-assignment needs already exists. Loops (`while`/`for` bodies may
run zero times) and use-before-assign fall out of the same all-paths rule.
A typed declaration's zero-init (`int x` → 0, `array a` → empty) counts as
an assignment — settle the exact "declaration zero-inits" set in the task.
See task `reject-undeclared-scalar-read`.

</del>

## Amendment (DG, 2026-07-08) — Model B: meaningful zero + typo check only

The definite-assignment escalation above is **superseded**. DG: "the
footgun is the misspelled variable … in Go the zero value is preferably
meaningful — we boxed ourselves into a corner such that the zero value is
useless."

- **Scalar zero values are meaningful** (untyped → `""`, `int` → `0`,
  container → empty) — the runtime already produces these. A bare read
  `$x` of a **known** variable that happens to be unassigned on some path
  is **allowed** and yields the zero value (Go-like). `if c; then x=1; fi;
  echo $x` is fine (prints the zero on the else path), NOT an error.
- **The only rejection is the typo:** a scalar read `$x` / `${x}` whose
  name is introduced **nowhere in scope** ~~(assignment, declaration, loop
  var, `read`/`mapfile` target — anywhere in the current function scope or
  the enclosing top-level scope)~~. That is a cheap single-pass **scope
  membership** check (a "may"/union analysis), NOT the definite-assignment
  dataflow (no branch intersection, no zero-trip-loop modeling, no SSA
  backstop).
- `${x:-WORD}` stays the explicit "non-empty default" opt-out ~~(always
  allowed)~~. ~~Containers (`${a[i]}`) are out of scope for the typo check for
  now (element reads yield the zero value; a nowhere-introduced container
  is a possible follow-up).~~ DD86 governs positional scope for all bindings.
- Rationale/tradeoff: this catches the high-value footgun (typo → silent
  empty → `rm -rf "$PREFIX/"`), which is a *scope* property, not a
  dataflow one. The price (a forgot-to-assign-on-one-branch logic bug
  reads the zero value instead of erroring) is the same price Go pays for
  a meaningful zero value. SB5031. See task `reject-undeclared-scalar-read`.

## Addendum (DG, 2026-09-13) — `<` and `>` are a byte compare

`[[ a < b ]]` / `[[ a > b ]]` order operands by **unsigned byte value**,
i.e. `memcmp`/`strcmp` order — equivalently bash under `LC_ALL=C`. There
is no locale-sensitive collation: the runtime is locale-free (no libc, no
`setlocale`, no `LC_COLLATE`), so byte order is the only ordering it can
express, and it is the only one that is reproducible across machines.

Consequence worth stating explicitly, because it is where a collating
locale would answer differently: **all upper-case ASCII sorts before all
lower-case** — `[[ Z < a ]]` is **true** (0x5A < 0x61). A `en_US.UTF-8`
bash says false. Spec `ft.40` pins this.

The same byte rule governs `==`/`!=` (decision 1 above) — those were
always `sash_strcmp` — so ordering and equality are now consistent with
each other. Both backends share the one runtime comparator.

Related: the file-age operators `-nt` / `-ot` compare mtime and `-ef`
compares (device, inode); `-nt`/`-ot` compare at **nanosecond**
(timespec) resolution with a strict `>`, which is what bash does — verified
three-way on files 1 ms apart inside the same whole second. (`-N` is the
operator with the `>=` subtlety; see `bug-file-test-n-diverges-from-bash`,
closed.) Both backends share the one runtime comparator, so c inherited
that correction for free. They are grouped here
only because they landed with `<`/`>` — c implemented none of the five
until 2026-09-13, silently answering **false** for every one.
