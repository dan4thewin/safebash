title: "bug: a function's name escapes binding and read checks, including arithmetic len"
status: open
priority: P1
kind: bug
depends: []
gates:
  - "c2 and c6: declaring a function's name (`int f=3`, `str f=3`) is SB4019, as `f=3` is (c1)"
  - "c3 and a2: reading a function's name as a variable (`$f`, or a bare `f` in `$(( ))` when f is not an int function) is rejected, as a read of a name assigned nowhere is (SB5031, c4)"
  - "a bare `f` in `$(( ))` still calls an int function f, and `(g)` still passes the read check"
  - "B80: `(len f)` where f is only a function gets a located source diagnostic in both modes, never a native ICE or a C garbage value; ordinary `$(len f)` remains rejected"
  - "specs in both modes; the SB census stays clean in both directions"

Found 2026-10-30 in F6 unit A (journal 547), while checking a probe. The
binary before F6 gives the same results, so it is pre-existing. F6 did not
touch the functions involved, so by the 2026-10-26 rule it comes here.

```sh
# c1: SB4019 "variable 'f' collides with function 'f' — rename one"
fn f { echo hi }
f=3

# c2: compiles and prints v=3 (c6: the same with `str f=3`)
fn f { echo hi }
int f=3
echo "v=$f"

# c3: compiles and prints v= (nothing assigns a variable f)
fn f { echo hi }
echo "v=$f"

# a2: compiles and prints n=0; f's body never runs
fn f { echo hi }
int n=$(( f ))
echo "n=$n"

# c4: SB5031 "'$zz' is assigned nowhere (possible typo)"
int n=$(( zz ))
```

In a2's place, `(f)` is SB4030 on both binaries: "`f` can't be called in
an arithmetic sexpr — it yields a string, not an int".

## Cause

- SB4019's check, `check_var_func_collision`
  (`transform/stmt_handlers.rs`), has one caller, `handle_assignment`.
  Declarations never reach it.
- validate's read check, `ReadSink` (`validate.rs`), merges function names
  into its `known` set so that a nullary sexpr `(g)` is not flagged as an
  undeclared read. So no read of a function's name is ever SB5031, whether
  it is `$f` or a bare `f` in arithmetic.
- `var_to_ssa` (`transform/expr_handlers.rs`) treats a bare arithmetic name
  as a call only when it is an int function. Any other name is read as a
  variable, here one that nothing assigns.

## Open choice for the fix

Which diagnostic a read gets: SB4019 (the name belongs to a function),
SB5031 (no variable of that name is assigned), or, for a bare `f` in
arithmetic, the SB4030 that `(f)` gets. Any of them states the rule. The fix
picks one per position and records it in a DD#58 addendum.
`bug-int-call-by-name-redefinition`'s i3 used to depend on this choice.
DD86 now rejects its duplicate definition (DG, 2026-11-06; journal 582),
so its gate belongs to `reject-duplicate-function-definitions`.
The single-definition cases in this task remain independent and unfixed.

## Port closeout recheck (2026-11-05)

This is the backlog home for batch-531 **B77, B78 and B80**. All remain
live on `09a5bd49` in current-tree native/C probes (journal 570).
Typed `int f=3` and `str f=3` print 3; the plain assignment gets SB4019.
`$f` is empty and bare arithmetic `f` is zero without calling the void
function. The legitimate nullary int-function control still returns 7.

B80 is the collection-name consequence:

```sh
fn f { echo hi }
int n=$(( (len f) ))
echo "$n"
```

Native compilation exits 1 with `ICE: undefined arrlen: f`
(`codegen/types.rs:322` at this commit). C compiles and prints `4294940122`
in this observation; that arbitrary value is not a stable result to assert.
Replacing the initializer with `$(len f)` gets the proper introduced-name
refusal, SB5039, in both modes. The arithmetic spelling reaches lowering
without that refusal. Priority is P1 because this includes an ICE and a
silent C miscompile, not just diagnostic consistency.

Keep callable names distinct from variable/collection bindings across these
positions. Do not turn B80 into a successful empty collection or zero fallback.
The diagnostic choice above remains for the fix; closeout changes no behavior.

Evidence: `files/port-closeout-probes.tar.gz`, `observations.*.log`,
the complete mode xtraces and `b80-native.stderr`. The harness's diagnostic
filter omitted this native ICE text, so the separate raw compiler stderr
and exit status are retained.
