# 108 — Rubric: irreducible vs needlessly complex

Built incrementally across 8 cognitive-vs-cyclomatic refactors
this session (journal entries inferred — see commit messages
`30ee686` through `29c09cf`). DG asked: how do we distinguish
the "irreducible" complexity from the "genuinely needlessly
complex"? The cog-vs-cc divergence pattern is the answer.

## Data points

All eight functions started in the post-cyclo-allow-retirement
double-failure cohort (cc > 11.7 AND cog > 8.8). Measured before
and after each refactor:

| # | function | cc before/after | cog before/after | shape |
|---|---|--:|--:|---|
| 1 | `value_is_tainted` (ssa_taint.rs) | 40 → 10 | 47 → 1 | over-wide dispatcher with shared arm bodies |
| 2 | `compile_ssa` (codegen/entry.rs) | 20 → 1 | 44 → 0 | duplicated 50-line trace blocks (Branch/Loop) |
| 3 | `string_parts_to_exprs` (parse/expr.rs) | 17 → 2 | 44 → 1 | 4-level if-let cascade (`$arr[i]` lookahead) |
| 4 | `init_positional_params` (codegen/setup.rs) | 22 → 5 | 42 → 6 | 185-line two-phase + N-arm dispatch |
| 5 | `check_dq_stmt` (validate.rs) | 21 → 8 | 31 → 3 | $?-state-machine walker over StmtKinds |
| 6 | `validate_arith_context` (validate.rs) | 12 → 3 | 29 → 3 | flat cc, deep cog (condition/statement fork) |
| 7 | `infer_stmt` (type_check.rs) | 19 → 6 | 28 → 6 | journal-056 bug + per-arm fold |
| 8 | (assorted Phase-11 refactors before cognitive landed) |

## The rubric

For any function flagged in the report, compute **delta = cog −
cc**. The delta's sign and magnitude tells you whether it's
irreducible, refactorable, or pretending to be either.

### Delta interpretation

| delta range | what it means | action |
|---|---|---|
| **delta ≥ +15** | Pure hidden nesting — cc didn't see it because the branches are inside ifs / let-elses / nested matches. cyclomatic was lying about how simple the function is. | **Refactor with flatten patterns**: `?`, `let-else`, early-return, helper extraction. Expect cog to drop 80-100%. |
| **delta +5 to +15** | Real branching distributed across a function body. cc and cog mostly agree but cog is rightly more sensitive to nesting. | **Refactor with phase or per-arm extraction**. Both metrics will drop similarly. |
| **delta −5 to +5** | Metrics agree. Genuine refactor candidate if cc and cog are both > 2× mean. | **Standard helper extraction**. |
| **delta −15 to −5** | Match-arm dispatcher with non-trivial arms. cc inflated by arm count; cog charges only one for the match. | **Refactor only if cc is very high (> 30) AND arms have real logic.** If arms are 1-3 lines each, leave alone — extraction would be noise. |
| **delta ≤ −15** | Pure wide dispatcher. cc = ~N variants, cog = 1. Irreducible — this is "look up which variant, do the corresponding thing." | **Leave alone.** This is what `cyclo-allow` used to annotate; cognitive now signals it for free via cog ≤ 3. |

### When both are above mean

A function with `cc > 2× mean_cc` AND `cog > 2× mean_cog` (the
"double-failure" cohort) is **definitionally a refactor target**
regardless of delta. The current means are cc 5.86, cog 4.38;
thresholds 11.7 and 8.8. After the wave so far: 44 such
functions, 8 in deferred emit_janet, 36 elsewhere.

### When the function is a dispatcher

Pure dispatchers (delta ≤ −15, cog ≤ 3, cc may be very high)
fall into one of three categories:

- **Enum dispatch** (`to_tok`, `from_name`, `lower_value`,
  `lower_one`) — match on enum variant, one arm per variant,
  each arm is a simple call or constant.
- **Lex / token-class dispatch** (`classify_word_token`,
  `tok_to_text`, `keyword_to_str`) — match on token kind, one
  arm per token, each arm returns a label or routes to a
  specific scanner.
- **Display / Debug impls** (`display`, `as_u8`, `sigil_str`) —
  exhaustive match for `fmt::Display` / `From` traits.

All three categories are irreducible by construction. cog 1 says
"this function has no real decision points; it's a function
table." Refactoring would degrade readability without any
correctness or maintainability gain.

## Refactor patterns observed

By function signature, the refactor that works:

### High delta (+15 or more) — flatten

