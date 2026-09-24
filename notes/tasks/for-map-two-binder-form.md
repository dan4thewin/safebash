---
title: "for over a map: should `for k v in m` mirror `attest k v in m`?"
status: open
priority: P3
kind: decision
depends: []
gates:
  - "DG decides between the zsh-style generic N-binder `for` and the attest-style map-aware two-binder form, or rejects both"
  - "if rejected, the parser names the cause instead of `SB4014 expected a { } body or do`"
  - "samples written both ways are compared on ergonomics and implementation cost before deciding"
---

## The finding

DG, 2026-09-23: "we support `k v in m` for attest, does it work for a for
loop?"

It does not. Established by execution against `target/release/sash`:

| binder shape | `attest` | `for` |
|---|---|---|
| scalar | `attest s [[…]]` ✓ | n/a |
| array element | `attest e in a [[…]]` ✓ | `for e in @a` ✓ (splat, not bare name) |
| map key+value | `attest k v in m [[…]]` ✓ | **rejected** |

```
$ for k v in m { echo $k=$v; }
error [SB4014] expected a `{ }` body or `do`
```

Same for `for k, v in m` and for `for i j in a` over an array.

Map key+value iteration today is the two-concept idiom, which works:

```sash
map cfg=([host]=example.com [port]=8080)
for k in $(keys cfg) {
  echo "$k=${cfg[$k]}"
}
```

## Two facts that bound the decision

**The syntax space is free.** Bash rejects `for k v in a b` outright
(`syntax error near unexpected token 'v'`), so adopting the form diverges from
nothing. Verified with the local bash.

**The array asymmetry is forced; this one is not.** `for e in a` cannot mean
"iterate array `a`", because bash gives that a real meaning — the literal word
`a`. sash correctly reports SB5050 (`a` assigned but never read) there, and
that is why `for` needs `@a` while `attest` can take a bare name in a binding
clause. But `for k v in m` has **no** bash meaning to collide with, so the
bare-name spelling is available here in a way it is not for arrays.

## Precedents (DG asked for these)

**Closest: zsh, and it is the same spelling.** zsh's `for` accepts multiple
binders and consumes the word list in groups of N:

```zsh
typeset -A m=(A x B y)
for k v in "${(@kv)m}"; do echo "$k=$v"; done
```

Note *what* zsh generalises: the mechanism is "N binders consume N words per
iteration", not "iterate a map". The map-ness comes from the `(@kv)` flag
flattening the map into `k v k v`. Not verified locally (no zsh installed);
taken from zsh documentation and corroborated references.

**Everyone else uses a separator or destructuring, not juxtaposition:**

| language | form |
|---|---|
| Go | `for k, v := range m` |
| Python | `for k, v in d.items():` |
| Rust | `for (k, v) in &map` |
| Lua | `for k, v in pairs(t)` |
| Ruby | `hash.each { |k, v| }` |
| PHP | `foreach ($a as $k => $v)` |
| JavaScript | `for (const [k, v] of Object.entries(o))` |
| Perl | `while (my ($k, $v) = each %h)` |
| Janet | `(eachp [k v] tbl …)` |

So space-separated juxtaposition is a **shell-family** idiom (zsh), and it is
already sash's own idiom via `attest k v in m`. Within sash, consistency
argues for juxtaposition; outside the shell family, almost nothing else does.

## The two designs, which are not the same feature

**Design 1 — zsh-style, generic.** `for a b in <list>` consumes the list N at
a time, for any list. Map iteration only falls out if there is also a
kv-flattening splat to feed it. More general, more machinery, and it invents a
second splat flavour.

**Design 2 — attest-style, map-aware.** `for k v in m` is specifically the map
form, taking a bare map name exactly as `attest k v in m` does. Narrower, and
it is the one that actually closes the asymmetry DG noticed.

These have different costs and different blast radii. Deciding "add it"
without picking one is not a decision.

## Samples both ways

**Today** (works, specced):

```sash
map cfg=([host]=example.com [port]=8080)
attest k v in cfg [[ -n $k && -n $v ]]    # two binders, bare map name
for k in $(keys cfg) {                     # one binder, keys splat
  echo "$k=${cfg[$k]}"
}
```

