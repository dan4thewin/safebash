title: DD#119 — a redirect target must be provably non-empty
status: accepted

1. Filename redirect targets must be string-typed and provably non-empty.
   Nonempty literals qualify; each string-variable or string-cell read needs
   its own nonempty-guaranteeing disposition unless the direct immutable
   scalar exemption below applies.
2. Integer variables and cells qualify when explicitly quoted as strings.
   Concatenation does not let literal neighbors excuse an unguarded string read.
3. A directly tested immutable scalar binding instance may omit its
   disposition when success holds on every reaching path. A proof is
   available in its source body and lexically nested captures that inherit
   the same binding, not through named helper calls.
   Only simple unary tests qualify: `-n`, `cpath?`, `p?`, `g?`, `-e`, `-f`,
   `-d`, `-r`, `-w`, `-x`, `-s`, `-L`, `-h`. Scalar attest shorthand and an
   equivalent full predicate follow the same rule.
4. That exemption permits exactly one read of the proved binding, with
   optional literal text added only at the redirect site. Quotes and braces
   preserve the read; copies, derived bindings, named helper boundaries,
   collections and cells do not transfer the proof. A capture's own proof
   does not escape to its parent or siblings. Compound predicates,
   comparisons and deductions from a false test do not establish it.
   Fresh lifetimes need their own proof.
5. Direct command captures and intrinsics are not redirect-target operands.
   Bind an eligible value first and satisfy the same target rules.
   Cell indices may contain otherwise-legal captures: selecting a cell is
   distinct from using a capture directly as the filename.
6. Empty defaults, elision, forced reads and taint-disable switches do not
   establish nonemptiness. Target operand evaluation retains its own taint
   and failure checks; an ordinary test does not attest tainted data.

---

Settled with DG (2026-09-26). DG's framing: *"what if we require any variable
that is a redirect target to use a disposition, save perhaps an observably
non-empty const, and excluded direct use of commands or intrinsics"*.
On 2026-11-25 DG selected the narrowly bounded direct-test exemption below,
explicitly rejecting transitive property tracking.

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

1. **A non-empty literal** — a literal or quoted string with at least one
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
5. **A directly tested immutable scalar**, under the bounded rule below.
6. **A concatenation** of disposition-bearing or quoted-integer parts, with
   each variable part satisfying its own obligation independently; or exactly
   one directly proved immutable scalar with literal neighbors at this use.

DG clarified on 2026-11-17: **allow captures in guarded cell indices; keep
direct capture targets forbidden.** For example,
`> "${paths[$(- sub aba a X)]:?missing}"` selects a cell and then requires
that cell's filename to be nonempty. The index follows ordinary expression,
taint and failure rules; neither the disposition nor handling the redirected
command catches an index operand's failure. DD86's argument-before-redirect
contract still applies. This does not admit a direct `> $(...)` target or
an unguarded string-cell read.

Everything else is rejected, in particular:

- a bare `$p` on a string without the direct immutable proof — the empty
  case is exactly the bug;
- a command substitution `$(cmd)` — not provable, and DG's 2026-08-30
  reasoning stands verbatim: the compiler cannot know what filename a command
  would produce;
- an **intrinsic**, including the punctuation sugars `$$` / `$!` / `$?`;
- `${p:-}` — a disposition whose default is empty proves nothing;
- `${p:~}` — the elide disposition yields *nothing*, which is the bug itself.
  (Already SB5046 in the redirect position; the rule now explains why.)

## Direct immutable proof

DG's requirement is deliberately local:

> Only the proven nonempty immutable value can be used sans dispositon as
> the redirection target, with optional concatenation of literals.

He clarified that the concatenation belongs **only at the redirect site**,
and limited the predicates to obvious unary tests in simple usage. This is
an ergonomic allowance, not an attempt to recognize every logical proof of
nonemptiness.

```sash
_dir=$(- arg 1)
attest _dir cpath?
echo message > $_dir/log
```

The whole predicate must directly test the scalar with one of:

```text
-n cpath? p? g? -e -f -d -r -w -x -s -L -h
```

