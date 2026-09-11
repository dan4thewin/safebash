# 263 — One type mistake, seven crashes

`bug-scalar-subscript-segfault` was filed with one reproducer:

    str s=hello
    echo "${s[0]}"

Segfault. Before fixing it I probed the neighbourhood, because a filed bug
is a sighting, not a census. Seven spellings, seven crashes:

| spelling                           | outcome                        |
|------------------------------------|--------------------------------|
| `echo "${s[0]}"`                    | segfault (rc 139)              |
| `echo "${s[$k]}"`                   | segfault                       |
| `int n=5; echo "${n[0]}"`           | **compiler ICE**, codegen/types.rs:29 |
| `for x in @a; echo "${x[0]}"`       | segfault (loop variable)       |
| `s[0]=x`                            | segfault (write form)          |
| `echo $(( s[0] + 1 ))`              | segfault (arith form)          |
| `[[ ${s[0]} == h ]]`                | segfault (test form)           |

One defect, seven faces. `check_subscript_key_kind` in `type_check.rs` opens
with

    if !env.collections.contains(name) || env.is_slot(name) { return; }

— it validates the *key* against the subject's kind, and a subject that is
not a collection has no kind to validate against, so it returns. Nothing
downstream asks the question again. The subscript lowers to a table read, so
codegen emits a load of a `SashTable*` from a slot holding a `char*`. For an
`int` subject the slot holds a raw i64 and the type lookup itself panics —
the same mistake, caught one layer earlier, which is why it presented as an
ICE rather than a segfault.

## Where the check belongs

The obvious home was `type_check.rs`, next to the early return. That is the
wrong home, and the loop-variable face is what proves it. `TypeEnv.vars` is
populated from Assignments and Declarations only (`infer_stmt`,
`infer_declaration_types`); a `for` variable is never declared into it. A
check written there would have fixed six faces and left the seventh
segfaulting — and I would have had a passing test suite saying I was done.

`validate.rs` already has `collect_scope_names`, which enumerates every name
a scope introduces: declarations, assignments, `for` variables, read and
mapfile targets, DD#93 attest-convert targets. It also already hosts SB5064,
the exact dual of this bug (`$a` on a collection — scalar sigil, collection
variable; here, container sigil, scalar variable). The two diagnostics now
sit adjacent, and SB5064's doc comment — which named this bug as its open
dual — can finally point at a fix.

## The allow-set is not the declaration set

The tempting predicate is "subject is not in `collect_collection_names`" —
the `-a`/`-A` declarations. That is wrong, and would have rejected working
programs. Two constructs introduce a collection with no declaration at all:

    argv g            # then ${g[0]} is valid
    read ARR < file   # then ${ARR[0]} is valid

plus `args` inside a function and the DD#100 slots `A`..`E`, which are bound
by the caller. `bulk_fill_targets` — written last session for the slot taint
analysis — already returns exactly the read/readline/mapfile/matchall/argv
target names, so the allow-set is declarations ∪ bulk-fill targets ∪ slots ∪
`args`.

`readline v` fills a *scalar* but is collected too. That is deliberate: the
error direction matters here. Over-approximating the allow-set costs a missed
diagnostic; under-approximating it rejects a program that works. A false
negative leaves a latent segfault, which is bad; a false positive breaks
someone's working script, which is worse, because they cannot route around a
compiler that refuses correct code.

An undefined name is left to SB5031. `${nope[0]}` is not a type mistake, it
is a missing variable, and it keeps its own diagnostic.

## Three walk sites, not one

