# 265 — three P1 bugs: a loop-lowering segfault, `while read`, and the splat join

*2026-09-11*

DG asked what was still broken, then said "fix P1s now". Three P1 bugs were
open; all three are closed here. The first was not on the list — it was found
while testing the counter-argument to the second.

## The question that found the segfault

DG's ruling on `while read ARR` was already recorded (reject, 2026-09-10), but
DG asked for the counter-argument before I implemented it. The obvious one:
bash's `while read -a arr` is a real per-line field-splitting idiom, and
rejecting sash's spelling of it would remove something useful.

So I went looking for the sash spelling. The candidate was

```sh
while readline line {
  attest line [[ $line =~ ^.*$ ]]
  read $line ARR
  ...
}
```

which segfaulted. The counter-argument evaporated for a different reason —
`read $s ARR` splits on **NUL**, not whitespace, so `read $s ARR` on `"a b c"`
yields **one** element, and there is no per-line field split anywhere in the
language for `while read ARR` to be the spelling of. `readline v` is the whole
replacement.

But the segfault was real, and bigger than the construct that exposed it.

## (1) An auto-declared bulk-read target inside a `while` body

```sh
int n=0
while (( n < 1 )) {
  (( n = n + 1 ))
  read ARR              # segfault
  echo "n=$(len ARR)"
}
```

Narrowed across four axes: not `readline`-specific (a plain `while (( ))`
counter does it), not taint-related (a clean literal does it), not
operand-related, and **`for` loops are unaffected**. Top level is fine. An
explicit `array ARR=()` in the same position is fine.

`--emit-c` named the defect outright:

```c
const char* ARR_ptr = ""; i64 ARR_len = 0;
...
sash_split_into_array(__t5.ptr, __t5.len, 0, 1, (unsigned char*)ARR_ptr);
```

No table was ever allocated. The fill wrote into the `""` initializer — a
read-only string literal.

**Cause.** `lower_while` (`transform/control_flow.rs`) is a two-pass lowering:
pass 1 lowers the body to discover loop-carried variables, then **clears the
block's instructions** and re-lowers with phi-merged versions. It rolled back
the instructions but not `state`. `autodeclare_bulk_target` is guard-then-emit:

```rust
if state.collections.contains(name) { return; }
state.collections.insert(name.to_string());
```

Pass 1 emitted the empty-collection `Assign` and registered the name; the
`insts.clear()` discarded the `Assign`; pass 2 hit the guard and emitted
nothing. Every other backend inherits it — this is in the transform, above the
backend split.

**Fix.** Snapshot `collections`/`assoc_arrays` before pass 1 and restore them
before pass 2, alongside the existing `insts.clear()`. This is the same
save/restore the codebase already does in
`enter_fn_body_scope`/`exit_fn_body_scope`, which saves exactly this pair (plus
`readonly_vars`).

`readonly_vars` is deliberately **not** restored. It leaks the same way, but
restoring it would change behaviour: `const r="x"` inside a `while` body is
rejected today, and `for i in 1 2` rejects it too (via unrolling, which is a
legitimate second binding). Making `while` alone accept it would create a
divergence I have no ruling for. Filed separately.

Version counters are also not rolled back — `counter` is documented as a
high-water mark that never rolls back, and `insert_phis_two_branch` consumes
pass 1's versions.

**Blast radius.** Measured by stashing the fix and re-running: `read ARR` and
`read $s ARR` — DD#99's two read forms — and nothing else. `argv V` in the
same position was already fine, so `autodeclare_bulk_target`'s other callers
reach their table by another route.

Specs `commands.sh` 14.37–14.40, including a control (explicit declaration in
the same position) that passes both before and after, which is what localises
the defect to the autodeclare path.

## (2) `while read ARR` → SB5069

Now implementable. `read` consumes the whole stream, so the loop has no second
read to make; it spun forever on an empty array, compiling clean.

The check sits in `validate_while`, which already separates condition from
body. It walks each condition statement for a `read` command, so it catches
the pipeline form (`tr … | read ARR`) as well as the bare one, and it fires
for `until` too — the defect is looping on a slurp, not the sense of the test.

Two things deliberately still compile:

- `if read ARR` — testing whether a read produced anything is meaningful.
  Only *iterating* on it is the category error.
- `read ARR` in a loop **body** — which is exactly what (1) just fixed.

