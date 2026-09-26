---
status: next
priority: P1
kind: bug
---

# `> "${n:?msg}"` on an int is broken in both backends

DD#119 says a redirect target must be provably non-empty, and exempts ints
("exempt but still needs quotes to make a string", DG). The quoted,
dispositioned spelling the rule directs authors to **does not work**.

## Reproducer

```sh
int n=5
/bin/echo hi > "${n:?x}"
```

- **native** — prints `hi` to **stdout**. The redirect does not happen at all.
  A silent wrong answer, not a crash: same class as the `latest_var_as_str`
  bug fixed in 006fce2.
- **c** — emits `n_ptr` / `n_len` into the generated C. Those are the string
  slots of a `VarSlot`; `n` is an `i64` and has neither.

```c
sash_require_value(n_len, "x", 1);
__redir_target_1_0_ptr = n_ptr;
__redir_target_1_0_len = n_len;
```

`sash_require_value(n_len, …)` is also asking the wrong question — an int has
no length, and the whole point of the int exemption is that it is non-empty by
*type*, so the runtime check is redundant as well as unbound.

## How it surfaced

Invisible until `$!` became int-typed (so that `wait $!` could satisfy the new
int-only rule for `wait`). `cov.252b` does `int c=$!` then
`> "${c:?no child}"`, and c-mode failed while native passed — the differential
oracle again. Probing then showed native is *also* wrong; it just fails
quietly, which is why a passing native spec meant nothing.

Nothing previously exercised int + disposition + redirect together.

## Scope

Not `$!`-specific. Any int variable, any disposition. Confirmed with a plain
`int n=5`.

Note the two backends fail *differently*, so this is likely two faults sharing
a cause upstream: the transform hands the redirect path an int slot without
converting it, and each backend then mishandles that in its own way. The
native fix in 006fce2 (`redirect_expr_simple_expansion` rendering ints via
`emit_int_to_str`) covered the **bare** `$n` spelling; the dispositioned
spelling goes through a different path and was missed.

## Gates

- `spec/coverage.sh` cov.252b is the live failing spec (c mode).
- Add specs for a plain `int n=5` in both `:?` and `:-` forms, native and c.
- Prove non-vacuity per backend separately — native's failure mode is silent,
  so a spec must assert the file contents, not just the exit status.

## Blocks

DG chose (2026-09-26) to fix this **before** landing the exit/wait/arg
int-only tightening, because DD#119 is already committed telling authors to
write a form that fails. The int-sink work is stashed
(`git stash list` → "int-sink work (DG option B…)") and also saved as
`int-sink-v2.patch` in the session files. Reapply it after this is fixed.
