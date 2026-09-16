---
date: 2026-09-16
---

# A filter that cannot pass

Two filings and five specs. The specs were the easy half.

## `skip_dollar_var` — seven cold branches, three calls

`lexer.rs::skip_dollar_var` scans a `$name` inside a **brace-less
disposition operand** (`$x:-DEFAULT`, DD#90/#91). The whole sweep
called it three times, and every call was a single alphabetic byte
followed by a terminator. So seven branches had never run: the
special/digit arm, the leading underscore, an underscore *inside* a
name, the multi-byte scan loop, and the run-out-of-source exit.

`vm.bl16` looked like it covered this and does not — `${…}` in that
position takes `skip_braced_to_close` instead.

Five specs, `vm.bl19`–`vm.bl23`. One detail made the last branch
reachable without contortion: **`run_sash` passes source with `-c`, not
via a file**, so `'… /bin/echo $x:-$abc'` ends at the `c` with no
trailing newline, and the scanner exits on end-of-input rather than on
a terminator. Nothing had to be staged.

Sweep 5207 → **5217**, predicted exactly.

## The find: a filter that cannot pass

`codegen/setup.rs::collect_positional_refs` walks every value of every
instruction of every block, every compile, to collect "positional /
special" names. Its only filter:

```rust
if name.parse::<u64>().is_ok() || name == "#" || name == "@" || name == "0"
```

**89 calls, 0 inserts.** All four TRUE branches cold; line 1515 has
count 0.

What made it confusing for a while is that the *consumer* runs — the
loop body in `init_positional_params` executes 10 times. So the set is
non-empty. The resolution is that it has **two producers with different
contracts**, and only the other one contributes:
`redirects.rs::scan_redirect_for_vars` inserts every `SimpleExpansion`
name in a redirect target, unfiltered. That is the live path, and its
real job is DD#53 — `str f=out; /bin/echo hi > $f`.

The filter's own neighbour already explained why it cannot pass.
`init_one_positional`'s doc comment says the numeric / `$#` / `$@` init
strategies **were retired**: top-level positionals are
validation-rejected and function positionals resolve through `args`
(DD#60). Probed, every spelling the filter tests for is rejected before
codegen — SB5017, SB5024, SB5024, SB5045 — or resolves via `args`.

So ~46 lines in `values.rs` plus most of `scan_inst_for_vars` walk the
program to feed a predicate that provably cannot fire.

**I did not delete it.** The retirement is *documented*, which means
someone retired the strategies and left the scanner standing. That is
precisely the "might be deliberate" case, and the two readings — safety
net, or simply not noticed — lead to opposite actions. Filed as
`notes/tasks/purge-dead-positional-prescan.md` with three options and a
recommendation (delete; a net that cannot fire cannot catch a
regression either, and if a net *is* wanted, `unreachable!()` is the
honest spelling).

## Also filed: cross compilation and `setjmp.S` on mac

DG reported `sash-rt` failing to build on macOS.
`sash-rt/csrc/setjmp.S` is 43 lines of unguarded x86_64 AT&T assembly —
no `#if`, no `__aarch64__`, no `__APPLE__` — fed unconditionally to
`cc::Build`. It cannot build on a Mac twice over: Apple Silicon is
aarch64, and Mach-O mangles C symbols with a leading underscore, so the
exported `_setjmp`/`setjmp` pair aliases the wrong thing.

The interesting part is that this is an inconsistency *inside*
`sash-rt`: `src/lib.rs:1259` already carries a deliberate
`#[cfg(target_arch = "aarch64")]` `_start`, whose comment records a
real drift bug between the two init paths. The Rust half treats aarch64
as supported; the C half assumes x86_64.

Filed with the wider gap rather than alone, because fixing `setjmp.S`
by itself would not produce a working cross compile: `codegen/entry.rs`
uses `cranelift_native::builder()` (host ISA only), 41 sites hardcode
`CallConv::SystemV`, `lib.rs:652` shells out to GNU `ld -static`, and
`embed_sash_source` writes an ELF section. And Apple does not support
static linking at all, which collides head-on with byte-identical
static-ELF output. Scope is DG's call.

`notes/tasks/cross-compile-and-setjmp-on-mac.md`.

## Not a spec gap

`codegen/entry.rs::panic_payload_message` has 4 cold branches and needs
no spec: three unit tests (`payload_string_is_recovered`,
`payload_static_str_is_recovered`, `payload_other_type_falls_back`)
already cover it, and `mk cov` deliberately excludes unit tests. This
is the "modulo unit tested regression guards" exception, and it is
worth naming so it does not get re-probed every round.
