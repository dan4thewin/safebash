# 298 — A gate that was only prose

`converter-special-var-sugar` is the DD#75 §1 cleanup: bash's `$?`/`$$`/`$!`
are input sugar, the keyword forms `$(status)`/`$(getpid)`/`$(getcpid)` are
canonical, and `sash-convert` should emit the canonical spelling. Three
substitutions.

It took three bug fixes to get there, one of them in a task marked **done**.

## The mapping is context-dependent, again

The same shape as the bracket forms two days ago: one bash spelling, more than
one sash answer.

```
echo $?              ->  echo $(status)
n=$(( $? + 1 ))      ->  n=$(( (status) + 1 ))
```

`$(status)` inside `$(( ))` is SB4020 — a command substitution is not an
arithmetic operand. The DD#58 sexpr `(status)` is. The punctuation sugar reads
the same in both, which is precisely why a mechanical rewrite has to know where
it is standing.

`$#` already had this shape and already carried the warning, in a comment in
`convert_simple_expansion`:

> `(argc)` is the DD#58 arithmetic sexpr call. The command-substitution
> spelling `$(argc)` is not an arithmetic operand: it used to parse as literal
> 0, so a converted `$#` printed **-1**, and it is now a hard SB4020.

A note left by a previous fix, sitting six lines above where I needed it. That
is the system working.

These three also differ from `$@`/`$*`/`$#` in a second way: they are
process-wide, so they convert *inside* a function body, where the positionals
are the caller's arguments and must be left alone. The `in_function`
early-return had to move down.

## The gate that was only prose

Before emitting `(getcpid)` I measured it. It was 0.

```
/bin/sleep 1 &
echo $(getcpid)             -> 3827668
int n = $(( (getcpid) ))    -> 0
```

`bug-arith-keyword-pid-intrinsic-zero` is marked **done**, and its first gate
reads:

> `$(( (getpid) ))` / `$(( (getcpid) ))` / `$(( (random) ))` compute the value,
> not 0

Two of three. `resolve_nullary_sexpr` carries the admission in a comment —
"`getcpid` (`$!`) is excluded — filed" — and the filing it points at is the one
whose gate says otherwise.

The fix is one arm: `getcpid` reuses the `$!` slot the way `getpid` reuses `$$`
and `status` reuses `$?`.

### Why it was silent rather than wrong-loudly

The branch below those arms treats an unknown nullary name as a *variable*, and
that is correct: `$(( (x) ))` is arithmetic grouping. A misspelling lands there
and gets caught — `$(( (getpidd) ))` is SB5031, "assigned nowhere, possible
typo".

But `validate` exempts every intrinsic name from the typo check. So `getcpid`
— a name the compiler knows perfectly well — was waved past the check that
would have caught it *because* it is an intrinsic, and then not handled by the
transform that handles intrinsics. Two passes disagreeing about what the name
is, and the disagreement landing in the one branch that cannot report it.

That is a shape worth naming: an exemption granted on one pass's knowledge,
consumed by another pass that lacks it. The exemption is not wrong. The
assumption that both passes share a definition is.

### Why the deferral outlived the fix

No spec. The `sx.1`–`sx.11` block pins `argc`, `getpid`, `getppid`,
`epochseconds`, `status`, `random`, `signum`. `getcpid` appears in the gate
text, in the symptom table at the top of the filing, and nowhere in `spec/`.

A gate that is only prose does not fail. Sixteen days and one "done" later, the
only thing that noticed was a downstream task needing the value.

## The third bug: a rewrite that ate its own output

Adding the `$?` arm produced this:

```
if [[ $? -ne 0 ]]$(status) -ne 0 ]; then echo bad; fi
```

My first read was that I had broken something. I had not. At clean HEAD:

```
$ cat in.sh
if [ "$1" = x ]; then echo y; fi

$ sash-convert in.sh
if [[ $1 = x ]]$arg1" = x ]; then echo y; fi
```

`if [ "$1" = x ]` is not an exotic construct. It is close to the most common
line in shell scripting, and the converter had been silently corrupting it.
Every `[ … ]` containing a known env var, a positional, or a `${#s}` had the
same shape.

The cause is the one I described in journal 297 one day earlier, in the
abstract, while fixing the patch sort order:

> a whole-node replacement and a patch inside that node cannot both be applied
> — the inner one re-emits source the outer one already consumed

`convert_test_command` replaces the whole `[ … ]` node, working purely on its
source text. `convert_simple_expansion` patches `$1` inside it. Both fire; the
inner patch re-emits from the middle of a span already consumed.

Yesterday I fixed the *ordering* case of this and wrote down the general shape
without going to look for other instances of it. There was one, it was
pre-existing, and it was hitting a top-ten bash idiom. Writing down the general
shape is not the same as checking for it.

### The fix is composition, not suppression

Two wrong ways to fix this: drop the inner patches (`$HOME` would stay `$HOME`
and fail SB5018), or drop the outer one (`[ ]` stays `[ ]`).

The right way is to let the inner patches run first and hand their *result* to
the outer rewrite. `convert_test_command` now renders its children into text,
runs its existing `[ ] → [[ ]]` transform over that text, and returns a bool so
`walk_node` knows not to recurse into children whose patches are already
spliced in.

Two wrinkles, both found by testing rather than by reading:

* **The decision has to be made on the raw text, the splice used for the
  output.** Rendering has a side effect — the hoisted `home=$(getenv HOME);
  attest …` prefix line — and emitting that while *not* replacing the node
  would duplicate it when the normal recursion re-ran. So the sub-prefixes are
  held in a local vector and merged only when a replacement is actually
  pushed.

* **Prefix lines ride on the `Patch`, not only in the shared vector.** `convert`
  hoists `patch.prefix_line` in a loop over the final patch list — a spliced
  patch never reaches that loop. First cut of the fix produced correct code
  with the `getenv`+`attest` pair silently missing. Now `splice_patches` carries
  them across.

`--annotate` keeps the raw source as `# was:`, not the spliced intermediate.

## And a spec id collision, caught by accident

I added `sx.12`/`sx.13` after reading the `sx.1`–`sx.11` block. `sx.14`–`sx.23`
exist further down the same file. `chk` ids are a flat global namespace, so the
later one wins and the earlier result is discarded — silently, since both pass.

I only noticed because the totals were four short of the arithmetic: 4441 where
14 new tests over a 4431 baseline should give 4445. Four, not two, because both
modes double it.

AGENTS.md warns about exactly this and tells you to grep the prefix first. I
read a range of the file instead, which is the same check done by eye and is
not a check. The mechanical version is one line and belongs next to the
warning:

```sh
grep -ho '\bchk [A-Za-z0-9._]*' spec/*.sh | grep -v skip_chk | sort | uniq -d
```

Run against the whole suite it reports nothing — the `chk`/`skip_chk` pairs in
`compiler.sh` and `misc.sh` are mode guards where exactly one branch fires, not
collisions.

The count discrepancy is worth keeping as a habit. Predicting the total before
the sweep, and taking a mismatch seriously even when everything is green, is
what caught this. "All tests pass" would not have.
