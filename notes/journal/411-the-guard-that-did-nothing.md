# 411 — The guard that did nothing

DG asked a one-line question that turned out to be load-bearing:

> SB4030 happens when we try to assign a string to an int; why do we have an
> implicit atoi for return? let's find all the implicit atoi conversions.

The question is sharp because the compiler states the rule in its own
diagnostic. `conversion_hint` (`type_check.rs:1859`) renders:

> a string→int conversion goes through arithmetic and must carry a
> disposition: `$(( ${VAR:-0} ))` (0 if it isn't an integer) or
> `$(( ${VAR:?msg} ))` (abort if it isn't)

So why does `return $s` quietly answer 0?

## Why `return` was different

`return` is not a statement with a value. It lowers to an ordinary assignment
to `$?` (`transform/stmt_handlers.rs::handle_return_stmt`), and native codegen
handled that target in `handle_exit_status_assign`:

```rust
let int_val = match value {
    SsaValue::Literal(s) => { … s.parse::<i64>().unwrap_or(0) … }
    _ => {
        let (ptr, len) = lower_value(…);   // as a STRING
        atoi_call(…)                       // then lenient sash_atoi
    }
};
```

`exit` never went near this. `emit_exit` (DD#89) uses `lower_value_as_int`,
which is **type-directed**: it routes a DD#77 disposition to `sash_atoi_or` /
`sash_atoi_or_abort` before anything else. `handle_exit_status_assign` asked no
questions — it flattened whatever it was given to bytes and ran the *lenient*
`sash_atoi`, the one documented (DD#72) as returning 0 for non-numeric input.

## The consequence, which is worse than the coercion

The interesting casualty is not `return $s`. It is the author who did
everything right:

```sh
f() {
  str s=abc
  return ${s:?not an int}
}
```

`${s:?…}` is the spelling the compiler *recommends*. In word context its
trigger is unset-or-empty (DD#65), and `s` is set and non-empty — so string
lowering produced `"abc"`, and the lenient atoi turned that into **0**. The
function returned success. **An explicit "abort if this isn't an integer"
guard compiled to nothing.**

`${s:-7}` failed the same way, yielding 0 instead of 7.

## The differential oracle earned its keep

The c backend was right all along. `emit_c.rs::emit_assign` handles the `?`
target with `lower_value_as_int` — and its comment says it "mirrors
handle_exit_status_assign", which was exactly backwards. It mirrored what that
function *should* have done.

| input (`str s=abc`) | native | c |
|---|---|---|
| `return ${s:-7}` | **0** | 7 |
| `return ${s:?bad}` | **0** | abort, rc 1 |

Two backends, two answers, and **no spec covered it** — which is why a green
sweep never noticed. This is the case `--emit-c` exists for, and it localised
the bug to one backend without any reasoning about which was correct.

The fix is that `handle_exit_status_assign` now calls `lower_value_as_int`,
converging native onto `emit_exit`, onto the c backend, and onto the rule
SB4030 already prints. The function got *simpler* — cc 2/cog 1 → cc 1/cog 0.

## Proving it, and the spec that would have been vacuous

Neutering the native fix fails **74.57**, **74.58**, **74.61** and leaves
**74.59**, **74.60** green — the latter two use a *valid* integer string, so
they confirm the fix does not over-reach rather than discriminating for it.
Under the same neuter c mode stays 138/138, which is a second proof: the bug
was native-only.

74.61 nearly shipped vacuous. The first draft was:

```sh
f() { str s=abc; return ${s:-0}; }
if f; then echo yes; else echo no; fi
```

Both the broken and the fixed compiler print `yes` — lenient `atoi("abc")` is
0, and the disposition default is 0. **Same answer by two different routes.**
Changing the default to `1` splits them: fixed → `no`, broken → `yes`.

(The draft also read `$?` after an `if`, which is SB5012 — `$?` needs a
preceding *command*. That one at least failed loudly.)

## The inventory DG asked for

Every site where an undispositioned string silently becomes an int:

| site | sash | bash | accountable spelling? |
|---|---|---|---|
| `return $s` | 0 | rc 2 + error | yes — **was broken**, now fixed |
| `exit $s` | 0 | rc 2 + error | yes, works |
| `wait $s` | rc 0, silent | rc 1 + error | yes, works |
| `${a[$s]}` read | index 0 | index 0 | yes — `${a[$(($s:?))]}` |
| `a[$s]=` write | index 0 | index 0 | **none parses** |
| `[[ -t $s ]]` | fd 0 | false | parses, ignored |
| `$(arg $s)` | index 0 | n/a | parses |

Rejected today, for contrast: `int x=$s` (SB4030), `(( n = s ))` (SB4030),
`x=${a[0]}` from a string array (SB4030), `${t:1:$s}` (SB4030), `${t:$s:2}`
(SB4047), `${a[s]}` and `${a[i]}` bare names (SB4030), `${a[i+1]}` (SB4041).

Three of the seven diverge from bash: `return`, `exit`, `wait`.

## DD#89's premise does not describe the implementation

DD#89 blesses the coercion in one clause:

> A string argument coerces to int (atoi), **as in every other int context**;
> `exit` imposes no extra type restriction.

The italicised justification is a consistency claim, and the table above shows
it is false: nearly every *other* int context rejects. The clause was written
while settling a **range** question (removing the `exit $intvar` reject); the
type remark rode along without being measured against the rest of the language.

This is worth recording as a pattern rather than a one-off: a DD that argues
"consistent with X" is asserting a fact about the implementation, and that fact
ages exactly like a filed task does. Nothing re-checks it.

## Where DG took it

Told that `a[$s]=Q` has no accountable spelling, DG went looking and found one
for the read side himself — `${a[$(($s:?))]}` — which does work (aborts on
`abc`, indexes correctly on `1`). He would prefer terser forms, `$a[$(($s:?))]`
or `$a[$((s:?))]`; measured, the first is SB4020 (not a sash spelling) and the
second is **SB4041 "ambiguous braced index"** — the ambiguity he suspected is
one the compiler already diagnoses by name.

His steer on the broader question:

> Rather than prioritize bash consistency, I want to prioritize sash
> self-consistency.

That settles the *direction* of any tightening — SB4030's stated rule is the
self-consistent one, and matching bash at `${a[$s]}` / `[[ -t $s ]]` is not a
reason to keep a hole. The scope and the DD are still open; DG asked to discuss
before anything is tightened.

## Filed

`notes/tasks/undispositioned-string-to-int-sites.md` — the six remaining sites,
the missing subscript-LHS spelling, and the DD#89 amendment.

## Gates

Sweep **6386** (predicted exactly: +5 specs × 2 modes on 6376), 6373 passing,
13 skipped, zero failing, **zero flips** vs `4578aed`. `mk test` 106 passed.
fmt / clippy / complexity clean.
