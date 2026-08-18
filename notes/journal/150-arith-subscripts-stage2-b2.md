# 150 — arith subscripts model B Stage 2 + B2 (DD#79)

DG: `./sash -c 'int array a=( 7 13 21 ); (( a[i]++ )); echo $a[$i]'` — shouldn't
this work? It didn't (SB4035). Investigation confirmed it's the ratified but
unbuilt **Stage 2** of `arith-subscript-values` (model B), plus a design seam
DG then resolved.

## What was missing

Stage 1 (`${a[i]}` reads) had shipped. Stage 2 — bare `a[i]` in arith (rvalue +
lvalue) — was designed (model B, 2026-08-16) but not implemented: the arith
lexer hit `[` → SB4035. And a second issue in DG's line: `echo $a[$i]` read `$i`,
which the arith index `i` never assigned → SB5031.

## The seam DG resolved (→ B2)

In arith an undefined name *read* 0 (`$(( y+1 ))` → 1); outside arith `$y` is
SB5031. So `(( a[i]++ ))` accepted an undefined index `i` as 0, but the later
`$i` was rejected — same name, opposite treatment. DG: "using i gives a zero but
leaves i nonexistent… once I referenced it, it should exist" → then "a
consistent application of SB5031 could also make sense."

Chose **B2** (over auto-vivify-on-read, which masks typos; and status-quo silent
0): extend SB5031 into arith — a bare name *read* assigned nowhere is a typo.
Anything you *assign* (`(( f++ ))`, `for (( i=0;… ))`) is in scope, so it exists.
Recorded as **DD#79** (both the model-B table and B2).

## Implementation

**Parse.** `scan_arith_ident` captures a trailing balanced `[…]`
(`scan_balanced_brackets`) as a `BareSubscript(name, idx)` token; the parser
re-parses `idx` as a nested arith expr → `ArithExpr::Subscript{name, index}`.
`try_postfix`/`try_assignment` accept a `Subscript` LHS →
`SubscriptAssign{name, index, op, value}` (`a[i]++`/`--`, `a[i]=v`, `a[i] op=
v`). `classify_arith_expr` treats `SubscriptAssign` as a mutation.

**Lower.** `lower_arith_mutation_structured` turns `SubscriptAssign` into the
existing statement `Assignment{name, index: Some(Arithmetic(idx)), value:
Arithmetic(<computed>)}` — reusing the subscript-assign write path (the
computed value embeds the `a[i]` read for `op=`/`++`). The rvalue lowers
(`arith_to_ssa` → `subscript_read_to_ssa`) to `SsaValue::Subscript` with the
arith index as an int key (`lower_subscript_raw`'s integer-key path calls
`lower_value_as_int`, so an int SSA index works directly — no conversion).

**Reject bare `m[k]` (silent-wrong bug).** `int map m; $(( m[k]+1 ))` used to
give 1 and `(( m[k]++ ))` left the cell unchanged (int-key miss on a
string-keyed map). `type_check` now flags `Subscript`/`SubscriptAssign` on an
assoc array (read form) and a lowered assignment with an `Arithmetic` index on a
map (write form) → SB4030.

**B2.** `check_undeclared_reads` descends `ExprKind::Arithmetic`
(`check_arith_reads`): a bare `Var`/`DollarVar`, a subscript array name, and
every index/operand are checked against the in-scope set. Since `(( f++ ))`
lowers to an `Assignment` (its name collected), assigned names pass; pure reads
of unassigned names error. **Function names** are collected into the scope set
so DD#58 nullary calls (`$((f))`) aren't false-flagged (this was the only sweep
breakage — 4 tests — before the fix).

## Verification

Matrix (native): reads `a[0]+1`→8, `a[i+1]`→21; lvalues `a[i]++`→8, `a[2]+=5`→26,
`a[0]=99`→99; conditions `(( a[0]>5 ))`→yes; OOB `a[9]`→0 (zero-value); bare
`m[k]` → SB4030; B2 `$(( y+1 ))`/`a[i]`(undeclared i) → SB5031; `(( f++ ))`,
`for ((i=0;…))`, `int i=1;$(( i+1 ))` → fine. DG's line with `int i=0` → 8.

Full 3-mode sweep: **0 passing→failing, +24 passing** (bsx.1-7, b2a.1-3 across
modes); native 0-fail. clippy clean; complexity gate green (flattened
`arith_to_ssa` cc 14→13, `check_sexpr_string_producers` cog 13→7,
`scan_balanced_brackets` cog 11→5). native+c; janet arith-value subscript is the
pre-existing filed gap (`janet-arith-subscript-read`, extended);
`janet-undeclared-arith-counter` filed for `(( f++ ))`.

Follow-ups (filed, non-blocking): `arith-bare-subscript-followups` — `$a[i]`
dollar form in arith (SB4035), nested `a[a[0]]`, and the DG design question of
whether a *string*-array cell in arith needs a DD#77 disposition.

DD#79 recorded (model B + B2). arith-subscript-values marked done.
