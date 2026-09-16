---
status: open
priority: P2
---

# The codegen positional pre-scan cannot insert anything

`codegen/setup.rs::collect_positional_refs` walks **every value of every
instruction of every block** on every compile to build a set of
"positional / special" var names. The filter that would put a name in
that set can never pass.

## Evidence

`values.rs::maybe_insert_positional` is the only filter:

```rust
fn maybe_insert_positional(name: &str, refs: &mut HashSet<String>) {
    if name.parse::<u64>().is_ok() || name == "#" || name == "@" || name == "0" {
        refs.insert(name.to_string());
    }
}
```

Over the full sweep it is called **89 times and inserts 0 times** — all
four alternatives are false every time (`values.rs:1515` has line count
0; all four TRUE branches are cold).

Meanwhile `init_positional_params`'s loop body *does* run — 10 times —
because a **second, unfiltered** producer feeds the same set:
`redirects.rs::scan_redirect_for_vars` inserts every `SimpleExpansion`
name it finds in a redirect target, with no filter at all. That is the
live path (`str f=out; /bin/echo hi > $f`), and its real purpose is
DD#53 (an undeclared name referenced as `$NAME` initialises to empty).

So the set has two producers with different contracts, and only the
unfiltered one contributes.

## Why the filter cannot pass

`init_one_positional`'s own doc comment (setup.rs:1456) already says so:

> The numeric (`$1`), `$#`, and `$@` init strategies were retired:
> top-level positionals are validation-rejected (SB5024/SB5045) and
> function positionals resolve through the `args` array (DD#60), so
> those forms never reach here — only special names that fall to the
> empty default do.

Probed against the live binary, every spelling the filter tests for is
rejected before codegen:

| input | result |
|---|---|
| `/bin/echo $0` | SB5017 |
| `/bin/echo $#` | SB5024 |
| `/bin/echo $1` | SB5024 |
| `/bin/echo $@` | SB5045 |
| `fn f { /bin/echo $1 }` | compiles — resolves via `args` (DD#60) |
| `fn f { /bin/echo $# }` | compiles — resolves via `args` |
| `fn f { /bin/echo $@ }` | SB5045 |

## What is dead

Reachable only to feed the filter, so provably unable to affect output
(46 lines in `values.rs`):

- `maybe_insert_positional` (4 cold branches)
- `scan_value_for_vars` (1 cold branch)
- `scan_composite_children`
- `scan_test_for_vars`

and consequently most of `scan_inst_for_vars` / `scan_command_args`
(setup.rs 1414–1454) — only the `for r in redirects` half matters, so
both collapse to a redirect walk, and `init_positional_params`'s
3 cold branches go with them.

## Why this is filed rather than done

**The retirement was deliberate.** The doc comment documents it
explicitly, which means someone retired the positional strategies and
*left the scanner standing*. Under "always ask before changing
something that might be deliberate", deleting it is DG's call, not
mine — the two readings (knowingly kept as a safety net vs. simply not
noticed) lead to opposite actions.

It also forces a naming decision: with the positional half gone,
`collect_positional_refs` / `init_positional_params` /
`init_one_positional` are misnamed — they would be collecting redirect
target names and giving them the DD#53 empty default.

## Options

1. **Delete** the filter subtree; collapse the walk to redirects only;
   rename the survivors to say what they do (e.g.
   `collect_redirect_target_vars` / `init_undeclared_redirect_vars`).
   Removes 8 cold branches and a full-program walk per compile.
2. **Keep the walk, assert the invariant** — replace the filter body
   with `unreachable!()` naming SB5024/SB5045/DD#60, matching the
   `join_intrinsic` convention. Documents the invariant but keeps
   paying for the walk.
3. **Leave it.**

Recommend 1: the walk is not a safety net — it cannot fire, so it
cannot catch a regression either. If the intent is a net, option 2 is
the honest spelling of it.

## Gate

Redirect-to-variable is the live path and is specced; any change must
keep the sweep green. Found in the branch-coverage loop, journal 338
follow-up.
