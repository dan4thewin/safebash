# 320 — The write that was never the hard part

DG: *"lets do the profraw for sash-rt and keep ratcheting the branch
coverage."*

## sash-rt was never unmeasurable — it was unwritten

`contrib/coverage.sh` has carried this line for months:

> sash-rt is not measurable this way at all — it is no_std with no libc, so
> nothing writes a profraw at exit.

Both halves of that sentence are true and the conclusion drawn from them was
wrong. `-C instrument-coverage` compiles against `sash-rt` perfectly well: the
counters and the `__llvm_prf_*` sections land in every binary it produces. A
fresh instrumented build has **2762** `__profc` symbols in the archive. The
data was always there. What was missing was thirty lines that hand a buffer to
`write(2)`.

The blocker was never instrumentation. It was that
`__llvm_profile_write_file()` — the function compiler-rt registers as an
`atexit` handler — wants `FILE*`, `getenv`, `snprintf` and `flock`. A no-libc
static ELF has none of those, so every measured run exited with live counters
and left nothing behind.

`__llvm_profile_write_buffer()` is a **different object** and wants none of it.
That is the whole trick. Taking the symbol closure of the buffer path out of
rustc's own `libprofiler_builtins`, seven objects suffice, and `ld -r` over
them leaves exactly four undefined symbols:

| symbol | supplied by |
|---|---|
| `__ehdr_start` | the linker, in any static link |
| `memcpy` | `sash-rt/src/lib.rs:32` — already there |
| `memset` | `sash-rt/src/lib.rs:42` — already there |
| `memcmp` | `sash-rt/src/lib.rs:52` — already there |

The `__start___llvm_prf_*` / `__stop___llvm_prf_*` bounds are weak and
linker-provided. The *full* archive needs ~42 libc symbols; the buffer path
needs three we already define.

I first measured this against brew's `libclang_rt.profile-x86_64.a` and it
works, but rustc's own `libprofiler_builtins` is the better source and not for
size reasons: the `.profraw` header version is written by
`InstrProfilingVersionVar.o` while the counter *layout* is decided by the
rustc that instrumented the code. Taking both from one toolchain means they
cannot disagree. Mixing brew clang 20 with rustup rustc produces a file
`llvm-profdata` rejects — or worse, misreads.

Nothing in `sash-rt/src/coverage.rs` knows the `.profraw` format. compiler-rt
does the serialising; we supply a destination. An LLVM bump cannot silently
corrupt the output because we never encode it.

## The exit that no Rust code could hook

The dump needs somewhere to run, and `sash-rt` had seven `exit_group` calls
plus one more exit that was not a call at all:

```
"call main",
"mov rdi, rax",
"mov eax, 60",
"syscall",
```

The **normal** exit path — a program that runs to completion — was two
instructions of naked assembly, written once per architecture. Any at-exit
work had eight sites to add and two of them were in `asm!`.

`sash_startup` already exists for exactly this reason, and its doc comment
records what happened without it: aarch64's `_start` called `sash_init_heap`
but not `sash_trace_init`, so `--trace` on aarch64 produced a binary that was
larger, slower and silent. The same drift, one function further along.

So `sash_shutdown(status) -> !` is now the mirror: `_start` ends in `call
sash_shutdown` on x86_64 and `bl sash_shutdown` on aarch64, `sash_exit`
delegates to it, and the six remaining direct `exit_group` calls route through
it. One `exit_group` remains in the tree, inside `sash_shutdown`. This ships
unconditionally — only the `coverage::dump()` inside it is feature-gated — and
the sweep is unmoved at 4699.

## Two things that look fine when wrong

**The reference binary.** `sash` links with `--gc-sections`, so each compiled
program keeps only the runtime objects it actually calls, and its covmap
describes a *subset* of sash-rt. Reporting against one of those silently
scopes the result to whatever that one program happened to touch. The script
builds a `--whole-archive` reference binary instead, which needs
`--unresolved-symbols=ignore-all` because whole-archive also drags in
`compiler_builtins` members referencing `rust_eh_personality`. That binary is
never executed — `llvm-cov` reads two sections and nothing else.

**The variable name.** The dump keys on `SASH_RT_PROFILE_DIR`, not
`LLVM_PROFILE_FILE`. cargo-llvm-cov sets the latter for the *compiler*, and a
compiled program that honoured it would overwrite the compiler's own profraw
on every run — during a sweep, thousands of times.