**Design 2** (proposed):

```sash
map cfg=([host]=example.com [port]=8080)
attest k v in cfg [[ -n $k && -n $v ]]
for k v in cfg {
  echo "$k=$v"
}
```

The juxtaposition makes the case: the two lines would read the same way, and
today they do not. Against that, the existing idiom is two constructs the
reader already needs for other reasons (`keys`, dynamic subscript), so this
buys ergonomics rather than expressiveness.

## Cost if adopted (Design 2)

Not a parser-only change. Touches: `parse/parser.rs` (the `for` header),
`ir.rs` (a second binder on `For`), `validate.rs`, taint (the value binder
inherits the map's taint — `attest k v in m` is the precedent),
`type_check.rs`, `transform/control_flow.rs`, `codegen`, `--emit-c` parity,
`sash-convert`, and specs in both modes.

The transform side is cheaper than it looks: `lower_for` is **cc 5 / cog 4**
today and `lower_for_static` is 4 / 4 (measured 2026-09-23). The complexity
in that file sits elsewhere — `redirect_merge_false_branches` 10/15,
`insert_case_phis` 10/8, `detect_array_iter` 9/5. `detect_array_iter` is the
one a map form would actually have to grow, and at cog 5 it has room before
the 3×-mean bar.

(Do **not** trust AGENTS.md's "Cyclomatic residuals" table here — it still
lists `lower_for` at 35 and `lower_case` at 40, which predate the parser
migration. See the separate note below.)

## If rejected: the diagnostic is still wrong

`SB4014 expected a \`{ }\` body or \`do\`` names a consequence, not the cause.
A user who has just learned `attest k v in m` will try `for k v in m` and be
pointed at the wrong token. Even under "leave the language alone", the parser
should say something like "a `for` loop binds one name; iterate a map with
`for k in $(keys m)`". That also covers `for i j in a` and `for k, v in m`.

This is the cheap, contained half of the question and does not depend on the
language decision.

## Status

DG chose **leave it, file a task** (2026-09-23), with: "consider other
languages, established precedents, write samples both ways to judge
ergonomics/cost." That research is above; the decision itself is open.

## Incidental finding: AGENTS.md's residual table is stale

While costing the transform work I checked AGENTS.md's "Cyclomatic residuals"
table, which claims three `sash/src/transform/` functions under-report against
Mozilla due to a tree-sitter grammar skew. Measured against the live index
2026-09-23:

| function | AGENTS.md says | actually |
|---|---|---|
| `transform/statements::lower_one` | 55 | **17** |
| `transform/control_flow::lower_case` | 40 | **2** |
| `transform/control_flow::lower_for` | 35 | **5** |

All three predate the parser migration (commit `1c0e64a` deleted `lower/` and
the CST). The table's advice — "allow ~10 slop on big-match-on-CST-kind
functions in `sash/src/transform/`" — now points at functions that are no
longer big and at a CST that no longer exists. Anyone costing work in that
directory from AGENTS.md will over-estimate by 3–20×, which is exactly what
happened in the first draft of this task.

Filed here rather than acted on: AGENTS.md is DG's operating document, and
whether the skew *rationale* still applies anywhere (vs. the numbers simply
being stale) needs a re-measure against `rust-code-analysis-cli`, not a
find-and-replace.

**Resolved 2026-09-24.** DG asked for the correction, and the re-measure was
run: `rust-code-analysis-cli` (installed with `--locked`, which is required —
the plain install dies with E0308 on two incompatible `tree_sitter::Language`
types) compared per-function `cyclomatic.sum`/`cognitive.sum` against the index
across all of `sash/src` — 1901 functions in 45 files, 1620 agreeing exactly.

**All three named functions agree with Mozilla exactly** (`lower_one` 17/1,
`lower_case` 2/1, `lower_for` 5/4). The skew *rationale* did not survive
either: cyclomatic never over-reports in indexnudge (119 low, 0 high) and
cognitive is mostly low as well (158 low, 16 high), all within a few points.
AGENTS.md now records the direction and the re-measure command instead of a
table, and the stale-prone current-state numbers throughout the file were
dropped in the same commit.
