# 450 — The version that was defined on only one path

DG's answer to the SB5067 discussion was "yes to (a) + (ii)". Two halves of one
idea: relax the rule that a function may not taint a global, and close the hole
that made the rule load-bearing in the first place.

## (a) — the rule was syntactic, and the syntax was the wrong thing to check

SB5067 fired on the *act* of tainting a global inside a function body. That was
written before DD#121, when there was no way to undo a taint at the point it
happened, so the act and the leak were the same event. Adjacency changed that:
an attest must now immediately and unconditionally follow the operation that
taints a variable, so a body can taint a global and then clean it before the
next statement runs.

The rule is now about the body's **net effect**: a function may not *exit* with
a global tainted that it did not *inherit*.

Not "clean at exit" — that would be unsatisfiable. A body inherits the caller's
tainted set, and R1 forbids cleaning an inherited taint (the attest would not
be adjacent to anything), so the first global tainted at top level would put
every function in the program permanently in violation. Paired with R1, the net
effect of a body is exactly nil, which is what lets `check_taint` keep its
fixpoint-free two-phase shape.

The probe that mattered was the bypass: top level taints `g`, attests it clean,
then a function re-taints it. Rejected — the attest removed `g` from the entry
set, so the re-taint is a net insert. Both scalar and array forms.

Spans survive the change because the sites are gathered *before* the walk, with
their spans, and the verdict is passed after; only names still dirty at exit
are reported, deduped in reverse so the last responsible site wins. An `if`
that attests and an `else` that leaks blames the `else`.

`cov.266` broke in an instructive way. Its program was `argv g; f() { argv g;
}; f` — but the global is pre-tainted at top level, so under the net-effect
rule the body *inherits* it, nothing fires, and the program falls through to
SB5050 dead-variable instead. The test had never read the variable it wrote.

## (ii) — the naive fix could not have worked, and it said so

Two previous passes over the SSA taint backstop recorded that giving the bulk
fills taint "did NOT work" with a `taint.insert("{array}_0", Tainted)`. Neither
recorded why. The why is short: `autodeclare_bulk_target` emits `Assign {
target: a_N, value: IndexedArray([]) }` for the implicit declaration, that is
classified **Clean**, and `taint_of_name` takes the **max** version. A `_0`
insert is outranked by the empty declaration that necessarily precedes the
fill. It could never have worked.

So stop fighting the versioning and use it. `ArgvFill`, `ReadArray`,
`SplitValue` and `MapFromArray` now carry `target: VarRef`, and each
construction site calls `state.define()` after the autodeclare:

```
a_1  Assign(IndexedArray [])   Clean     the autodeclare
a_2  ArgvFill                  Tainted   the fill
a_3  AttestRetag               Clean     an attest, if present
```

The max-version rule then gives the right answer at every point with no
special-casing. It also answers a question the task file had left open — it
asked for an "attest-clears check to avoid false-positives on blessed arrays".
There is nothing to add: the attest already defines a strictly higher clean
version.

`ReadArray` needed a `clean_pipe: bool` it did not have. It participates in
lastpipe like `ReadLine` does, but carried no flag, so classifying it
unconditionally Tainted would have rejected `echo a b | read g` — a builtin
producer, genuinely clean.

## The segfault

The sweep went from 0 failures to 54. Fifty-three native SIGSEGVs.

The matrix said something precise: `for`-in and subscript reads of a
bulk-filled collection survived, `$(len a)` did not, and `len` of a *literal*
array was fine. Explicit declaration versus autodeclare made no difference; an
intervening attest did not help.

That shape is a slot-resolution difference, and it was. `for`-in and subscript
resolve a collection by **name**, through `find_array_slot`. `load_var_or_empty`
resolves by **version**, `vars[&var.display()]`. And `map_inst_versioned`
enumerated exactly three instructions — `Assign`, `ReadLine`, `CaptureEnd` — so
the new `a_2` was never mapped.

The miss is silent. `load_var_or_empty` returns a null pointer and zero length
when a variable is out of scope, which is correct for its intended case and
catastrophic here: `sash_table_len(NULL)`.

This is the same lesson as journal 449, in a new position: never create an SSA
version defined on only one path. `emit_attest_retag` already follows the rule
by aliasing its target version to the source slot. The four fills now do too.

## The asymmetry that was live but unreachable

One failure remained after that, in **both** backends — so not codegen.
`via.12`: `array a; argv a; n=$(len a); echo "n=$n"`.

DD#60 says a length/count operator measures a variable rather than reading it,
so it is clean even on a tainted one. `check_param_expansion_use` implements
that. `value_is_tainted` did not. And `cmd_subst_taint`, in the same file,
separately classifies `len` and `strlen` Clean — so the answer depended on
which internal node the transform happened to pick for `$(len a)`. `echo $(len
a)` passed and `n=$(len a)` did not.

That inconsistency was already in the tree. It was simply unreachable while no
collection could ever be tainted; (ii) made it live in the same commit that
made it visible. Both sides now share `is_measurement_op`.

It is worth being clear that this was not a judgement call. The rule was
already decided, already documented, and already implemented — twice — in the
file that got it wrong the third time. Applying it was the only consistent
move available.

## Numbers

Sweep 7071 (3638 native, 3420 c, 13 skipped), zero failing, exactly the
predicted total: 7059 plus six new dual-mode specs. `mk test` clean.

Nine new specs: `bfb.17`-`bfb.22` pin the net-effect rule, `sw.14`-`sw.19` pin
the four fills, the attest-clears control, and the DD#60 measurement control.

Gap 1 of `ssa-taint-full-backstop.md` — capture-form builtins, whose taint is a
boolean fixed at transform time before analysis runs — is the only part left,
now split out to its own task.
