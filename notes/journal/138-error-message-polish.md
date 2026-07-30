# 138 — error-message polish: bless diagnostic + SB5029

DG error-message review. Two fixes.

## Runtime bless failure — was too terse

`bless h [[ $h =~ zzz ]]` (with `$h=/home/dangood`) printed only:

```
bless: validation failed
```

— no variable, no value, no program name. Now:

```
<argv0>: bless: h = '/home/dangood' failed the gate
```

You can see *which* gate failed and *why* (the value that didn't satisfy
the predicate). New runtime helper `sash_rt::sash_bless_fail(name_ptr,
name_len, val_ptr, val_len)` composes `<argv0>: bless: <name> = '<val>'
failed the gate` (the `= '<val>'` clause is omitted when `val_len == 0`,
e.g. a bless-all over a collection → `<argv0>: bless: <arr> failed the
gate`). Reuses the `sash_argv[0]` write from `sash_emsg`.

Codegen: `intern_inst` now interns the source var **name** (instead of the
old static string); scalar `emit_bless` loads the source value `(ptr,len)`
from its var slot and calls `sash_bless_fail`; `emit_bless_fail` (shared by
`BlessAll`) passes the name with a zero value. Declared the extern in
`setup.rs` / `ExternFuncs`. emit_c/janet unaffected (native runtime path).

Spec: taint.sh 24.20 (native-only — c/janet discard runtime stderr) pins
that the diagnostic contains `bless: x = '` and `failed the gate`.

## SB5029 — too long

`echo *`:
- was: *inline `` * `` glob is not allowed in command arguments — a glob is
  array sugar: use `` array a=(*); … @a ``. Quote it ("*") for a literal
  asterisk.* (140c)
- now: *inline `` * `` glob is not allowed in command arguments — use an
  array: `` array a=(*); … @a `` (or quote "*" for a literal asterisk)*
  (drops "is array sugar", ~40c shorter).

native+c+janet 0 regressions (native 814); codegen/taint_spec unchanged
(182/74, 18/24 — stash-verified).
