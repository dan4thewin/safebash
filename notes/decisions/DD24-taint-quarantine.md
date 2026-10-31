title: DD#24 — Taint Quarantine and Bless
status: implemented

All external input is tainted. Tainted variables cannot be used at any
site until blessed. This is the canonical taint/bless decision,
incorporating DD#43, DD#44, DD#47, DD#49.

## Taint sources

A variable is tainted iff its defining instruction is one of:

- `x=$(getenv VAR)` — environment variable read
- `x=$(arg N)` — single program positional param (DD#60)
- `array a=$(argv)` / `array a=$(argv N)` — whole/sliced argv snapshot (DD#60)
- `read x [...]` from any inherited fd, opened fd, or pipe
- `mapfile -t x` / `readarray` from any inherited fd, opened fd, or pipe
- `$(cmd)` where `cmd` is an external command

`read` / `mapfile` from a heredoc or here-string of clean data
produce clean values.

User-defined sash functions are trusted: `$(user_func ...)` capture is
clean. A function that touches a tainted source must bless it before
use or the function itself fails to compile.

## Clean sources

Literals, arithmetic from clean operands, `$#`, `$?`, `$$`, `$!`,
variables assigned from clean expressions.

(`$LINENO` was removed from this list — DG 2026-08-19 rejected it as a
magic var; it no longer exists in sash, so it is never a taint source.)

(`$?`/`$!`/`$$` are the punctuation **sugar** for the canonical clean
intrinsics `$(status)`/`$(getcpid)`/`$(getpid)` — DD#75 §1. The clean
status attaches to the keyword form; the sugar inherits it.)

Function positional params (`$1`..`$9`, `$@`) are clean within
functions — the compiler sees the caller's arguments and knows
their taint status. Top-level positionals are program argv (tainted,
use `$(arg N)`).

`$0` is not supported by the compiler — the converter rewrites it
to `$(arg 0)`.

For-in iteration is a **bind** or a **read** by iterand. Iterating a
*source* (`for x in $(argv)`, `while read x`) binds each element to the
loop var from the source — assignment-from-source, so per-iteration
body-bless is valid. Iterating a stored tainted *variable* (`for x in @a`
where `a` is a tainted named array) reads its elements (the `b=$a` case)
and is rejected; bless-all the array first (DD#61).

## Quarantine (DD#44)

Tainted data cannot be used at ALL use sites — not just dangerous
ones. Every operation other than direct assignment-from-source counts
as a read and is rejected:

- `$x`, `${x}`, `${x:op}`, `${x[i]}` in any expression
- `b=$a` — RHS read of tainted `a`
- `cmd "$x"` — command argument
- `arr[$x]=v` — index expression
- `(( x ))` / `$(( x ))` — arithmetic operand
- Passing as a function argument
- `join @arr` when any element is tainted

No propagation — taint is a property of a definition, not a flow.
Per-definition classification with use-site rejection.

### Enforcement note (2026-08-18) — the `b=$a` read was a hole

The `b=$a` case above was **not** enforced for scalar assignment until
2026-08-18: `taint_assignment` uniquely *propagated* taint (`b=$a` compiled
and marked `b` tainted, caught only at a later sink) instead of rejecting the
read, silently implementing the Perl-style flow model this DD explicitly
rejects. Every other read site (`arr=($a)` SB5021, `(( a ))` SB4030, `f $a`
SB5020, `m=([k]=$a)` SB5021) rejected correctly; scalar assignment was the lone
offender. Now `b=$a` (and `b="$a"`, `b=${a}`, `b=${x:-$a}`, `b+=$a`) is a
rejected read (SB5020) directing to bless-at-source. Assignment *from a source*
(`b=$(getenv X)`) is unchanged — that is the sanctioned way to obtain external
data. This also closed a related DD#78 hole (`b=${x:-$(getenv)}`, a source in a
`:-` default at a store site). See journal 149.

## Bless (DD#43)

Assign first, validate second:

```bash
port=$(arg 1)
bless port [[ $port =~ ^[0-9]+$ ]]
```

`bless VAR [[ test ]]` — VAR explicitly named, must appear in test
expression (DD#49: SB4012 rejects tests not containing `$VAR`).
Additionally, `$VAR` must appear on the LHS of the test operator, not
the RHS — `bless x [[ "$x" == "$x" ]]` is rejected because the var
appears on the RHS (tautological, no real validation).

The test expression is a `[[ ]]` test — string comparisons, regex
match, file tests only. No external commands. No arithmetic. The
tainted VAR is readable inside the test expression. Other tainted
vars are not (standard use-site rejection applies). (Enforced since
2026-10-31 by SB5098, strictly: see the addendum below.)

### Addendum (2026-10-31): a predicate runs no code (SB5098)

Nothing enforced "No external commands. No arithmetic." before 2026-10-31.
Taint (SB5020) was the only check on a predicate, and it asks a different
question: `attest x [[ $x == $(/bin/echo yes) ]]` reads no other tainted
name, and under `--no-taint` nothing looked at all. Spec 24.16, written for
this paragraph, passed on two SB5020s that have nothing to do with it: its
predicate never names `$x`, and its `echo $x` reads a tainted name. A
predicate that runs code never reached a runtime. The transform lowers a
predicate's operands in place and has nowhere to run a body, so each one
died in a backend "should have been lifted" ICE. The forms that did compile
were worse: a `$(getenv …)` or `$(<file)` comparand made the gate compare
the attacker's input against more of the attacker's input.

**SB5098** enforces the paragraph as a shape rule, strictly (DG
2026-10-31): a predicate *tests values; it runs no code*. The predicate is
the one place the attested name is readable while still tainted, so code
run there would receive the unvetted value.

- A `[[ ]]` predicate holds no `$( … )` of any kind. That covers a user
  function, an external and a statement builtin, and also `echo`, the value
  intrinsics and `getenv`. A predicate never needs a capture's value: bind
  it to a variable before the statement that taints the name, and compare
  against the variable.
- The `${…:s/…/…/}` sugar is a `$(sub …)` call (DD#63), so it is SB5098
  too, and the message names both spellings. `=~` stays legal: it is a
  test.
- No `$(<file)`. A file's contents are tainted, like `getenv`'s value.
- No `$(( … ))` at any depth. "No arithmetic" covers an operand, an index
  and a `${…}` default alike.
- DD#93 Part D's `(( t = s ))` is arithmetic by construction, so the
  sentence cannot mean every arithmetic. There the rule is that the value
  calls nothing: no sexpr call of an intrinsic or a function, no bare
  int-function name, and the `[[ ]]` rule again inside each `${…}` the
  value imports. A `(name)` naming neither is grouping around a variable
  read (DD#58), not a call, and stays legal: `(( c = n + (d) ))` (cov.278).

Each banned node is reported separately, a nested one included, at the
position the parser gives it. Inside an index or a quoted word, that is the
`${` or the `"` that holds it; anywhere in a Part D value, it is the attest.
A shape rule runs under `--no-taint` too. Where the node is also a taint
source (`getenv`, `$(<file)`), DD#93's predicate-reads rule reports SB5020
beside it.

Considered and not taken: a whitelist of pure value intrinsics evaluated in
place, which an earlier draft of this rule allowed (with `echo`, `getenv`
and `$(<file)`). Nothing is evaluated in place now, so the old question of
reading the attested name inside a capture's arguments no longer arises.

Specs taint.sh 24.16, atpred.1, atpred.3–8 and atpred.13–31. The branch's
atpred.2 and .9–.12 are not ported: they pinned evaluation in place.
Journal 552.

## Runtime enforcement (DD#47)

Two forms:

| Form | On failure |
|---|---|
| `bless VAR [[ test ]]` | Writes "bless: validation failed" to stderr, exits 1 |
| `if bless VAR [[ test ]]` | False branch taken, VAR remains tainted |

On success, VAR becomes clean from this point forward (new SSA
version). A later re-assignment from a tainted source re-taints it.

### Amendment (2026-09-01, DG): bless in a caught `&&`/`||` position

A `bless` as the **left operand of `&&`/`||`** is a *caught* gate — exactly like
`if bless` — not a bare abort. This makes the **validated-or-default** idiom
fall out of the existing control-flow model, with no new operator:

| Form | On failure |
|---|---|
| `bless VAR [[ t ]] \|\| VAR=DEFAULT` | run the fallback; VAR := DEFAULT (clean) |
| ~~`bless VAR [[ t ]] && CMD`~~ | **superseded by DD#121** — see below |

`bless port [[ $port =~ ^[0-9]+$ ]] || port=8080` yields `port` when it validates
(clean, blessed) and `8080` otherwise (clean literal) — usable either way. The
fallback is **trusted, not re-validated** (DG: caveat programmer — a default
that wouldn't pass the gate is the author's responsibility). This supersedes the
earlier unbuilt `${var:~DEFAULT}` sketch (journal 036); `:~` stays the DD#84
elide operator.

**Superseded in part by DD#121 (2026-09-30).** A *caught* attest must now leave
its subject clean on every path that falls through to the merge, so the `&&`
row above is rejected outright (SB5089): `&&` has no failure branch, and its
implicit fall-through resolves nothing. The `||` row stands unchanged — the
fallback assignment is exactly the resolution DD#121 R2 asks for. The
`if bless VAR [[ test ]]` row above likewise stands for the **written** else:
VAR is tainted inside that branch, and the branch must resolve it (or `exit` /
`return` / `break` / `continue`) before falling through. A caught attest with
no `else` at all is SB5089.

Lowering (`transform/statements.rs::lower_list_stmt`): a bless left operand of
`&&`/`||` is rewritten to the if-bless form (`bless … || X` → `if bless {} else
{ X }`; `bless … && X` → `if bless { X }`), reusing `lower_if_bless`
(branch-on-test, retag-clean-on-pass). Previously it lowered as the abort form,
so `||` never caught it: it aborted on failure and, on **success**, the branch
read a stale status and wrongly ran the right side. specs taint.sh bfb.1–6.

## Auto-bless

`known_externals.rs` provides auto-bless patterns for 26 environment
variables and 25 commands with conservative regex patterns.

## See also

- DD#42 — getenv/argv as explicit-origin builtins
- DD#53 — Env var isolation (sash vars and env vars are separate namespaces)

## Containers

Arrays and maps: each element carries its own taint by the same
rules as a scalar. Reads of tainted elements (including `join`,
iteration, subscript) are rejected unless blessed.

### Amendment (2026-08-26): reads are use-sites in EVERY position

The use-site list above (`b=$a`, `cmd "$x"`, arith operand, function
arg, `join @arr`, …) is **position-uniform**: a read of tainted data
is rejected wherever it appears — not only in argument position. In
particular a command substitution that *reads* already-tainted data
(`$(join @a)`, `$(join a)` bare-name, `$(echo $tainted)`) is a
use-site in the **assignment RHS** (`x=$(join @a)`), **subscript /
array-literal element** (`a[i]=…`, `array c=(…)`), **`[[ … ]]` test
operand**, **redirect target** (`> …`), and **nested** cmdsubs — the
same as argument/for-word position. Likewise a *direct* tainted
scalar in a `[[ ]]` operand or a redirect target is a read.

DG's model: every `$( )` is internally `_tmp=$(…)` and reuses the one
read-checking machinery, whichever position it sits in. A bare
*source* (`x=$(getenv X)` / `$(arg N)`) remains the sanctioned way to
obtain external data (it taints the target); only a *read of
already-tainted data* is rejected. `len` stays exempt (cardinality,
not element data); `join`/`keys` expose element data and are reads.

This closed a laundering-bypass class where those positions silently
compiled and leaked at runtime. CST is the authoritative gate
(clean SB5020); the SSA pass is defense-in-depth (see task
`ssa-taint-cmdsub-operand-propagation`). Spec: taint.sh 60.37–60.46.

## Clarification (2026-05-24): Pipeline taint and exec boundary

"Read from pipe" taint depends on whether the pipe SOURCE execs:

- `echo "hello" | read x` → clean (echo is a builtin, no exec,
  compiles to internal buffer)
- `printf "k=v\n" | mapfile -m m` → tainted (printf execs)
- `/bin/cmd | read x` → tainted (absolute path, execs)
- `user_func | read x` → clean (sash function, no exec)

The principle: taint comes from exec boundaries (fork+exec), file
reads, and env var reads. A pipe from code that runs inline within
the sash process is not an external source.

## Addendum (2026-10-26): a stage's marks do not change a pipe's taint

The clarification above decides a reader's taint by whether the pipe's source
execs. Two classifiers answered by looking at a stage's `-`/`!` marks instead
of the statement beneath them.

**A `-` on the source hid it (batch-531 B25, CRITICAL).** `is_external_source`
peeled a stage's redirect but not its marks, so `- /bin/cat f | readline x`
bound `x` clean and printed the file. The SSA backstop gets the same answer
through `clean_pipe`, so neither layer caught it. The classifier now peels
through `Statement::stage_body`, the one peel every stage classifier shares,
and `x` is tainted (24.21). It stays tainted when the reader is negated as
well (24.27).

**A `-` or `!` on the reader hid it (B31).** `taint_stmt_no_read_taint`
recognised only a bare reader, so a marked reader fed by a clean producer was
tainted anyway. `echo hi | - readline x`, `- read ar`, and `! readline x` or
`! read ar` in a condition drew a false SB5020 (24.22-24.25). The same readers
in a return producer drew a false SB5067 (24.28, 24.29). The function now
looks beneath `stage_marks`. An external producer still taints a marked reader
(24.26, 24.27). A redirect is not peeled: a redirected reader's input is not
the pipe, and DD#122 rejects that stage (SB4022, DD#122 addendum 2026-10-26).

Specs: taint.sh 24.21-24.29. Journal 537.
