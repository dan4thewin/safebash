# 287 — one startup, written once

`--trace` did nothing on aarch64. Not "did the wrong thing" — the flag
that gates every trace-write site stayed 0, so `sash --trace` there
produced a binary that was larger, slower, and completely silent. A
feature reporting success and doing nothing.

The cause was three lines of assembly:

```
        "call sash_init_heap",
        "call sash_trace_init",     // x86_64
```

```
        "bl sash_init_heap",        // aarch64 — and that is all
```

`sash_trace_init` is what reads `SASH_TRACE` out of the environment.
aarch64 never called it.

## Why this one is interesting

The obvious fix is a fourth line. The task file (filed 2026-09-13 while
DG was asking whether `SASH_TRACE` was implemented at all) listed a third
gate that asks for something better:

> Ideally the two `_start` bodies stop being able to drift: the
> divergence is only possible because the startup sequence is written
> twice, once per architecture.

That is the actual defect. `_start` must be per-architecture — it is the
only assembly in the project, and it reads `argc`/`argv`/`envp` off the
raw stack, which is ABI-specific. But *what to initialise* is not
ABI-specific in the slightest, and it was being enumerated twice in a
language where nothing checks that two lists agree.

So the sequence moved into plain Rust:

```rust
#[no_mangle]
pub unsafe extern "C" fn sash_startup() {
    unsafe { sash_init_heap() };
    sash_trace_init();
}
```

and each `_start` now makes a single call to it. The assembly still does
the arch-specific part — stash `argc`, `argv`, `envp` — then hands off.
Adding an init step is a one-line edit in one language, and the two
architectures cannot disagree about it because they no longer each hold
an opinion.

Checking first whether anything else called those two symbols took one
grep across the whole tree: nothing did. Both are referenced only from
the two `_start` blocks. The C backend links the same `libsash_rt.a` and
uses the same `_start`, so it needed no change at all.

## The part I could not do

The gate says `tr.1`–`tr.3` should pass on aarch64. I cannot run them.

This host is x86_64 with no aarch64 hardware and no `qemu-user`. I added
the `aarch64-unknown-linux-gnu` rustup target to at least get a
compile-only check, and that is not enough either: `sash-rt` vendors C
sources — Plan 9 regcomp/regexec, heap4, `setjmp.S` — built through
`cc-rs`, which wants `aarch64-linux-gnu-gcc`. Installing it needs root.
I removed the target again rather than leave a half-usable one behind.

What I did verify is the half that *is* reachable, and it is the half
that could break working behaviour: x86_64 now routes through
`sash_startup` too. `SASH_TRACE=1` on a `--trace` binary still prints
`+ branch → if.then`, unset still prints nothing, and the full sweep is
4330 passing / 9 skipped / 0 failing / 0 flips. That last number is the
one that counts here — *every* compiled binary in the suite goes through
this function now, so an error in it would not have been subtle.

The aarch64 edit itself is `bl sash_init_heap` → `bl sash_startup`: same
instruction, same kind of symbol, no new assembly shape. Low risk. But
reviewed is not run, and I would rather say so in the task file than
tick the gate.

## No spec, on purpose

I considered adding one and decided against it twice over. The suite runs
on x86_64, so a spec for an aarch64-only bug cannot fail on the machine
that executes it — it would pass for the wrong reason forever. And the
drift is now prevented by construction rather than detected after the
fact, which is the stronger guarantee; a test that greps the two `_start`
bodies for agreement would be guarding a property the code no longer has
the shape to violate. No existing spec reads repo source either, so it
would have been a new and lonely idiom.

Worth noting what the original task file got right: it identified the
structural cause in the same breath as the symptom, and that is why the
fix is four lines of Rust instead of one line of assembly. The filing
quality did the work here.