A simple ordinary `[[ ... ]]`, a scalar attest shorthand, and its equivalent
full unary predicate qualify. The redirect must be reached only after a
successful test of that same immutable binding instance. The proof may
come from the same body or a lexically enclosing body across capture
boundaries, but never across a named helper call.
Ordinary consts qualify as well as scratch; an immutable initializer known
to be nonempty does not qualify without the direct test.

```sash
_file=$(- arg 1)
attest _file cpath?
text=$(/bin/cat < $_file)
```

The capture reads the same live `_file`, so no property is transferred to
a new value. This also applies through nested captures. A proof established
inside a capture stays there and in its own nested captures; it does not
become available in the parent or a sibling capture. DG selected this
lexical allowance after the initial body-local implementation rejected an
otherwise qualifying captured redirect. The captured text still needs its
ordinary taint validation before use.

Quoting, braces and empty literal wrappers do not change the tested value.
Testing `$_dir/suffix`, `${_dir:-fallback}` or `${_dir:?message}` does not
test the binding alone. Negated `-z`, comparisons, compound predicates and
unlisted unary operators confer no new proof.

At the redirect, `$_dir/log` and `"prefix${_dir}/log"` qualify. There must be
exactly one dynamic read: even two reads of the same proved binding do not
qualify together. Assigning `_file=$_dir/log` does not transfer the proof;
`_file` needs its own qualifying test or a disposition. Copies in either
direction, function arguments/results, named helper calls, collection attests
and cell reads similarly confer no certificate on another binding or body.
Revisiting scratch construction starts a new instance that must be tested
again. These restrictions do not replace the existing per-part disposition
route for multipart targets.

The retained property is **nonemptiness only**. A file may disappear after
`-f`; appended literal text may cease to satisfy `cpath?`. Neither continued
existence, authorization nor conservative-path spelling is promised for the
resulting target.

Taint and DD132's caught-scratch read permission remain independent.
An ordinary test does not declassify its operand. A transparent `:!` read
may explicitly override taint while testing or using the same bytes, but
does not itself prove nonemptiness. The taint switches cannot manufacture
this proof either.

## Why dispositions remain the general gate

They are not ceremony. A disposition is the general **proof mechanism** that
makes emptiness accountable at the point of use.
Requiring one is requiring the author to say what happens when the value is not
there, which is the question the old rule never asked. The direct immutable
exemption permits a visible earlier test of the same immutable binding,
including use in its lexically nested captures, without propagating facts
through a chain of values.

This is also why `:-` with an empty default and `:~` must be excluded even
though both *are* dispositions: the test is not "did you write a disposition"
but "does the result prove non-empty". A rule phrased the first way would admit
`${p:~}` and reintroduce the original bug wearing the remedy's clothes.

## Strictness in concatenations

DG chose **strict**: literal neighbors do not discharge a variable's obligation.
Except for the single directly proved immutable read above, each string part
needs its own disposition. The lenient reading — a non-empty literal anywhere
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

The rejection remains **SB4030**. `type_check.rs` handles string typing,
integer quoting and the literal/disposition rules. It defers eligible
one-string-binding shapes to the shared pre-fold SSA proof analysis.
Original predicate and redirect metadata retain the source shape, body
identity and target span before Boolean lowering or temporary spills can
erase them. Unused definitions and unvisited targets after a terminal
command do not silently escape the deferred obligation.

The message names the failure and gives its remedy: a nonempty-guaranteeing
disposition, or the bounded direct immutable test where applicable. Moving
that part of the check later does not introduce a new diagnostic code or
relax taint, caught-read permission or operand failure handling.

A command substitution or intrinsic target is rejected earlier still, at parse,
by the existing SB4049 scalar rule — DD#119 does not weaken it.

## Not decided here

- Whether intrinsics deserve their own `ExprKind` (see above).
- ~~Here-strings (`<<<`) and heredocs are unaffected: they feed stdin *data*, not
  a filename, and were already excluded from the scalar rule.~~ DD83's
  producer/pipe contract governs stdin feeding.