A spec I wrote for the replacement form failed, and the compiler was right:
`while readline v < f` re-opens `f` every iteration and never advances — in
bash too. The redirect belongs on the producing side. Corrected in 14.46 with
a note, since it is an easy trap to re-lay.

Specs `commands.sh` 14.41–14.46.

## (3) A splat in a scalar operand → SB5070

Part (b) of `bug-sed-op-on-array-empties-it`. Part (a) (`a =~ s/…/…/` on an
array) was already closed by SB5064 last session.

```sh
array a=(x:1 y:2 z:3)
t=$(gsub @a ":" "-")     # [x-1 y-2 z-3] — one joined string
```

Arity checks out because a splat is one *syntactic* argument, so
`check_cmdsub_arity` counts three and passes; the splat then collapses to a
joined string at runtime. The author asked for something the language does not
define and got a silent reinterpretation — `$(gsub "$(join a)" …)`.

**The filing named `gsub`. Measurement found seven.** `sub`, `gsub`,
`toupper`, `tolower`, `basename`, `dirname` and `match` all silently join.
Fixing the two that were filed would have left five identical silent wrong
answers behind a green suite, so SB5070 covers all seven — the scalar-operand
intrinsics, in every argument position, bare splat or with a disposition.

`len`, `join` and `keys` are excluded, and the exclusion is principled rather
than expedient: they take a collection **by name**, so `@a` there is not a
reinterpretation at all (`len @a` correctly returns the element count, `keys
@a` is already SB5025). `len @a` and `join @a` are DG's own open DD#75
questions and are untouched.

The message names `$(join a SEP)` — DD#75's explicit joined-string form — and
spec 96.20 runs that advice to confirm it actually compiles and produces the
value the author was reaching for.

Specs `var_modifiers.sh` 96.12–96.22.

## What the three have in common

All three compiled clean and did the wrong thing — the shape the bug policy
exists for. Two produced a wrong *answer*; one produced a wrong *pointer*. In
all three the fix was a diagnostic or an allocation, not a feature.

The method that worked twice: **test the workaround before accepting the
filing's framing.** For the regex task last session that dissolved the bug
entirely. Here it found a segfault the filing knew nothing about, and it
turned a two-intrinsic fix into a seven-intrinsic one.

## A fourth, found by the verification itself

The name-for-name Rust-suite diff — capture failures with the fixes, `git
stash`, capture again, `diff` — came back with **one** new entry:

```
> janet_tests:janet_const
```

Which would mean a regression. It was not one, and the way it was not one is
worth writing down.

`janet_const` passed in isolation. It failed every time the suite ran. It
passed under `--test-threads=1`. And the *stashed baseline* failed it two runs
in three — so the single baseline capture had simply been a lucky draw, and
comparing two single runs of a flaky test manufactures a regression that isn't
there.

The cause:

```rust
fn run_janet(name: &str, janet_src: &str) -> String {
    let path = format!("/tmp/sash_janet_run_{}.janet", name);
```

`janet_const` and `janet_int_keyword` both passed `"standalone"`. `cargo test`
runs tests in parallel threads, so the two raced on one file and whichever lost
executed the other's program. `janet_const` expects `314`, `janet_int_keyword`
expects `42`, and either can win.

Exactly one collision existed in the file. The fix is to give `int_kw` its own
name — but a rename alone leaves the next author free to re-lay the trap
silently, and this is a known hazard class here: AGENTS.md already warns that
`chk` ids are a flat global namespace where a duplicate is overwritten
"silently if both pass".

So the temp-path helpers now `claim()` each path through a `Mutex<HashSet>` and
panic on reuse. A name collision is a named failure pointing at the mistake,
instead of a wrong answer in a different test some fraction of runs later. Same
principle the DD#96 write-up states: *a path that silently accepted junk
because nothing pinned the rejection.*

After the fix, `janet_tests` reports 15 failures across five consecutive
parallel runs and under `--test-threads=1`, and the five-suite diff against the
stashed baseline is **empty**: 140 pre-existing failures, identical names,
before and after. (`codegen_tests` still hangs — it bypasses `validate`, so
DD#99-retired surfaces compile into non-terminating binaries. That is the open
`rust-tests-hang-on-retired-surfaces` decision, untouched here.)

The lesson is about method, not about Janet: **a count-only comparison would
have shown 141 vs 140 and sent me hunting a regression in the transform.** The
name-for-name diff is what turned "one more failure" into "this specific test",
which was the only thing that made the flake findable.
