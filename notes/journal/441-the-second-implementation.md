# 441 — The second implementation

*2026-09-28*

Three bugs this session, and the same sentence describes all three: a
compile-time count taken over a value that only exists at runtime. Two are
fixed. The third is DG's call, and it is the worst of them.

## The DD said it already

The session opened with me putting a fork to DG — patch the DD#84 elidability
denylist, or invert it to an allowlist? — and DG declining to answer it on my
terms. He said he thought elision applied only to external commands, and
perhaps array construction, and told me to go read the DD.

He was right about the principle and slightly off about the inventory, and the
inventory is what mattered. DD#84's "Elidable positions" is a closed list of
three:

> - an external / command argument list (`ls ${args[0]:~} rest`);
> - a `sash`-**varargs** builtin arg list (`echo ${x:~}`);
> - an array literal element (`array a=(x ${y:~} z)`).

The word doing the work is **varargs**. Every one of the eleven commands that
was ICEing is fixed-arity, so the DD already excluded them — no amendment, no
language decision, nothing for DG to decide. I had brought him a question that
a document in the repository answers in one sentence.

That is the over-asking failure mode, and it is instructive that it looked
exactly like diligence. I had the DD number, I had cited it in the task file,
and I had still not read the section that settles it. "Present options with
tradeoffs" is not a substitute for finding out whether there is a choice.

The loose phrase is in the DD's first bullet: "an external / **command**
argument list". Read at speed, any command has an argument list. That is the
reading the implementation had:

```rust
let elidable = !matches!(expr_literal_str(&c.name), "readline" | "read");
```

Allow everything, except the two that were reported. Each of those two had
been appended after it panicked codegen — the comment above the line says so —
and the same list had been patched twice before, once for arithmetic and once
for the `read` slot. Six of the thirteen SB5046 specs in the suite are
regressions for this one bug. A denylist whose failure mode is a compiler
panic will be patched forever, once per report.

It is now an allowlist over `BuiltinCmd`, total, no wildcard. The
discriminator falls out of the DD: a name `from_name` does not recognise is an
external program or a user function call, and both are bullet one. A builtin
added tomorrow is non-elidable until someone writes an arm for it, and the
totality gate will not let them skip that.

## exec had its own everything

One of the eleven was `exec`, and it did not belong with the others. `exec
CMD ARGS…` builds an external command's argv, which is bullet one as plainly
as anything is. Rejecting it would have made the compiler non-conformant with
the DD to make a bug go away.

So I went to look at why it panicked, and found `build_argv_on_stack` in
`codegen/dispatch.rs` — a second argv builder, sitting fifty lines from
`finalize_exec`, doing by hand what `commands.rs::build_argv_values` had been
doing correctly for months. It lowered every argument as a scalar, so `@a` and
`${x:~}` reached the panic that exists to say "a list builder should have
handled this".

Then I diffed it against `build_argv_fixed`, the function it was a copy of,
and found the line it was missing:

```rust
emit_argv_guard(builder, module, externs, ptr, len);
```

DD#99 stage 7. An argv entry crosses `execve` as a C string, so a value
carrying an interior NUL is truncated there — silently, with a zero exit, and
the callee receives a *different, shorter* argument than the program computed.

```
exec /usr/bin/printf "[%s]" $s     # s = "abc\0HIDDEN"  →  [abc]   rc=0
     /usr/bin/printf "[%s]" $s     # same value         →  aborts
```

A program that validates a string and then execs with it was handing the
callee something it had not checked. That is a validation bypass, and it was
the quietest of the three faults: no panic, no diagnostic, correct-looking
output, exit 0.

The c backend had the same duplication — `emit_exec_builtin` hand-rolled its
own array literal — and the same two ICEs, but it *did* emit the guard. So the
differential oracle would have caught the truncation the moment a spec existed
to run through it. None did. `exec` had no splat coverage, no elide coverage,
and no NUL coverage at all; the fifteen specs it had were all redirect forms.

The doc comment on the c side is the part worth keeping:

> Native's `build_argv_on_stack` lowers every element the same way, and
> neither backend expands an `@a` splat here.

Written as a parity note. The two backends agreed, and what they agreed on was
that neither had the feature. Cross-backend agreement is only evidence when
the two were built independently; here each was a copy of the same omission,
so the oracle was confirming a shared blind spot rather than testing one. The
one place they *did* differ — the NUL guard — is exactly where a spec would
have paid, and that is the argument for writing specs against a thing you
believe is symmetric.

Both now go through the shared builders. External-command output is
byte-identical.

## The third one is still open

While proving that user-function call sites were safe to keep elidable — the
DD's §(A) is entirely about optional trailing args to functions, so they had
to be — I checked what eliding does to `argc`. It does exactly what it says:

```sash
fn f {
  str a=${args[0]}
  str b=${args[1]}
  int n=$(len args)
  echo "argc=$n a=[$a] b=[$b]"
}
str x=""
f one ${x:~}
```

→ `argc=1 a=[one] b=[]`, compiled without complaint. And:

```
f one
→ error [SB5042] function 'f' expects at least 2 arguments but was called with 1
```

The compiler rejects the literal spelling of the state it just allowed the
program to reach. `check_callee_arity` is handed `arg_values.len()`, and an
`ElideEmpty` is one SsaValue that becomes zero tokens.

The mirror image is there too. `f @q` with a two-element `q` into a
`Fixed{2,2}` callee is *rejected*, with `expects 2 arguments but was called
with 1` — a count that is true of no execution of that program.

Same sentence as the other two: a compile-time count over a value that expands
at runtime. Third time today.

The fix is a range check — compute the runtime `[lo,hi]` of the argument
count and diagnose only when it is disjoint from the callee's `[min,max]`.
That fixes the false rejection outright. What it does not decide is what
happens when the ranges overlap and the runtime lands outside, which is
direction A above. The tempting fail-closed answer — reject a dynamic count
into a fixed-arity callee — kills DD#84's own motivating case, since "pass a
second argument if I have one" against a `Fixed{1,2}` callee is precisely a
dynamic count that is always in range.

So it is a genuine fork, it changes generated code under one option, and it
decides whether a compile-time arity guarantee survives contact with dynamic
argument counts. Filed as `bug-call-arity-counts-compile-time-values.md`,
blocked on DG, with a lean toward emitting a runtime check.

## What to take from it

A second implementation of something is not redundancy. It is a place where
one of the two will be improved and the other will not, and nothing will
report the divergence — the argv guard was added to `build_argv_fixed` and
simply never reached the copy fifty lines away. I collapsed four copies of the
`TestExpr` recursion two commits ago for tidiness. This time the duplicate was
carrying a silent truncation.

And: when a filing says "blocked on DG, this is a language question", read the
DD before writing that sentence. Twice now the answer has been in the document
I was citing.

---

Sweep: 6809 rows, 6796 passing, 13 skipped, zero failing, **zero flips**.
Specs `ex.16`-`ex.22` (exec argv) and `84.37`-`84.52` (elidable positions —
including the first coverage the *accept* side has ever had).
