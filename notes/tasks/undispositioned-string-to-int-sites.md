title: "Undispositioned string→int sites outside assignment and arithmetic"
status: done
priority: P2
depends: []
gates: none — DG decided 2026-09-26

# Undispositioned string→int sites outside assignment and arithmetic

SB4030's own message states the rule:

> a string→int conversion goes through arithmetic and must carry a
> disposition: `$(( ${VAR:-0} ))` … or `$(( ${VAR:?msg} ))`

DD#77 enforces it inside `(( ))`/`$(( ))`, and `check_assignment` enforces it
for `int x = $s`. Everywhere else a string silently becomes an int through the
**lenient** `sash_atoi` (DD#72: non-numeric → 0).

**All seven sites are now closed.** See "Closing" at the end.

**Four of the original seven sites were closed first.** `return` (journal 411, which
was also a native/c divergence), and the two subscript surfaces plus their
missing spelling (journals 413/414). The rest await DG's scope decision.

## The remaining sites

Measured 2026-09-26 against `761b3f4`, with `str s=abc`:

| site | sash | bash | accountable spelling exists? |
|---|---|---|---|
| `exit $s` | rc 0 | rc 2 + error | yes — `exit ${s:?m}` works |
| `wait $s` | rc 127, silent | rc 1 + error | yes — `wait ${s:-0}` works |
| `[[ -t $s ]]` | false, fd 0 | false | parses but is ignored |
| `$(arg $s)` | masked by SB5020 | n/a | untested rather than settled |

`$(arg $s)` is listed for completeness: taint fires first on every shape tried,
so whether it would coerce is unknown. Do not record it as closed.

Contrast — already rejected: `int x=$s`, `int x=abc`, `exit abc`, `(( n = s ))`,
`x=${a[0]}` (string element), `${t:1:$s}` (SB4030), `${t:$s:2}` (SB4047),
`${a[$s]}` / `a[$s]=` (SB4030), `${a[s]}` / `${a[i]}` (SB4030), `${a[i+1]}`
(SB4041).

Note `exit` is inconsistent **with itself**: `exit abc` is SB4030 as a string
literal, while `exit $s` is accepted. Stricter about a value written in the
source than about the same value in a variable.

## CLOSED — the subscript surfaces (journals 412, 413, 414)

Both `${a[$s]}` and `a[$s]=` are SB4030, and the rule is no longer the narrow
"declared `str`" test journal 412 shipped. It is now positive: **an array
index must be int-typed**, so an undeclared `i=1` is rejected too. See DD#80's
2026-09-26 amendment.

**The LHS-spelling blocker that this task called "the only thing that would
make the two surfaces symmetric" is gone** (`0e23874`). The LHS now calls
`expr::parse_subscript_index` — the same function the read path calls — so
the *parse* barrier that made the accountable spellings unwritable is gone:

| spelling | before | now |
|---|---|---|
| `a[$(( ${s:?m} ))]=Q` | SB5011 | **works**, and aborts on a non-integer with the author's message (rc 1) |
| `a[$(( ${s:-2} ))]=Q` | SB5011 | **works**, defaulting to 2 |
| `a["$i"]=Q` (map) | SB4014 | **works** — quoting is the int→string conversion |
| `a[${s:?m}]=Q` | SB4014 | parses, then **SB4030** |
| `a[${s:-2}]=Q` | SB4014 | parses, then **SB4030** |

The last two rows matter: they now reach the type check instead of dying in
the parser, and the type check correctly rejects them, because a bare
disposition keeps its *subject's* type and `s` is a string. **A bare
disposition is not the conversion** — `$(( … ))` is, exactly as SB4030's
message says. The first two rows are that message's advice, and it now
compiles on the left of an `=`.

So the read and write diagnostics no longer have to differ, and the write
message no longer has to avoid recommending a form that would not compile.

## DG's steer

> Rather than prioritize bash consistency, I want to prioritize sash
> self-consistency.

That settles the direction: `[[ -t $s ]]` matching bash is **not** a reason to
keep it lenient. It does not settle the scope.

## DD#89 — amended 2026-09-26

The false clause ("a string argument coerces to int, **as in every other int
context**") is struck, with the measured table recorded in its place. The
amendment deliberately does **not** decide whether `exit $s` should reject —
that is the behaviour change this task is still about.

DG has separately raised whether **`-t` should stop being a test operator**
and become a distinct predicate (`if $(terminal? $fd)`), following SB5019
having already moved the `-eq` family into `(( ))`. That would retire one of
the remaining sites by removing the surface rather than tightening it.

## Note for whoever implements

Each site has its own lowering, so this is not one switch:

- `exit` — `codegen/dispatch.rs::emit_exit` / `emit_c.rs::emit_exit`
- `wait` — `codegen/dispatch.rs::emit_wait` / `emit_c.rs::emit_wait_c`
- `-t` — `codegen/tests.rs::emit_isatty_test` / `emit_c.rs::lower_test_unary`
- `arg` — `codegen/value_handlers.rs::lower_arg_intrinsic`

The rejection itself belongs in `type_check.rs` (where SB4030 already lives),
not in codegen — codegen only sees a value whose type was already settled.
The subscript fix is the worked example: one predicate in `type_check.rs`
closed both surfaces.

Also: `return`'s fix was a **native/c divergence** that no spec caught. Check
both backends at each site before assuming they agree.

## Closing (2026-09-26)

DG scoped the remaining four, and all four are now closed — by rejection in
every case, never by adding a coercion.

| site | closed by | how |
|---|---|---|
| `exit $s` | `a875d58` | int-only sink (`check_int_sink_operands`); `exit $(( ${s:?m} ))` is the accountable spelling |
| `wait $s` | `a875d58` | same check; `$!` also became int-typed so `wait $!` still works |
| `$(arg $s)` | `a875d58` | same check — and it was the worst of the three: it silently yielded **argv[0]** |
| `[[ -t $s ]]` | journal 418 | the operator is **retired** (SB5028), so the site no longer exists |

`-t` is worth noting as the one that did not need a rule. It was the sole
int-typed unary test in a `[[ ]]` family that is otherwise entirely string and
file predicates, and the only unary arm whose codegen had to `atoi` its
operand. Retiring the operator removed the int sink along with it. DG also
declined a `terminal?` predicate replacement as YAGNI.

The traversal fix in `6013848` matters here too: before it, these operand
checks never ran on an assignment RHS, so `str t=$(arg $s)` would have slipped
past any rule written for them.
