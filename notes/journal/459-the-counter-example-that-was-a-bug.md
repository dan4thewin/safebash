# 459 — the counter-example that was a bug

DG, in six words: *"your counter-example to make a tainted map?"*

I had claimed, a few turns earlier and with some confidence, that `keys` is
unconditionally clean because **a tainted map is unconstructible**. The claim
was cited to DD#93, which does say so — *"Every route to a tainted map is
closed at construction"* — and I had not checked a single one of them. DG's
question is the only part of this session's work that was not mine: the three
defects below all came out of trying to answer it.

I found the counter-example. It was a bug.

## 1. The vouch that evaporated at the map boundary

```sh
array a
read a
map m = @{a:!~}
for k in $(keys m) { echo $k }
```

DD#91's `:!` is the taint-off disposition: the value *"is treated as clean"*
and *"does not propagate taint"*, and the DD is explicit that *"the taint
bypass is applied in **both** taint passes so they agree."*

They did not agree. This compiled past `validate` and died at the **use**:

```
tainted variable k_3 used as an argument to 'echo' without attest
(SSA taint backstop)
```

No SB code, no span, an internal SSA version name for a variable the author
called `k`. The backstop is a last-resort net; reaching it at all means the
front end believed something the back end did not.

The same `:!` in a command-argument position — `/bin/echo @{a:!~}` — was
accepted and clean. Only the array→map conversion lost it, and it lost it into
a *builtin* `echo` too, so it was not an extern-argument artifact.

`--emit-ir` showed the IR carrying the vouch faithfully:
`:operator "arrvals_default" :taint-off true`. The loss was below that.
`SsaInst::MapFromArray` has a `mode` field whose doc reads `2  :~/:!
(discard)` — the two spellings **collapse to one value**, and the instruction
had no field for the vouch at all. `map_from_array_parts` returned
`(source, mode, key, message)` and dropped `pe.taint_off` on the floor;
`ssa_taint.rs` then propagated the source array's taint unconditionally.

A `bool` on the instruction, populated in the transform, consulted in the taint
arm. Three lines of substance.

**What it does to DD#93.** The DD's table of routes predates DD#91 — `:!` is
genuinely a fourth route it never considered. But fixing it does not falsify
the DD, it *restores* it: a vouched map is **clean**, not tainted. With the fix
in, I checked all four by hand: `@{a:~}` / `@{a:-X}` → SB5020 at construction;
`m[k]=$tainted` → SB5021 at construction; `read m` / `argv m` / `match … m` →
SB4030 (which is defect 2, below); `@{a:!~}` → clean, by the author's explicit
vouch. The claim holds. It just wasn't true when I made it.

### The gate made the fix better

The commit was **rejected by the pre-commit complexity gate**. Adding a
two-branch `if` inside one arm of `classify_inst_def` took it from cog 2 to
cog 4, past the `cog ≤ 2` flat-dispatcher exemption DG added on 2026-09-16.

The right response was not `SKIP_COMPLEXITY_GATE=1`. The file already has
`elide_taint` and `splat_taint` — helpers that exist for exactly this reason,
one arm's worth of logic lifted out of the dispatcher. `map_fill_taint` is the
third. `classify_inst_def` came out at **cc 11 / cog 1, lower than before the
change**.

## 2. Every bulk fill accepted a map target

Probing route three is what found this.

```sh
map m
read m
echo "n=$(len m)"
```

Compiles clean. A collection fill writes DD#33 raw-8-byte-i64 keys `0, 1, 2 …`
— and because the target was already declared `map`, the auto-declare to array
is skipped and those integer keys land in a string-keyed table. The key becomes
**eight NUL bytes**. A DD#87 breach, reachable from ordinary safe code, with no
unsafe construct anywhere in the program; it surfaces much later and in an
unrelated place as the runtime abort *"argument contains a NUL byte and would
be truncated at exec"*.

DD#99 settles the rule in as many words — *"maps are built by assignment, not
by read"* — and records DG rejecting a one-step read-into-map form as implicit
polymorphism. **Nothing enforced it.** All six spellings were affected: `read`,
`read SRC`, `argv`, `argv N`, `match … m`, `matchall … m`.

The telling part is where the fix went. `check_command` **already** dispatched
all four commands to `register_collection_target`, at exactly the right arities
(target is the last argument in every case; `match`/`matchall` only at arity 3).
Nothing new had to learn the dispatch table. The bug was one guard, written
against the wrong half of the type lattice:

