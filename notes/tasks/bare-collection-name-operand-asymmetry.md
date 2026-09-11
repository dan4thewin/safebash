---
title: "asymmetry: bare collection-name operands to builtins vs user-defined functions"
status: closed
priority: P2
kind: decision
depends: []
gates:
  - "DG decides whether a bare collection name to a user function should splat/pass-by-reference like it does to len/join/keys, or stay a literal string requiring @splat — and whether the builtin bare-name form should be kept, deprecated, or unified"
---

# Bare collection-name operands: builtins vs user functions

Surfaced during DD#86 Phase 4 (dead-variable rejection), when deciding what
counts as a *read* of a collection.

## The asymmetry

A **builtin** value function takes a collection by **bare name**:

```sash
array a=(x y z)
echo $(len a)      # reads the array a → 3
echo $(join a ",") # reads the array a → x,y,z
keys m             # reads the map m
```

`len`/`join`/`keys` resolve a bare `a` to the collection `a` (and also accept
the explicit splat `@a`). If `a` is not a collection, `join a` → SB4030
*"`a` is not a collection … or splat a collection with `@a`"*. So bare-name
**is** a first-class collection reference here (`type_check::join_operand_name`,
`validate::len_operand_name` both map bare `Literal("a")` → collection `a`).

A **user-defined function** does the opposite — a bare `a` is the **literal
string `"a"`**, and you must splat to pass the elements:

```sash
fn show { … }
show a     # passes the literal string "a"
show @a    # passes the array a's elements
```

So the same surface (`f a`) means "read collection `a`" for a builtin but
"literal string" for a user function. A reader can't tell from the call site
which one applies without knowing whether `f` is a builtin.

(Historical note: DG recalled `join` once requiring the splat — the bare-name
form is the current behavior, but the recollection is exactly this asymmetry
biting.)

## Why it matters

- **Readability / least surprise.** `len a` and `myfn a` look identical but bind
  `a` completely differently.
- **Refactoring hazard.** Promoting a builtin-like helper (`len`) to a user
  function silently changes `helper a` from a collection read to a string arg.
- **Analysis surface.** Every read-aware pass (SB5031 undeclared-read, SB5050
  dead-variable, taint) must special-case the builtin bare-name operands
  (`len`/`join`/`keys`) because they are reads the generic expansion-based
  enumerator does not see. Phase 4 added `collect_value_fn_operands` for exactly
  this; a unified rule would remove the special case.

## Options for DG

1. **Keep the asymmetry** (status quo): builtins take bare collection names;
   user functions require `@splat`. Document it loudly. Passes keep the
   `len`/`join`/`keys` special-case.
2. **Unify toward splat**: builtins also require `@a` (drop the bare-name
   collection operand). `len a` becomes `len @a`. Most consistent; churns every
   `len`/`join`/`keys` call site and many specs.
3. **Unify toward bare-name/by-reference**: a bare collection name to a user
   function passes the collection by reference (like the builtins). Powerful but
   introduces reference semantics sash has so far avoided.
4. **Restrict bare-name to a marked class**: only a designated set of
   collection-consuming builtins accept bare names; everything else splats.
   (Effectively option 1 with an explicit list — which is the de-facto state.)

Recommendation deferred to DG. The finite builtin set today is exactly
`len` / `join` / `keys` (`BuiltinCmd::Len`/`Join`/`Keys`, `ssa.rs`).

## Related

- DD#86 Phase 4 (`notes/tasks/dd86-scope-phases-2-5.md`, dead-variable
  rejection) — added `collect_value_fn_operands` to count these bare-name
  operands as reads.
- DD#60/#75 (`@args`, splat forms) — the splat surface these would unify with.

## Resolution (DG, 2026-09-11) — live with it; bare = string

> I'm going to just live with the current state that non-sigiled [names]
> provided to a user function are strings, since there is no expectation that
> they should operate on bare variables/collections, unlike `len`, `join`, et
> al. With use, I may reconsider or have an inspiration.

The gate asked DG to decide between pass-by-reference, literal-string, and
unifying/deprecating the builtin bare-name form. **None of the above:** the
asymmetry is kept, because the two callee kinds are genuinely different.

- `len` / `join` / `keys` are **language surface**. Their operand slot is
  *declared* to name a collection — that is what the builtin is for — so a
  bare name there can only sensibly be a name.
- A **user function** takes values. Nothing about it creates an expectation
  that a bare word resolves to a variable, any more than `f hello` should look
  for a variable named `hello`.

DD#100 supplied the missing half of this in the meantime: `=a` is now a
first-class spelling for "bind the collection", so the author who wants the
collection has an unambiguous way to say so, and the bare form no longer has
to carry two jobs.

Measured behaviour, unchanged and now pinned by `spec/collparam.sh`
100.35–100.37:

```sh
array a=(x y z)
show() { echo "arg1=${args[0]}"; }
show a               # arg1=a   -- the STRING
echo "len=$(len a)"  # len=3    -- the COLLECTION
show =a              # binds the collection to slot A
```

Explicitly provisional — DG left the door open ("with use, I may reconsider
or have an inspiration"). The specs pin the behaviour so it cannot drift
silently before that reconsideration; they are not a blessing of it. Recorded
as a clarification in `notes/decisions/DD100-collection-parameters.md`.

Note the related cleanup is a *separate* question and still open: removing
**splats** from builtin operand slots is tracked in
`dd75-len-collection-only-strlen`. DG has already ruled there that builtins
take bare names but not splats.