The seven faces are not all reachable from one traversal. Expression
subscripts (including inside `[[ ]]` and double quotes) come from
`direct_exprs` + `walk_expr_tree`. The write form is a `StmtKind::Assignment`
with `index: Some(_)` — the subject is the statement's `name` field, which
appears in no expression. Arithmetic subscripts are `ArithExpr::Subscript`,
a separate node that `walk_arith_var_reads` deliberately skips (it descends
into the *index*, documented as "the array name is handled by the bare
subscript cell check"). That check lived in `type_check.rs` as a private
`for_each_arith_subscript`; it moved to `ir.rs` as `walk_arith_subscripts`
and both passes now share it — the same consolidation that `walk_stmt_tree`
got last session, for the same reason.

## A gap found by the control

Writing the "legitimate subjects still work" control surfaced a second,
unrelated bug: under `--janet`, `${args[0]}` inside a function emits
`(get args 0)` against a `defn` that binds no such symbol, and janet refuses
to compile it. Confirmed pre-existing by stashing and re-running at `fe71aa1`
— this change is validate-only and cannot affect emission. Filed as
`janet-args-subscript-unbound`; it is the same family as
`janet-collection-parameters` (the DD#100 slots are emitted unresolved the
same way), and the two want fixing together: both are "a collection the
callee receives rather than declares".

The control is worth more than the negative tests here. Ten specs assert the
rejection; two assert that arrays, maps, `args`, `argv` targets and slots
still subscript. It was the second pair that found something.

## A flake, and the bug under it

The sweep after the complexity refactor turned up one non-janet failure:
`native:14.33`, a pipeline-redirect test untouched by any of this. It
reproduced about 1 run in 40, and `runs` showed it had already flaked once at
`fe71aa1` — which is why the regression query stayed silent (the baseline
snapshot happened to catch the failing run). The database being its own
history earned its keep here.

The fixture is

    /usr/bin/printf "m\n" > rd2.txt; echo ignored | /bin/cat < rd2.txt | /bin/cat

Stage 2 redirects stdin from a file, so it never drains the pipe and `echo`
races SIGPIPE against stage 2's exit. Before changing anything I checked
`notes/decisions/`, and DD#55 addresses it head-on: "SIGPIPE is the
programmer's responsibility — use `- cmd` on the producing side or choose
tools that don't trigger it (`sed -n 1p` over `head -1`)." The abort is
deliberate. The **test** was wrong — it asserted rc=0 from a pipeline the
model says may abort. Suppressing the producer with `-` fixed it: 0 failures
in 300 runs, against 5 in 200 before.

Chasing it, though, turned up a real defect one layer down. `cat f | head -1`
aborts by design, but it aborted with **exit 128**, and so did SIGTERM, and so
did every other signal:

    /usr/bin/timeout --preserve-status -s TERM 0.1 /bin/sleep 5
    # sash: 128      bash: 143

`sash_wait` in `sash-rt` returned a bare `128` whenever the child had not
exited normally, discarding the signal number. DD#55 defines the pipeline
aggregate as the worst exit status under pipefail, and bash's pipefail reports
`128 + signum`; `emit_signal_exit_check` in codegen *already* applies that
convention to the program's own signal deaths. The runtime was the lone
dissenter. Three lines — `status.terminating_signal()` — and SIGTERM is 143,
SIGPIPE is 141, matching bash exactly.

This needed no decision: the convention was already established in the
codebase and the DD already specified the semantics. Step 1 of the tree —
settled by a source of truth — so: fix, and report.

Writing its specs cost two more corrections, both from reading the harness
rather than guessing. Runtime stderr lands in `$rerr`, not `$err` (`err` is
compile diagnostics only). And `- cmd` cannot be used to *observe* a status,
because DD#70 makes `-` discard it and `$(status)` after one is SB5012 — so
the specs pin the program's own exit code, which is the observable that
actually diverged. The diagnostic text is asserted in native only, per the
existing convention at 55.1/55.8/55.10: the c backend exits with the right
status but emits no abort diagnostic.

## Result

`spec/types.sh` 75.44–75.54, `spec/errors.sh` 55.57–55.59, `spec/commands.sh`
14.33 de-flaked. Sweep: 4818 passing, 39 failing (all `janet:`,
the pre-existing set), 482 skipped — zero non-janet failures, zero
regressions against `fe71aa1`. The Rust integration suites show the same 141
failures before and after, name for name (`rust-tests-hang-on-retired-
surfaces`).
