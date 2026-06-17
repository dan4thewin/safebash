title: DD#62 — [[ ]] test simplification: exact-only ==/!=, =~ quoting, drop -v
status: candidate

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

### 1. `==` / `!=` / `=` are exact string comparison only (no glob)

Today the unquoted RHS of `==`/`!=` is glob-matched (`emit_eq`/`emit_ne`
call `sash_fnmatch`); a quoted RHS is exact. **Drop the glob branch:**
`[[ $x == foo ]]` and `[[ $x == foo* ]]` both become exact compares
(the latter against the literal three chars `foo*`). Quoting no longer
changes `==`/`!=` semantics. **Pattern matching is done with `=~`**
(regex), which is strictly more expressive and explicit.

`case` is unaffected — it lowers patterns to its own `=glob` operator
(`emit_glob`), which keeps `sash_fnmatch`. This decision is only about
`[[ ]]` `==`/`!=`/`=`.

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

sash has **no unset state**: an undeclared scalar reads as the zero
value (empty string / `0`), and `unset` of a scalar is rejected
(SB4030). So bash's `-v` (distinguish *declared-empty* from *undeclared*)
tests a distinction that does not exist. sash's actual `-v`
(`emit_var_set_test`) just tests non-emptiness — a misnamed alias for
`-n` — and is broken for the one case where bash's `-v` is useful
(array elements: `[[ -v a[0] ]]` returns false for a set element).
**Remove `-v`** (a use of `-v` becomes an unsupported-operator error).

## Architectural consequence (see the task for implementation)

With `==`/`!=` glob gone and the dead `=~` quoted-literal branch
removed, `right_is_quoted` has no reachable callers and is deleted. With
`-v` gone, the only `[[ ]]` operands that are *not* plain values are the
`=~`/`case` **pattern** operands (raw regex/glob text — never a
subscript). Every other operand (LHS of all ops; RHS of `==`/`!=`/`<`/
`>`; unary `-n`/`-z`/`-f`/… args; `case` subject) is a value, and value
operands can be lowered to `SsaValue` in transform. Then test
subscripts flow through the SSA path (`lower_subscript_value`), the
test-path `lower_subscript` has zero callers, and it is deleted —
closing the `dynamic-map-subscript-in-test` bug by construction.

## Open questions (resolve before the regex-quoting/sed work)

These were raised but not decided; the task's regex/sed spec tests are
blocked on them:

- **Meaning of `\`.** One coherent rule is needed. Is `\` a
  pass-through *engine* escape (so `\.` reaches Plan 9 regexp as
  "literal dot"), or a sash-level *literalizer* the engine never sees?
  This determines `\.`, `\/`, `\ `, `\\` uniformly.
- **`/` delimiter in `${var:s/re/repl/}`.** Today the parser splits on
  raw `/` with `find`/`rfind`, no escaping — a `/` in the regex or
  replacement corrupts the split (`${v:s/a\/b/c/}` → garbage). Decide:
  `\/` escape, alternate delimiters (`s|re|repl|`), or quoting.
- **Does quoting suppress metachar interpretation**, or only group
  whitespace? (Interacts with the `\` rule above.)
- **Undeclared scalar read** currently yields empty silently
  (`echo $foo` compiles). Adjacent to the zero-value model behind
  decision #3 — should an undeclared scalar read be a compile error
  instead of a silent empty? (Footgun in a "safe subset".)
