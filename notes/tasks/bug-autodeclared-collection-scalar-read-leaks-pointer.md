title: "MEMORY DISCLOSURE: an auto-declared collection read with `$name` leaks a heap pointer to argv"
status: done
priority: P0
kind: bug
depends: []
gates:
  - "`argv x` / `read x` followed by `/bin/echo $x` does not reach execve with a `SashTable*`"
  - "`str x=safe; read x` is a diagnostic (SB5049-class), not a silent retype"
  - "`int x=1; read x` is a diagnostic, not `ICE: expected string var, got int`"
  - "SB5064's collection set includes auto-declared targets (`read`/`argv`/3-arg `match`/`matchall`)"

# An auto-declared collection read with the scalar sigil leaks a heap pointer

Found 2026-09-28 while answering DG's question about how a `$( )` body is
validated. It has nothing to do with command substitution — that was only the
path that removed the masking.

## Repro — three lines, no redeclaration

```sh
read x
attest e in x [[ -n $e ]]
/bin/echo $x
```

```
$ echo LEAKED | ./prog | od -An -tx1
 40 20 cc e6 3c 7f 0a
```

`0x7f3ce6cc2040` — a live mmap address, handed to an external process. Fresh
value every run (ASLR). `argv x` is the same and needs no stdin:

```sh
argv x
attest e in x [[ -n $e ]]
/bin/echo $x
```

**Both backends produce it identically.** This is not a codegen divergence; it
is the shared model. `--emit-c` shows the mechanism in four lines:

```c
SashTable* __t0_tbl = sash_table_new(8);
x_ptr = (const char*)__t0_tbl;      /* the table goes in the SCALAR slot */
x_len = 0;
...
sash_argv_guard(x_ptr, x_len);                       /* guard sees len 0 */
const char* __t3_argv[] = { "/bin/echo", x_ptr, 0 }; /* execve reads to NUL */
```

`sash_argv_guard` is handed `x_len` = 0 and so validates nothing, while
`execve` reads `x_ptr` until the first NUL — straight through the table
header, whose first field is a pointer.

## Two independent faults

### 1. SB5064's collection set is blind to auto-declared collections

`collect_collection_names` (validate.rs:6808) walks only
`StmtKind::Declaration` with `-a`/`-A` flags (`insert_collection_decl`:6817,
`is_collection_flags`:6829). The auto-declaring builtins never produce a
`Declaration`, so they never enter the set.

`type_check.rs` *does* register them — `register_argv_target` (1914) and
`register_newread_target` (1920) insert into `env.collections`, per the DD#60 /
DD#61 comment at 1888. So the program has **two collection sets that disagree**,
and the one SB5064 consults is the incomplete one.

The contrast is exact:

| program | result |
|---|---|
| `array x=(a); /bin/echo $x` | **SB5064**, correctly |
| `argv x; …; /bin/echo $x`   | **compiles, leaks** |
| `read x; …; /bin/echo $x`   | **compiles, leaks** |

SB5064's own doc comment (validate.rs:6547) says `$a` on a collection "was
neither rejected nor meaningful… in every value context it silently expanded
to nothing". That is still true for every auto-declared collection — except
that in the external-command sink it is now worse than nothing.

### 2. Auto-declare does not check for a conflicting existing declaration

`register_argv_target` / `register_newread_target` insert unconditionally. An
explicit redeclaration is properly caught — `str x=safe; array x=(a)` is
**SB5049** "`x` is already declared in this scope" — but the builtins bypass
that rule entirely:

| prior declaration | `read x` / `argv x` then a scalar read |
|---|---|
| none      | **leaks** (fault 1) |
| `array`   | correct |
| `map`     | SB4030 — but only incidentally, from attest-all's binder arity (type_check.rs:2007), not from any check on `read` |
| `str`     | **leaks** — silent retype, type env still says `str` |
| `int`     | **`ICE: expected string var, got int`** (codegen/types.rs:34) |

