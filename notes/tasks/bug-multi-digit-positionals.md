title: "Bug: `${10}` and `${01}` read an unbound variable inside a function; four functions disagree about what a positional is"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "DG chooses (a) or (b) under Decision needed; the ruling is recorded as a DD75 addendum that also settles DD75's `$1..$n` / `$1..$9` disagreement"
  - "one shared function decides whether a name is a positional and which argument it reads; the read, the arity scan, SB5024 and `classify_read` all call it"
  - "every program in the table reads the argument bash reads or is a compile error, in both backends; none reads an unbound variable"
  - "a spec pins each row in both backends; each fails on the pre-fix binary"

## What

The results are the same in both backends.

| Program | sash | bash |
|---|---|---|
| `fn f { echo "[${10}]" }; f` | `[]` | `[]` |
| `fn f { echo "[${10}]" }; f a b c d e f g h i j` | SB5043 "function 'f' takes exactly 0 arguments but was called with 10" | `[j]` |
| `fn f { echo "[${01}]" }; f 4` | `[]` | `[4]` |
| `fn f { echo "[$(( ${01} + 1 ))]" }; f 4` | `[1]` | `[5]` |
| `fn f { echo "[$(( ${10} + 1 ))]" }; f` | `[1]` | `[1]` |
| `fn f { echo "[${09}]" }; f a b c d e f g h i` | SB5044 at 1:1, "function 'f' reads args non-contiguously — argument 0 is unused" | `[i]` |
| top level `echo "[${10}]"` | SB5024 at 1:6, "$10 is function-local — use $(arg 10) at top level" | |
| top level `echo "[${01}]"` | SB5024 at 1:6, "$01 is function-local — use $(arg 01) at top level" | |

Bash function syntax is `f() { …; }`. The two rows that agree with bash do
so only because no argument is passed. `${10}` reads a variable called
`10`, which nothing binds, so a function that reads it takes no arguments.

At top level, SB5024 treats `${10}` and `${01}` as positionals, and the
suggested `$(arg 01)` compiles and reads argument 1. Inside a function, the
same names read a variable. The message also names the unbraced spelling
`$10`, which in bash and in a sash word is `$1` followed by `0`.

## Cause

Four functions decide what a positional name is, and they disagree:

| Function | Decides | Accepts |
|---|---|---|
| `positional_arg_index` (`sash/src/transform/expressions.rs`) | the read | exactly one byte `1`-`9` |
| `positional_index_1_9` (`sash/src/transform/functions.rs`) | the arity count (SB5043, SB5044) | any name that parses as a number from 1 to 9, so `01`, `09`, `001` |
| `flag_positional` (`sash/src/validate.rs`) | SB5024 at top level | any all-digit name except `0` |
| `classify_read` (`sash/src/validate.rs`) | the typo check (SB5031) | none: a name that is not an identifier is `Silent` |

So `${01}` counts as argument 1 for arity, which is why `f 4` is
accepted. The read does not take it as a positional, so it reads the
variable `01`, which is empty. `${09}` counts as argument 9 with arguments
1-8 unused, hence SB5044. `${10}` counts as nothing.

This is the private-duplicate shape that
`sweep-private-duplicates-of-shared-walkers` tracks: one question with
four answers.

## Decision needed

DD75 says two things. Line 147 has "The `$1..$n → ${args[0]}..${args[n-1]}`
sugar (§5)", and the table at line 163 has
"| `$1`..`$n` | `${args[0]}`..`${args[n-1]}` | subscript |". Line 205 has
"`$1..$9` → `${args[0]}..${args[8]}`".

- **(a)** Accept `${N}` for every N ≥ 1, leading zeros included, as
  `${args[N-1]}` in words and in arithmetic. This is DD75's `$1..$n`, and
  it is what bash does: `${10}` is argument 10 and `${01}` is argument 1.
  Inside a function, sash-convert already passes `${10}` through unchanged.
  Unbraced `$10` stays `$1` followed by `0`, as in bash.
- **(b)** Accept only `${1}` to `${9}`, and reject any other all-digit name
  with SB4047, pointing at `${args[N-1]}`. This is DD75 line 205. A
  converted bash function that reads `${10}` would then need a manual edit,
  or a converter rewrite to `${args[9]}`.

Recommendation: (a). It removes a limit rather than adding a rule, it
matches bash, and the converter's output keeps compiling.

## Fix (proposed, not yet ruled)

- One shared function, for example `ir::positional_index(name) ->
  Option<usize>`, answers "is this a positional, and which one". The four
  functions above call it, with no private copy.
  - Under (a), it accepts any all-digit name except `0`.
  - Under (b), it accepts `1`-`9`, and the parser rejects every other
    all-digit braced name before any pass sees it.
- The arithmetic reader of `${N}` (`braced_body_token`, `parse/arith.rs`)
  follows the same rule. `bug-digit-led-names` covers the digit-led names
  that are not all digits.
- SB5024 names the form the user wrote: `${10}`, not `$10`.

Found during the batch-531 port (step F14, finding B38). Filed under DG's
rule for the port that questions and uncovered cases become backlog tasks.