```rust
if env.vars.contains_key(&name) && !env.collections.contains(&name) { … }
```

That rejects a **scalar** target. A map is in `collections` too — so it passed
the guard, and `env.assoc_arrays` was never consulted on any path.

`readline` is the mirror and went in with it. It fills a *scalar*; given an
array or a map it compiled and **silently discarded the line** (`len=0`). Same
missing check, opposite direction, and the quieter failure of the two.

## 3. The arm that called an array a map

Route three again, from the other end. `check_collection_reader_taint` had an
arm for `keys` that fired three ways wrong:

- `keys <tainted array>` → *"keys of tainted **map** 'a'"*. It is an indexed
  array. The diagnostic names the wrong type.
- It **masked defect 2.** `validate` runs before `type_check`, so for
  `map m; read m; for k in $(keys m)` the bogus SB5020 preempted the correct
  SB4030 — the real bug was being hidden by a message about a condition that
  cannot occur.
- Following its advice (`attest k v in a`) lands on a *different* SB5020. A
  dead end.

With DD#93 restored, the only collection that could ever reach that arm was a
tainted **array**, where SB4030 is the honest answer. Deleted. `join` stays —
arrays genuinely can be tainted, and `taint.sh` 60.37-60.41 pin it, so a spec
control went in to make sure deleting both at once cannot pass.

### What the deletion uncovered

With the mask gone, `t1` reported SB4030 **twice**.

It is specific to a `$( )` in a **for-in header** — `join`/`len` through an
assignment report once. Two walkers overlap there: `check_for`'s words loop,
and `check_stmt`'s unconditional `cmdsub_bodies()` walk whose comment carefully
reasons about `Command`, `Declaration` and `Assignment` and never mentions
`For`.

I did not fix it. The two walkers are **not equivalent** — only
`check_subscript_in_stmt` calls `check_len_operand`, and only `check_command`
does fill-target registration and `check_int_sink_operands` — so "delete one"
is not available, and the merge option would widen where `check_len_operand`
fires and could newly reject programs that compile today. That is a structural
fork with a behaviour-change risk, which is DG's call, not mine:
`notes/tasks/duplicate-type-error-in-for-header-cmdsub.md`, three options, no
recommendation.

## Things learned that are not about these bugs

**SB4070 is universal.** `:-` defaults and `:?` messages are literals
*everywhere* since 2026-09-30, not just in map fills. I went looking for
whether a tainted *key* could ride in alongside the vouch — the carve-out
`splat_taint` documents for `:-WORD` — and found the shape unreachable. Which
means `splat_taint`'s careful handling of a tainted `:-WORD`, and the
`array b=( @{a:-$tainted} )` example in its own doc comment, are dead for the
variable case. A coverage finding I deliberately did not chase.

**`run_sash` feeds the binary `</dev/null`.** So `read` sees EOF and a spec
built on it asserts nothing while passing. The `getenv HOME` vehicle that
bkstp.9/12 already use is the fix; a clean pipeline (`echo hi | readline v`)
is the other, since a clean producer makes a clean pipe under DD#20 and adds
no taint noise.

**`taint_switch.sh`'s `--no-taint-cst` cohort is the right vehicle for any
front-end/backstop agreement spec.** It disables the front-end pass and leaves
the backstop as sole judge — far cleaner than contriving nested `$( )`.

**`read` splits on NUL only** (DD#99), so `printf 'k1 v1 k2 v2\n'` is **one**
element, not four. An odd count silently changes which disposition branch runs.
Three correct results looked wrong before I noticed.

**Diagnostic ordering is a measuring hazard, generally.** `validate` before
`type_check` means a taint diagnostic masks a type diagnostic for the same
program — defect 3 masking defect 2 is the instance here, but the general rule
is that when you are measuring *which* rule fires, vary the **use** site, not
just the construction. `map m; read m` showed SB5020 or SB4030 depending only
on whether the probe went on to read the keys.

## Gate

Sweep **7345** (3775 native, 3557 c, 13 skipped), zero failing, predicted
exactly: 7303 + 2×(15 + 6). `mk test` rc=0. All three spec-hygiene checks
clean.

Specs: `taint_switch.sh` bkstp.13-18 (plan 37 → 43), `types.sh` 75.117-75.131
(plan 292 → 307).