## Sinks reached

| sink | result |
|---|---|
| `/bin/echo $x` (external argv) | **leaks the pointer bytes** |
| `str y=$x; /bin/echo $y`       | **leaks** — propagates into a scalar, then anywhere |
| builtin `echo $x`              | prints empty (the historical "expands to nothing") |
| `[[ -n $x ]]`                  | false |
| inside a function body         | **leaks** |

## Why it was not caught

Every accidental masker fires before the scalar read:

- SB5020 (taint) blocks `argv x; /bin/echo $x` outright — the leak needs an
  `attest` first, which is why no spec hit it.
- SB5050 (dead variable) blocks the shortest forms.
- SB4018 blocks the `matchall` spelling (`matchall` result unused).

This is the SB5050/SB5020 masking pattern again: the leak is reachable only
after you satisfy the guards that were hiding it.

## Fix — needs DG

Fault 1 is the security fix and looks narrow: give
`collect_collection_names` the auto-declared targets too, so SB5064 covers
them. The question is **where the single source of truth should live** —
`type_check.rs` already computes exactly this set (`env.collections`) and
`validate.rs` recomputes a worse one. Two sets that must agree is what caused
this.

Fault 2 is a language question: should `str x=…; read x` be SB5049 (consistent
with `array x=(a)`), or is silently rebinding a name to a collection intended
and only the *type* needs updating? The `int` case must stop ICEing either way.

Note also that the `map` case is caught only by accident, so "we already reject
the conflicting cases" is not true today.

## Related

`bug-taint-escape-in-cmdsub-body` (P0/P1, same session): taint is not checked
inside a `$( )` body, which is what removed the SB5020 masker and exposed this.
They are separate bugs with a shared theme — a pass that computes its own
partial view of the program instead of using the one that already exists.

## Resolved 2026-09-28

DG settled both faults in one message: *"no shadowing and no rebinding. `$x`
should be an error after `argv x` (SB5064?)."* Both gates met.

**Fault 1 — SB5064's set.** `validate.rs` gained
`collect_auto_collection_targets` (the last argument of `read`, of a
3-argument `match`/`matchall`, and the literal target of `argv`) and
`collect_scalar_bindings`. `check_collection_scalar_read` now consults
`explicit ∪ (auto − scalars)`.

The question the filing raised — where the single source of truth should live —
was answered by keeping the sets separate but making the *disagreement* the
thing that is checked. `validate` runs **before** `type_check`, and a validate
diagnostic aborts, so simply importing `env.collections` was not available:
the set has to be computable from the AST alone. The `− scalars` term exists
for the same ordering reason. Without it, `x=safe; /bin/echo $x; read x`
reported SB5064 against line 2 — a *correct* line — and the user never saw the
real fault on line 3. Subtracting scalar bindings makes validate decline the
case, and type_check owns the better message. An explicit `array`/`map`
declaration deliberately still wins, so `array a=(1); a=x; echo $a` stays
SB5064.

**Fault 2 — the retype.** `register_argv_target` and
`register_newread_target` merged into one `register_collection_target` that
refuses a name already in `env.vars` but not in `env.collections`:

```
SB4030: cannot fill the collection `x` with `read` — `x` is already a string
variable, and a variable does not change type (DD#86); fill a new name, or
declare `array x` instead
```

The `int` ICE is gone by the same route — it now reports SB4030 at the right
line. The `map` case, which the filing noted was caught only by accident, is
now caught on purpose.

**Specs.** `types.sh` 75.101–75.107 (the four auto-declaring binders, the
assignment sink, and two controls: `readline` still binds a scalar, and the
correct `${x[1]}` spellings still run). `scope.sh` 86.59–86.65 (the rebinding
rule across all three non-declaring binders, plus the former ICE, plus two
controls). Sweep 6708 passing, zero failing.

Not done here: the filing's "Related" note still stands —
`bug-taint-escape-in-cmdsub-body` is open.
