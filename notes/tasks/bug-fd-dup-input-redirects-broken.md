title: "`<&` and `<>` redirects are accepted but wrong in BOTH backends"
status: done
completed: 2026-09-13
priority: P2
kind: bug
depends: []
gates:
  - "DG decides the disposition first: implement `<&`/`<>`, or reject them at parse with a real diagnostic"
  - "whichever is chosen, the construct never again compiles to a silently-dropped redirect"
  - "spec tests pin the chosen behaviour in both backends, with no mode guard"

# `readline l <&9` and `readline l <> f` silently do nothing

Measured 2026-09-13. Both forms are accepted by the parser and the validator,
compile with rc=0, and then fail to perform the redirect.

```sh
exec 9< f              # f contains "hello"
readline line <&9
attest line [[ -n $line ]]
echo "got:$line"
```

| backend | result |
|---------|--------|
| bash    | `got:hello` |
| native  | attest gate fails — `line` is empty |
| c       | attest gate fails — `line` is empty |

Same for `<>` (`RedirectOp::ReadWrite`).

## Two different defects with the same symptom

**c fails open.** `emit_c.rs:3362` handles `DupOutput` and defers `Write` /
`Append` / `Read` to `open_flags_for_op`. `DupInput` and `ReadWrite` hit

```rust
_ => { writeln!(e.out, "    /* TODO redirect op: {:?} */", r.op).unwrap(); }
```

and return `None` — the fd is saved by the surrounding `sash_dup` and then
never redirected. The command runs against the wrong fd.

**native mis-maps.** `codegen/redirects.rs:92` reads

```rust
RedirectOp::Read | RedirectOp::DupInput => 0,
```

so `<&` is treated as `<`: an **open(2) of a file whose name is the fd
number**. `<&9` tries to open a file called `9`. `ReadWrite` is likewise
unhandled.

## Disposition is DG's call

Two defensible answers, and this task should not proceed until one is chosen:

1. **Implement them.** `<&` is `dup2`, which the runtime already exports
   (`sash_dup2`, used by `DupOutput`). `<>` is an `open` with `O_RDWR`. Both
   are small. But neither has a DD sanctioning it as part of the safe subset,
   and `<>` in particular is a sharp edge with little payoff.
2. **Reject them at parse.** Consistent with how the other dropped bash
   features are handled (SB4034 and friends): a real diagnostic naming the
   construct and the alternative. This shrinks the surface rather than growing
   it.

The one option that is *not* available is the status quo, where the construct
compiles and silently does the wrong thing.

## Related

`spec/coverage.sh:100` (cov.86-88) already pins that a `>&`/`<&` target which
isn't a plain fd number is **rejected**. So the malformed case has coverage
and the well-formed case does not — which is how this survived.

## Completion (2026-09-13)

DG chose **implement** over reject. Both ops now work in both backends.

- **`<&N` is the same operation as `>&N`** — `dup2(N, fd)`. They differ only
  in the fd they default to, which `default_fd_for_op` already decides. So
  `emit_dup_output` was renamed `emit_dup_fd` in both backends and `DupInput`
  routed to it. No new runtime call.
- **`<>` needed a runtime flag code**: `3 => O_RDWR|O_CREAT` in
  `sash_redirect` (create if absent, **do not truncate** — that is bash).
- **A second bug fell out of testing against bash**: `<>` was defaulting to
  **fd 1**. Bash defaults it to **fd 0**, because `<>` is a `<` form. With fd
  1, `echo xy <> f` overwrote the head of the file and looked plausible
  (`cat f` showed `xy` followed by the tail of the old content); bash prints
  `xy` to stdout and leaves the file untouched. `ex.14` pins this.
- The c redirect fall-through — the **last** fail-open `/* TODO */` arm in
  `emit_c.rs` — is now an ICE.

**Correction to the filing above:** the original text claimed native "maps
`DupInput` onto `Read` flags and opens a file named `9`". That was a
misreading of `default_fd_for_op` (which fd to default *to*) as
`open_flags_for_op`. Native **no-opped** both redirects, same as c.

Specs `ex.12`–`ex.15` (`spec/exec.sh`, plan 19 → 23), unguarded in both
modes. Fail-first: reverting the three source files failed exactly 3 of 4
in each mode (`ex.15`, create-if-absent, passes without the fix because the
shell harness leaves the file behind).
