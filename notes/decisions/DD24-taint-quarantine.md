title: DD#24 — Taint Quarantine and Bless
status: implemented

All external input is tainted. Tainted variables cannot be used at any
site until blessed. This is the canonical taint/bless decision,
incorporating DD#43, DD#44, DD#47, DD#49.

## Taint sources

A binding acquires taint from:

- `x=$(getenv VAR)` — environment variable read
- `x=$(arg N)` — single program positional param (DD#60)
- `array a=$(argv)` / `array a=$(argv N)` — whole/sliced argv snapshot (DD#60)
- `read ARR` or `readline x` from any fd, including a builtin-produced pipe
- any kernel-backed `$( … )`, whether it captures an external command,
  a user function or an anonymous body

DD#127's boundary is the **kernel read**, not whether the intended producer
execs. A compiler-owned buffer capture introduces no taint; every pipe and
memfd drain does. `read SRC ARR` splits an already-held value rather than
reading a descriptor, so its source is checked as an ordinary value use.
Heredocs and here-strings are not part of the language (DD#83).

DD#127 says where output is attested: in the body that produced it or, when
that body has exactly one call site, immediately after the call. The same
rule covers what a function leaves in a global or a collection slot.
Read-boundary classification and shared-output ownership are implemented
(journals 560-561). An outside attest on a shared producer's kernel-read
output is rejected.

Acquisition is not the whole lifetime rule. Under DD#128, unresolved taint
survives an ordinary overwrite, unset, clean loop binding or buffer copy.
Only a legal adjacent attest or a clean whole-subject replacement written
directly on its failed branch resolves it. EXIT checks its actual incoming
states; ERR conservatively treats potentially tainted incoming globals as
unresolved, even if main normally attests them.

## Clean sources

Literals, arithmetic from clean operands, `$#`, `$?`, `$$`, `$!`, and
compiler-owned buffer captures introduce no taint. Assigning a clean
expression keeps a clean binding clean; it does not resolve an already
tainted live binding outside the failed-attest permission above.

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

There is no permission to propagate unresolved values through ordinary reads:
reject at the use, rather than accepting a copy and waiting for a later sink.
The analysis still tracks definition dependencies and unresolved state across
control flow, including DD#128's persistent taint after ordinary overwrites.

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
| `bless VAR [[ test ]]` | Reports validation failure and enters ERR with status 1 (DD#128) |
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
no `else` at all is SB5089. An `exit 0` path must additionally satisfy any
incoming clean-use requirements of EXIT; nonzero exit selects ERR. Other
transfers retain unresolved state at their real continuation (DD#128).

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

## Pipeline taint: the read boundary (2026-11-02)

Every pipe reader gets tainted bytes, including `echo hello | readline x`
and a user-function producer. The pipe is a kernel object, not an internal
buffer. The old exec-boundary exemption for builtin/function producers is
removed by DD#127. Attest beside the read, or use `:!` explicitly.

## Addendum (2026-10-26): a stage's marks do not change a pipe's taint

The then-current exec-boundary rule used the pipe's producer. Two classifiers
instead looked at a stage's `-`/`!` marks rather than the statement beneath
them. W2a removes that producer classification altogether; the history below
explains the regression cases, not an exception to the read boundary.

**A `-` on the source hid it (batch-531 B25, CRITICAL).** `is_external_source`
peeled a stage's redirect but not its marks, so `- /bin/cat f | readline x`
bound `x` clean and printed the file. The SSA backstop got the same answer
through `clean_pipe`, so neither layer caught it. Journal 537 fixed the
peel; W2a removes `clean_pipe`. The marked reader remains tainted (24.21,
24.27).

**A `-` or `!` on the reader hid it (B31).** `taint_stmt_no_read_taint`
recognised only a bare reader. Under the old rule, builtin-fed marked readers
drew false SB5020/SB5067 errors (24.22-24.25, 24.28, 24.29). W2a intentionally
taints all these reads; those acceptance fixtures must attest or explicitly
trust the data. DD#122 still rejects a redirected reader stage (SB4022).

Specs: taint.sh 24.21-24.29. Journal 537.
