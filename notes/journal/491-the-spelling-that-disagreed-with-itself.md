# 491 — the spelling that disagreed with itself

DG settled the int→string question from 490 and set three pieces of work. Two
were the ones asked for. The third was a bug, found by a spec written for the
first two.

## The ruling

> the aim is that int to string conversions must be unambiguously denoted, so
> `str s="$x"`, the double quotes are the expression of intent. `echo` also
> unambiguously denotes writing a string. In terms of implementation, I'd
> expect a bare `echo $x` where x is an int to essentially compile the same as
> `echo "$x"`; likewise for `$(wait)` — so I wouldn't expect the
> implementation of `wait` itself to be context-aware, I'd expect it to always
> return an int.

and then, having seen the duplicate-table filing:

> yes, do collapse now, and fix epochseconds - it should return an int in the
> runtime implementation. All the int intrinsics should need an explicit
> double-quote wrap to assign to string var.

Also answered along the way: `str s=$(wait)` is **SB4030**, and the diagnostic
already states the rule in its own words — *"quote it to convert int to
string: `"$VAR"`"*. `spec/errors.sh` `csc.14` had pinned it since the DD#126
work. Nothing was owed there.

## The denotation rule needed no code

I probed all ten `ArithDenotation::Int` intrinsics in string position before
writing anything. Unquoted → SB4030, quoted → ok, **uniformly**: `argc`,
`epochseconds`, `random`, `getpid`, `getppid`, `signum`, `status`, `getcpid`,
`waitstatus`, `wait`. `getcwd` is the single exception and is genuinely
string-valued.

Three of the ten *looked* like anomalies at first — `status` reported SB5012,
`waitstatus` SB5094, a bare `wait` statement SB4016. None is a type rule.
They are **availability** gates that fire before the type check, and with a
correct prelude all three fall into line. Worth recording because the shape
recurs: an earlier gate's diagnostic is not evidence about the later gate's
behaviour, it is evidence that you never reached it.

The first probe run was also simply wrong. I tested for compile success by
asking whether stderr was empty — but a successful `sash` run prints
`sash: x.sh -> x (static binary)` to stderr, so every case read as a failure.
Exit codes, not stream emptiness.

So the whole of DG's third item was already true. It is now **pinned**, by
`spec/types.sh` `den.1`–`den.22`: reject-unquoted and accept-quoted for each
of the ten, `getcwd` as the documented exception, and two regression specs for
the `epochseconds` ABI below.

## `epochseconds` was the one that wasn't

`sash_epochseconds()` returned `(ptr, len)`. It formatted `tv_sec` into a
stack buffer and called `alloc_cstr` — and then `lower_cmdsubst_as_int`'s
fallback ran `atoi` over the result to get the integer back. It is
`ArithDenotation::Int` on the surface, so the string was never the value, only
a transport. Now `-> i64`, like every sibling.

That mattered for the collapse: `epochseconds` was one of the two names the
filed task listed as genuinely string-valued and therefore exempt. Fixing the
runtime removed the exemption, leaving `getcwd` alone.

## The collapse

`try_nullary_intrinsic` now keeps exactly one arm — `getcwd` — and otherwise
delegates to `int_intrinsic_i64` + `emit_int_to_str`. Six names (`getpid`,
`getppid`, `waitstatus`, `random`, `argc`, `signum`) had their value
production written twice in the same file; the copies agreed, which is why
this was a hazard rather than a live bug, and exactly the shape that produced
the phantom `kill` builtin (489) and the `emit_exec_static_argv` fork (441).

The filing asked me to check `status` first and *not to assume*. The answer
was better than "the transform intercepts it": `nullary_stateful_slot`
rewrites a bare `$(status)` to `Var("?")` and `wrong_arity_backstop` rejects
`$(status …)`, so the name cannot reach the table at all. I plumbed
`exit_status_var` through anyway rather than excluding it — that makes the
delegation **total** over what `int_intrinsic_i64` knows, instead of leaving a
latent fall-through to the external-exec path if that interception ever moves.

The C backend never had the split. One table for all eight names, throughout.

## The bug: `(strlen <bare word>)`

Writing `den.4` I wanted to assert that `str s="$(epochseconds)"` produced ten
characters. I reached for `(strlen s)` and got **1**. Not an off-by-one — the
length of the *name*.

That alone is correct and specified: DD#102 made `strlen` take a **value**, so
`(strlen s)` measures the one-character string `"s"`, and `75.60` pins it. The
finding was next to it:

```
$(strlen nosuch)       -> 6
$(( (strlen nosuch) )) -> SB5031  "'$nosuch' is assigned nowhere"
```

Two spellings of one intrinsic disagreeing about what their operand *is*.

DD#102 settles it, in terms that leave no fork to take to DG: *"SB5039 and
SB5062 no longer fire for `strlen`. An unknown bare word is a string literal,
not a missing variable, so there is nothing to report."* codegen implements
that; `validate` did not. `check_arith_reads`'s `FuncCall` arm walked **every**
argument as a read regardless of the callee's operand discipline — the
pre-DD#102 model in which every int intrinsic resolved a name.

**Third site with the same fault.** `is_scalar_operand_intrinsic`'s own doc
comment records SB5070 having had it, repaired 2026-09-17. When a DD moves a
builtin between two families, every pass that asks "is this operand a name?"
is a site, and there is no list of them.

### Both directions, because the sink is shared

`ReadSink` feeds SB5031 (`Flag`) and SB5050 (`Collect`) from one walk, so the
phantom read failed open in two ways at once:

- **false positive** — SB5031 on a string literal, above;
- **false negative** — `str abcd=xy; echo $(( (strlen abcd) ))` compiled,
  because the phantom read kept a dead variable alive. The word form reports
  SB5050 correctly.

The second is the one that would never have been reported. It has no symptom.

### The fix did not add a third copy

`validate::arith_intrinsic_operand_ok` was *already* a hand-kept copy of the
transform's `intrinsic_operand_kind`, `Paren` recursion and all. So rather
than mirror the match a third time, `intrinsic_operand_kind` became
`pub(crate)` and validate now consumes it for both the arity check and the new
read check — following the `fold_arith_op` precedent, which validate already
imports from transform. Only an operand the transform will genuinely rebuild
as a `Literal` is skipped.

### What the eight failures were

After the fix `types.sh` went from 2 failures to 8 — and every one was a spec
**I had written that morning**, against the bug. Six used `(strlen s)` to
check "did this produce anything", which passed only because the phantom read
was suppressing SB5050 on the variable they had just assigned. They are
`(strlen $s)` now.

Worth noticing that the *existing* `75.60` carries a seemingly redundant
`echo "$s" > /dev/null`. It was redundant under the bug. It is load-bearing
now, and it is the only reason that spec did not join the eight.

## Gates

Full sweep **8149**, predicted 8149 (8097 + 26 specs × 2 modes), zero failing,
zero flips, exactly 52 new ids, machinery-capable check 248. `mk test` green,
both id-collision checks empty.
