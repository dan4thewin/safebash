---
title: "`$( )` classified by its FIRST statement: `argv` silently drops the rest, and the `argv N a` fill form is rejected as a splat"
status: done
priority: P1
kind: bug
depends: []
blocked_on: ""
gates:
  - "a statement after `$(argv …)` in a for-in source cannot be silently discarded"
  - "no diagnostic names the splat form when the author wrote the fill form"
  - "specs in both modes for both directions"
---

# The predicate

`validate.rs::cmdsub_command_is` (4823) decides what a `$( )` *is* by looking
at one statement:

```rust
fn cmdsub_command_is(stmts: &[Statement], name: &str) -> bool {
    let inner = match stmts.first().map(|s| &s.kind) { … };
    matches!(inner, StmtKind::Command(c) if expr_literal_str(&c.name) == name)
}
```

`stmts.first()` — not "the only statement". And it matches on the command
*name* alone, so it cannot tell DD#60's two `argv` forms apart:

- `$(argv)` / `$(argv N)` — the **splat expression**, legal only as a for-in
  sole source;
- `argv a` / `argv N a` — the **out-param fill statement**, legal wherever a
  statement is (DD#60: "the array is the trailing positional, like
  `mapfile -m m` / `read -a a` / `match S P arr`").

## Direction 1 — a statement is silently discarded (P1)

```sash
for x in $(argv; /usr/bin/touch /tmp/SIDE_EFFECT_RAN); do
  attest x [[ -n $x ]]
done
```

Compiles clean, loops correctly over the program arguments — and
`/tmp/SIDE_EFFECT_RAN` is never created. The second statement is dropped with
no diagnostic. Any side effect written there is silently skipped.

**`keys` is correctly guarded and `argv` is not**, which is what makes this a
gap rather than a design position. The identical shape on `keys`:

```sash
for x in $(keys m; /bin/echo extra); do … done
→ error [SB5073] `keys` is a value intrinsic — inside `$( … )` it is
  recognised only when it is the whole substitution, not one statement among
  several. Capture it first (`x=$(keys …)`) and use `$x` here
```

That message *is* the rule. `argv` was never added to whatever set SB5073
guards.

## Direction 2 — the fill form is rejected as the splat form

```sash
str s=$(argv 0 b; /bin/echo @{b:-none})
→ error [SB5025] $(argv) is an array splat — it is valid only as the sole
  source of a for-in loop; snapshot into an array with `argv a` (or
  `argv N a` to start at argument N), or use $(arg N) for a single argument
```

The author wrote `argv 0 b`. The diagnostic's advice is to write `argv N a`.
It is telling them to do what they did.

Position-dependent, which is the tell — the same statement is fine everywhere
else:

| position | `argv 0 b` |
|---|---|
| top level | accepted |
| function body | accepted |
| `if` body | accepted |
| `$( )` body | **SB5025** |

## Why this is a bug and not a deliberate restriction

Checked `notes/decisions/` first. DD#60 defines the two forms and restricts
only the *splat*: "**Unchanged:** the splat `$(argv)` / `$(argv N)` in a
for-in header". Nothing restricts the fill statement by position. DD#86's
2026-09-28 addendum makes a `$( )` body an anonymous function whose
first-introduced names are local, which is exactly the scope a local fill
wants.

## The open question for DG

Direction 1 is unambiguous — a dropped statement is a silent wrong answer and
`keys` already shows the intended answer. Direction 2 is a language question,
and the two fixes are the same code, so they are coupled:

- **(a) `argv N a` is legal inside `$( )`.** `cmdsub_command_is` requires a
  single statement *and* distinguishes the fill form by its trailing array
  name. `$(argv 0 b; …)` becomes an ordinary command substitution and works;
  `$(argv; CMD)` stops being a splat source and is diagnosed.
- **(b) it is not legal**, on the `keys` precedent — `argv` joins the SB5073
  set and both directions produce that message.

(a) treats `argv N a` as the statement DD#60 says it is. (b) is the smaller
change and keeps one rule for everything intrinsic-shaped inside `$( )`, at
the cost of forbidding a statement that is legal in every other body.

# Where

- `sash/src/validate.rs:4823` — `cmdsub_command_is`
- `sash/src/validate.rs:4743` — `is_argv_splat_expr`
- `sash/src/validate.rs:4751` — `is_keys_splat_expr` (same defect, but SB5073
  catches `keys` first — check whether it is load-bearing or luck)
- `sash/src/validate.rs:4757` — `is_array_splat_source`, the for-in gate

Found 2026-09-28 while probing whether `collect_bulk_targets` /
`collect_collection_names` shared the `$( )` descent gap fixed in journal 443.
**They do not** — `array`/`map` declared inside `$( )` are found correctly,
and `@zz` for a name introduced nowhere still reports SB5031 inside a
capture. That question is closed; this is what the probe turned up instead.


# Resolved 2026-09-29 — journal 444

**DG chose (a):** `argv N a` is legal inside `$( )`. It is a statement, and
DD#86 makes a body a function body.

Both directions are closed, and closing them exposed two deeper bugs — a
statement-builtin cohort that compiled to `sash: bug: bare command passed to
sash_exec`, and the finding that **42 of the 45 statement walkers in
`validate.rs` never entered a `$( )` body at all**. DG chose the structural
fix: bodies moved from the expression side of the IR to the statement side.
See the DD#86 addendum of the same date for the contract that establishes.

Specs: `spec/errors.sh` csc.11–csc.34, `spec/argv.sh` av.c1–av.c8.
