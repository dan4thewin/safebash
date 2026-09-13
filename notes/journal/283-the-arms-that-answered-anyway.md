---
title: "283 — The arms that answered anyway"
date: 2026-09-13
---

# The arms that answered anyway

DG asked a scoping question: *what native and C work remains before the
test redux?* It was meant to be a survey. Answering it honestly meant
re-measuring every open filing against the live binary rather than
trusting what the task files said, and that re-measurement turned up
three bugs, all of the same shape.

The shape is worth naming, because it is the most dangerous kind of
defect this project can produce. A match arm in `emit_c.rs` emits a
`/* TODO */` comment and returns `("\"\"", "0")`. The C compiles
cleanly. The binary runs. It prints an answer. The answer is a value the
compiler **never computed**. There is no crash to trip over, no
diagnostic to grep for, no exit code to check — just a confident wrong
result. A P1 closed in `bf96d25` had swept the command-dispatch arms;
six survived it.

## `[[ a < b ]]` was false. Always.

The first probe was meant to be perfunctory. In the C backend,
`[[ a < b ]]` evaluates to constant false. So does `[[ b > a ]]`. Bash
says true; native says true; C says false and says it silently.

Then it got worse in the useful direction: the gap was five operators,
not two. Native's `lower_test_binary` implements `<`, `>`, `-nt`, `-ot`,
`-ef`. C implemented `=`, `==`, `!=`, `=~` and dropped the rest into the
fail-open arm. Every one of the five had been answering false since the
C backend existed.

The fix was small — `sash_strcmp` was already externed, `sash_file_newer`
and `sash_file_same_dev` needed externing — and the important decision
was to route through the **same runtime comparators native uses** rather
than write a C-side compare. Two backends sharing one comparator cannot
drift. Two backends with two comparators eventually do.

What makes this bug instructive is not the fix but the survival time.
`[[ a < b ]]` had no spec test in any backend, ever. The project has 4270
passing tests and the operator was never once exercised. **The absence of
a spec test is not evidence of correctness — it is the absence of
evidence.** The construct nobody pinned is exactly the construct nobody
noticed was broken.

### Predicting the failure count

Fail-first verification says: revert the fix, confirm the new tests fail.
Eight new tests went in; reverting failed **six**. That is the correct
number, and knowing it in advance is the whole point of the exercise.
`ft.38` (`abd < abc` → no) and `ft.44` (`b_old -ef b_new` → no) are the
negative cases, and a constant-`false` arm answers a negative case
correctly by accident. A test suite that only asserts the positive
direction of an operator cannot tell "implemented" from "hardwired to
no".

Later, writing the DD addendum, I went to record `-nt` as
whole-seconds — a half-remembered detail from the `-N` bug — and stopped
to measure it instead. Two files 1 ms apart inside the same second:
bash orders them, and so do both of our backends, because the runtime
compares timespec. The claim I was about to write into a decision
document was false. `ft.45`/`ft.46` now pin the sub-second behaviour, and
the DD says nanoseconds because that is what the three-way comparison
showed.

## The rule that only applies in one position

Closing the remaining arms by guessing at reachability would have
repeated the mistake that left them open. The method that works is
enumeration: `SsaValue` has 28 variants, the C dispatcher handled 21, and
the residue of 8 is the suspect list. Three are intercepted upstream and
never arrive. The other five are precisely native's ICE set. So five ICE
arms plus an ICE terminal, and the "unsupported" state stops existing —
per DG's principle from earlier in the session, *unsupported is not a
legitimate state*.

One of the eight was not a codegen gap at all. The non-literal
command-substitution head: `$c hi` in statement position is SB5011, but
`$($c hi)` was **accepted** — aborting at runtime in native and silently
capturing the empty string in C. `check_cmd_in_stmt` only visited
`StmtKind::Command`, and `child_bodies()` returns empty for
Assignment/Declaration/Command, so cmdsub bodies — which live inside
*expressions*, not statements — were never walked at all.

The first fix was wrong in an interesting way. Running the existing
`check_commands` over cmdsub bodies rejected `$(getenv HOME)` and
`$(argc)` with SB5073, "value intrinsic in statement position." That
diagnostic is exactly right in statement position and exactly wrong
inside `$( )` — a value intrinsic in a command substitution is the
*only* place it belongs. **Only the head-must-be-literal rule
transfers.** The lesson generalises: when extending a check into a new
syntactic position, the unit that transfers is the individual rule, not
the pass that contains it. Spec `23.24` pins the distinction so a future
tidy-up cannot re-merge them.

## The half of `<>` that testing found

DG chose implement over reject for `<&` and `<>`, both of which were
accepted-and-ignored in **both** backends. `<&N` turned out to be the
same operation as `>&N` — `dup2(N, fd)` — differing only in the fd they
default to, which `default_fd_for_op` already decides. One emitter, two
ops. `<>` needed a runtime flag code: `O_RDWR|O_CREAT`, create if absent,
do not truncate.

`<&` matched bash immediately. `<>` did not, and the reason is the part
worth writing down: **`<>` defaults to fd 0, not fd 1**, because it is a
`<` form. With fd 1 the behaviour was plausible rather than obviously
broken — `echo xy <> f` wrote into the file, and `cat f` showed `xy`
followed by the tail of the old content, which reads like a partial
overwrite rather than a bug. Bash prints `xy` to stdout and leaves the
file alone. Nothing short of a three-way comparison against bash catches
that; reasoning about it from the code would have confirmed the wrong
answer.

There is a companion lesson about reasoning-from-code. When I first
filed the `<&` task I wrote that native "maps `DupInput` onto `Read`
flags and opens a file named `9`" — a misreading of
`default_fd_for_op` (which fd to default *to*) as `open_flags_for_op`.
Native was simply no-opping, same as C. The filing was corrected on the
way to fixing it, but it sat in the repo as a confident false claim for
an afternoon.

## Shape of the work

Six commits' worth of changes, but a single thesis: the compiler must
never assert a value it did not compute. Where the construct is real,
implement it; where it cannot occur, ICE. Nothing in between. Across
4270 tests nothing hit an ICE, which is corroboration that the
enumeration was right — not proof, and the distinction matters.

A parity item rode along: `emit_c_argv_value` hard-coded
`sash_atoi({iv}_ptr, {iv}_len)`, naming slots an int var does not have.
That one is genuinely not a bug today — `__fi` is a string var, and the
generated C is byte-identical before and after. It is a trap armed for
whoever makes `__fi` an int counter. Fixed by routing through
`lower_var_as_int_c`, which already knows the rule.

The complexity gate rejected the commit three times and was right every
time. Two findings for the pattern library: a nested `fn` inside a
function body accumulates its inner decisions into the parent exactly as
a closure does (hoisting one to top level took a function from cognitive
8 to ~1), and `for { if { if { } } }` at 8 becomes three flat functions
at ≤3 each with no logic change. The gate is not measuring how much the
code does; it is measuring how deeply it nests while doing it.