```rust
// Before: 4-level if/if-let cascade
if x.is_some() {
    if let Some(y) = x {
        if y.starts_with('a') {
            if let Some(z) = y.find('b') {
                do_thing(z)
            }
        }
    }
}

// After: extracted helper with ?/let-else
fn try_thing(x: &Option<String>) -> Option<()> {
    let y = x.as_ref()?;
    if !y.starts_with('a') { return None; }
    let z = y.find('b')?;
    do_thing(z);
    Some(())
}
```

Each `?` and `let-else` is linear flow to cognitive, so
nesting goes to zero. cc drops 8-10×, cog drops 30-50×.

### Multi-phase function — name the phases

```rust
// Before: 185 lines doing setup-then-loop-then-dispatch
fn init_positional_params(...) {
    // 30 lines of argv/argc setup
    // 30 lines of pre-scan SSA → ref set
    // 100 lines of per-name init with 6-arm dispatch
}

// After: phases as named helpers
fn init_positional_params(...) {
    let (argv, argc) = setup_argv(...);
    let refs = collect_positional_refs(program);
    for name in &refs {
        if should_skip(name) { continue; }
        vars.insert(key, init_one_positional(name, ...));
    }
}
```

cog drops to phase count + dispatch arms (usually 5-8).

### Duplicated shape — share with parameter

```rust
// Before: ~50 lines per arm of identical-shape code
match terminator {
    Branch { true, false } => {
        // 50 lines emitting trace for branch outcome
    }
    Loop { body, exit } => {
        // 50 nearly-identical lines emitting trace for loop outcome
    }
}

// After: one helper, parameterized by kind
fn emit_branch_outcome_trace(ctx, kind: &str, true_target, false_target) {
    // 50 lines using kind for label formatting
}
```

cog drops 30-50 in a single change — the originally-duplicated
shape is "doing the same thing twice", and cognitive correctly
recognises that expressing it once costs nothing.

### State-machine walker — per-arm extraction

```rust
// Before: large match with per-arm logic + post-arm common code
fn check_dq_stmt(stmt, state, diags) {
    match &stmt.kind {
        StmtKind::List(l) => { /* 6 lines */ }
        StmtKind::If(i) => { /* 12 lines */ }
        StmtKind::While(w) => { /* 7 lines */ }
        _ => { /* 15 lines of leaf logic */ }
    }
}

// After: per-arm helpers, dispatcher is one-line-per-arm
fn check_dq_stmt(stmt, state, diags) {
    match &stmt.kind {
        StmtKind::List(l) => check_dq_list(l, state, diags),
        StmtKind::If(i) => check_dq_if(i, state, diags),
        StmtKind::While(w) => check_dq_while(w, state, diags),
        _ => check_dq_leaf(stmt, state, diags),
    }
}
```

cog of dispatcher drops to ~3 (one per arm). Each helper carries
its own cog ≤ 9.

## Secondary findings

1. **Cyclo-allow was actively misleading.** The 51-comment set
   had 12 false-positives — functions annotated as "intrinsic
   dispatcher" that actually had cog ≥ 20 (real hidden
   complexity). Cognitive surfaces these without any human
   annotation needed.

2. **Hidden bugs become visible.** The infer_stmt fix (commit
   `29c09cf`) found a journal-056-shape bug missed by that
   audit because infer_stmt isn't a validation walker. Cog 28
   on a 19-line cyclomatic suggested "this match is doing too
   much"; reading it revealed the missing default arm.

3. **Cognitive responds disproportionately to refactor success.**
   In every data point, cog dropped more (in ratio) than cc.
   When a refactor works, cog goes near zero; cc drops by a
   factor of 4-20 depending on dispatcher breadth.

4. **The metrics together resist gaming.** A would-be lazy
   refactor that extracts an arm into a helper without
   flattening it would drop cc (fewer match arms) but leave
   cog roughly unchanged (the helper still has the same
   nesting). Cognitive forces honest refactoring.

## Gate proposal (deferred)

Once the wave is complete (target: zero double-failures outside
`emit_janet.rs`), promote the report-only top-5 to a soft gate:

- **Hard gate** on cog > 30 (no single function may have
  cognitive ≥ 30 unless explicitly waived for one commit).
- **Soft gate** on (cc > 2× mean AND cog > 2× mean) — report
  as warning, allow.
- **Pure cc gate** continues to exist but ranked below cog.

Don't enable yet. The cohort still has ~36 double-failures
outside `emit_janet`; let the wave finish first.

## Cohort progress this session

- Started: 50 double-failures (8 janet, 42 non-janet).
- Refactored: 8 functions, all dropped out of cohort.
- Current: 44 double-failures (8 janet, 36 non-janet).
- Means: cc 5.86 → 5.63, cog 4.38 → 4.01 (both pulled down by
  the refactor wave).