Filenames are `<pid>-<nanos>.profraw`. The pid alone is not unique enough: a
sweep runs thousands of short-lived binaries and the kernel recycles pids, so
two runs would collide and one would be lost silently.

## The first number

2925 profraws from a full sweep:

| file | lines | branches | functions |
|---|---|---|---|
| `lib.rs` | 82.43% | 67.93% | 91.34% |
| `inner.rs` | 67.81% | 66.67% | 76.47% |
| **total** | **81.51%** | **67.83%** | **89.58%** |

`coverage.rs` is excluded from its own report — it exists only under the
feature that turns measurement on, so no shipped binary contains it and
counting it would be the measurement measuring itself.

One warning survives: `linux_raw_sys::elf::ELF_ST_BIND` reports mismatched
data. It is one function of 423, it is a third-party crate, and it is already
outside the filtered scope. Identified rather than assumed benign.

`mk rt-cov`, `mk rt-cov-branch`, `mk rt-cov-reset`.

## Compiler branches: 85.34%, and four arms

Re-measured at `4fe07a2`: **85.34% branches, 551 missed** (was 85.12% / 562);
lines 95.33%, functions 97.51%.

**`wrap_with_redirects` never wrapped.** Of 18594 calls to
`wrap_pipeline_part`, 18574 arrived with no redirects and the other 20 all
returned SB4046. Lines 2244–2252 — the entire `StmtKind::Redirected`
construction — were never executed. The reason is that a command absorbs its
own trailing redirects in `parse_simple_command` (325 times in the same
sweep), so the only kinds still holding one at that point are precisely the
kinds `noncommand_redirect_target` names.

Deleting the dead arm silently would have been the wrong repair. If some
statement kind later gains the ability to carry a redirect past
`parse_simple_command`, the `None` case is reached and the redirect would
simply **vanish from the AST** — output going to a file the program never
opens, no diagnostic. So it became `redirect_reject_noun`, which panics on
`None`, with `redirect_reject_noun_is_loud_ice` to prove it, since no source
text can reach it. That is DG's rule for this campaign applied as stated: dead
code goes, a valuable should-never-run assertion gets a test that proves it
catches what it claims.

**Two unreachable `let`-`else` arms, both cross-pass, both commented.**
`resolve::resolve_command` and `validate::check_echo_splat` both destructure a
command head as `ExprKind::Literal` and return on anything else. Neither
`else` has ever run — 8221 and 8125 commands respectively. I instrumented the
first rather than reason about it, and the probe never fired.

The cause is pass ordering. A non-`Literal` head is SB5011 from
`validate::check_command_head`, which runs inside `run_structural_checks` —
and that exits before `resolve_commands` is called at all. `check_argv_splat`
is even further downstream, in `run_semantic_checks`. Both stay plain
`return`s: the guarantee lives in a different pass, and an ICE would encode a
cross-pass ordering neither file can enforce. This is the project's standing
rule — a *local* invariant earns an assertion, a *cross-pass* one earns a
comment — and these are the second kind.

**One guard removed.** `is_resolvable_external` opened with `name.is_empty()
|| name.contains('/')`. The `is_empty()` half never fires: `''` and `""` lex
as quoted forms, not literals, and the word scanner never emits a zero-length
unquoted word. It also bought nothing — an empty name would fall through to
SB5011 "command '' not found" regardless.

**One reachable arm that simply had no test.** `is_identifier_or_subscript`
splits on the first `[` and requires the word to end in `]`. The "does not end
in `]`" arm is reachable — `a[1=2` and `a[1]x=2` both take it — and had no
spec. Nothing would have caught its removal. `aw.27`/`aw.28`.

## What the counts could not settle

Three sites resisted diagnosis from counts alone, which is the standing
lesson: `lexer.rs:1399` (`pos < len` never false — a real loop bound whose
false exit is foreclosed by SB4057 firing earlier, so it stays),
`codegen/entry.rs:478` (all three embedded artifacts apparently always
`Some`), and the branch-index ambiguity on `parser.rs:2669`, where `aw.20`
appears to reach an arm the data calls unvisited. Filed rather than guessed
at.
