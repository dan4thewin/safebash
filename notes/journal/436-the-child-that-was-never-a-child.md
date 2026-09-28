# 436 — The child that was never a child

`SsaValue::child_values()` ended in `other => other.operand_child_values()`,
and two helpers below it ended in `_ => return None`. Journal 435 closed the
taint read path by making it total; this one asks the obvious follow-up
question about the getter one level down, and the answer was a live bug.

## The triage, before any probe

Four children were silently dropped: `FuncCall { args }`,
`IntFuncCall { args }`, `IntCell(inner)`, and `ArraySplat`'s disposition word.
Three consumers recurse through the getter: `value_is_tainted` (taint
derivation), `Interner::intern_value` (codegen — under-interning **panics**),
and `contains_capture` (transform).

Reading the consumers narrowed it fast. `value_is_tainted` has an explicit
`Taint::Clean` arm for `FuncCall`/`IntFuncCall` (DD#24) and an explicit
recursion arm for `IntCell`, so neither is a taint hole. `contains_capture`
hand-compensates for `IntCell` and `ArraySplat` with its own arms, and returns
`true` for the call variants before children matter. `intern_value`
hand-compensates for `ArraySplat` — with a comment saying `child_values` is
"kept lean".

Three hand-compensations for one getter's omissions. That is the tell.

## The one that nobody compensated

`ArraySplat` reaches taint through `splat_taint`, not through `child_values`,
and `splat_taint` read exactly two things: the array's name and `taint_off`.

```rust
if disposition.as_ref().is_some_and(|d| d.taint_off) {
    Taint::Clean
} else {
    taint_of_name(taint, array)
}
```

The `:-WORD` default is never consulted. So:

```sh
array a=( )
str t=$(getenv T)
array b=( @{a:-$t} )
```

`a` is clean, `taint_off` is false, so the splat derives **Clean**, and `b` is
classified clean with a tainted value in it. Every later read of `b` passes.

Measured with `--no-taint-cst`: the baseline `array b=( $t )` rejects, the
splat spelling compiles. Same shape as every escape this project has recorded
— a walk that is total on the derivation side for 29 variants and delegates
the thirtieth to a helper that looks at two fields out of four.

At top level the front-end pass masks it (SB5021). Inside a `$( )` body, where
the front-end does not extend its tainted set, it is reachable:

```sh
array a=( )
/bin/echo $(str t=$(getenv T); array b=( @{a:-$t} ); echo $b[0])
```

That compiled. It now rejects at the backstop.

## What `!` does and does not vouch for

DD#91 scopes `!` to its own subject's read. `@{a:!-$t}` vouches for the
**array**, not for an unrelated variable spliced into the default, so the
word's taint still counts even when `taint_off` is set. The fix keeps those
two separate: `taint_off` suppresses only `taint_of_name(array)`.

A `:?` word is different again — it is an abort **message**, not a value. The
abort path exits, so it yields nothing and contributes no taint, exactly as
`RequireValue`'s message does not. And a wordless `@{a:-}` has `has_word`
false and a placeholder in `word` that the program never wrote; examining it
would reject valid programs over a value that does not exist.

Three field combinations, three different answers. The old code had one.

## Making the getter total

The complexity split (`child_values` → `operand_child_values` →
`subject_child_values` / `operator_child_values`) existed to keep each
dispatcher under the gate. It also made three separate places where a variant
could go missing, and the wildcards meant rustc reported none of them.

Collapsed to one flat match over all 30 variants, every leaf spelling out its
own empty arm. It lands at cc 23 / cog 1 — a delta of −22, comfortably inside
the wide-dispatcher exemption, which is what that exemption is for. The
`do not add a wildcard` marker puts it under `contrib/totality-check.py`.

The three hand-compensations came out with it. `contains_capture` lost two
arms and got the same answer; `intern_value` lost one and now interns the
splat word through the ordinary path.

## What the probes said about the other three

Worth recording, because the interner is the one that panics rather than
misbehaves quietly. Nested `FuncCall` args with a literal appearing nowhere
else, at one and two levels of lazy nesting, in a splat word, and
`IntFuncCall` (`strlen`/`len`) in both positions: no panic, correct output.
`IntCell` in int arrays, int maps, and subscript arithmetic: all fine. The
gap was real but unreachable — the lifted call's args are interned as
top-level instruction values before nesting matters.

So of the four dropped children, exactly one was a live bug. The getter is now
total anyway, because the next variant added is the one nobody triages.

## Proving the specs could fail

`splat_word_taint` neutered to `Taint::Clean`, rebuilt: both new negatives
compiled. Restored: both reject. Without that step a spec asserting
"rejects" is indistinguishable from a spec asserting nothing.

`bkstp.5`–`bkstp.8` pin the negative, the clean-word positive, the `!`
non-vouching case, and the wordless placeholder. Predicted 6737, swept 6737,
zero failing.

## The bug the probes tripped over

Writing those specs required declaring the array **outside** the `$( )` body,
because a body-declared one is rejected:

```
[SB5031] `@a` splats `a`, which is introduced nowhere
```

`a` is declared one statement earlier in that same body. `check_scalar_splat`
reports with `walk_expr_tree`, which reaches inside a body, and collects with
`program_scopes`, which does not — `child_bodies()` is total over `StmtKind`
and a `$( )` body hangs off an expression, so it structurally cannot appear.

The same asymmetry, in a different subsystem, on the same day. Not fixed here:
widening `program_scopes` trades a false rejection for a false acceptance, and
which one sash wants is a scoping decision under DD#86. Filed as
`bug-sb5031-blind-to-cmdsub-body-decls` with the tradeoff stated and the
implementation note (`direct_exprs` needs an explicit lifetime before a body
can escape the closure).

45 walks in `validate.rs` recurse via `child_bodies()`. A hand-written subset
descends into command substitutions. Nothing makes a walk choose. Every one
that reports inside a body and collects outside it has this bug waiting.
