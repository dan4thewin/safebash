title: DD#119 — a redirect target must be provably non-empty
status: accepted

1. Filename redirect targets must be string-typed and provably non-empty.
   Nonempty literals qualify; each string-variable or string-cell read needs
   its own nonempty-guaranteeing disposition.
2. Integer variables and cells qualify when explicitly quoted as strings.
   Concatenation does not let literal neighbors excuse an unguarded string read.
3. Direct command captures and intrinsics are not redirect-target operands.
   Bind an eligible value first and satisfy the same target rules.
4. Empty defaults and elision do not prove a usable filename. Target operand
   evaluation retains its own taint and failure checks.

---

Settled with DG (2026-09-26). DG's framing: *"what if we require any variable
that is a redirect target to use a disposition, save perhaps an observably
non-empty const, and excluded direct use of commands or intrinsics"*.

## How it started

Not from the redirect surface at all. DG had directed that sash have no
*magical variables* — every `$X` punctuation form should be **sugar** for a
proper intrinsic (`$?` for `$(status)`, `$!` for `$(getcpid)`, `$$` for
`$(getpid)`), rather than the intrinsic being rewritten into the variable, which
is the direction the compiler actually ran. Desugaring `$$` immediately broke
`/bin/echo hi > $$`, because the target became a command substitution and
`redirect_target_is_scalar` rejects those (SB4049) — a rule DG had set
deliberately on 2026-08-30 to close `bug-redirect-nonsimple-target-empty`.

Three ways out were put to DG: carve an exception for pure nullary intrinsics,
keep the rule and migrate the specs, or give intrinsics their own `ExprKind`
so the target is honestly a scalar. DG took none of them and reframed the
problem, which turned out to be the right move: the existing rule is a
*syntactic proxy* for the property actually wanted.

## The rule

> **A redirect target must be provably non-empty, and string-typed.**

The original bug was a redirect to an **empty filename**. "Is it a scalar?" only
approximates "can I show it is not empty?", and the approximation leaks in both
directions — it admits an empty `$p` and excludes a `$(getpid)` that cannot
possibly be empty.

A target is provably non-empty when it is:

1. **A non-empty constant** — a literal or quoted string with at least one
   character. `> out.log`, `> "/tmp/x"`. An empty `> ""` is rejected: it is
   observably the bug.
2. **A string variable read carrying a non-empty-guaranteeing disposition** —
   `${p:?msg}` (aborts when empty, so reaching the redirect proves non-empty)
   or `${p:-fallback}` where the fallback is itself a non-empty constant.
3. **An int variable read, double-quoted** — `"$p"` / `"${p}"`. An int needs no
   disposition because it is non-empty *by type*: it always renders at least one
   digit. It still needs the quotes, because quoting is how an int becomes a
   string everywhere else in sash (DD#80's map keys, the string-intrinsic
   operands). DG: *"exempt but still needs quotes to make a string"*.
4. **An array cell read with a disposition** — `${a[1]:?msg}`. A cell is a
   string read like any other, so it carries the same obligation.
5. **A concatenation of the above**, with each variable part satisfying its own
   obligation independently.

Everything else is rejected, in particular:

- a bare `$p` on a string — the empty case is exactly the bug;
- a command substitution `$(cmd)` — not provable, and DG's 2026-08-30
  reasoning stands verbatim: the compiler cannot know what filename a command
  would produce;
- an **intrinsic**, including the punctuation sugars `$$` / `$!` / `$?`;
- `${p:-}` — a disposition whose default is empty proves nothing;
- `${p:~}` — the elide disposition yields *nothing*, which is the bug itself.
  (Already SB5046 in the redirect position; the rule now explains why.)

## Why dispositions are the right gate

They are not ceremony. A disposition is the **proof mechanism** — the only
construct in the language that makes emptiness accountable at the point of use.
Requiring one is requiring the author to say what happens when the value is not
there, which is the question the old rule never asked.

This is also why `:-` with an empty default and `:~` must be excluded even
though both *are* dispositions: the test is not "did you write a disposition"
but "does the result prove non-empty". A rule phrased the first way would admit
`${p:~}` and reintroduce the original bug wearing the remedy's clothes.

## Strictness in concatenations

DG chose **strict**: every variable part needs its own disposition regardless of
its literal neighbours. The lenient reading — a non-empty literal anywhere
discharges the proof for the whole target — was rejected. Under lenient,
`> $p.log` with an empty `p` writes to `.log`: not the empty-filename bug, but a
silent hidden file in the working directory, which is its own surprise. Strict
costs an extra `:?` and removes the class.

## What this was measured against

Before the rule, both of these compiled and failed only at *runtime*:

```sh
str p=""
/bin/echo hi > $p        # ./t.bin: : No such file or directory
/bin/echo hi > ${p:-}    # ./t.bin: : No such file or directory
```

A clean runtime error rather than silent corruption — the 2026-08-30 fix
achieved that much — but the whole point of the type checker is that a program
which cannot name its output file should not build. Roughly 35 sites across six
spec files used a bare-variable target and were migrated.

## Relation to the no-magic-variables work

The rule **unblocks** it without a carve-out. `> $$` is rejected because
intrinsics are excluded, uniformly and for a stated reason, rather than because
of an accident of how the desugaring happens to be represented. No
`ExprKind::Intrinsic` node is needed to solve *this* problem — whether one is
worth having for the four passes that currently pattern-match "a
`CommandSubstitution` holding one nullary `Command`" is a separate question,
filed as its own task and explicitly **not** decided here.

## Diagnostics

The rejection is **SB4030**. That is not a free choice: the rule needs
`env.var_type` for the int exemption, so it lives in `type_check.rs`, and
`lib.rs:387` prints every `TypeError` under the single code SB4030. The message
names the specific failure (bare variable, empty default, elide, empty
constant, unquoted int) and gives the remedy for that shape.

This is a known wart — "the target is not provably non-empty" is not really a
*type* error — but inventing an SB5xxx here would mean either duplicating the
type environment into `validate.rs` or teaching `type_check` per-error codes.
Both are larger than this rule and neither is decided here.

A command substitution or intrinsic target is rejected earlier still, at parse,
by the existing SB4049 scalar rule — DD#119 does not weaken it.

## Not decided here

- Whether intrinsics deserve their own `ExprKind` (see above).
- ~~Here-strings (`<<<`) and heredocs are unaffected: they feed stdin *data*, not
  a filename, and were already excluded from the scalar rule.~~ DD83's
  producer/pipe contract governs stdin feeding.
