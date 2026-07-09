---
title: sash-convert — top-level positional USES need a bless the converter can't author
status: ready
priority: P2
kind: feature
depends: []
supersedes: converter-command-arg-splat
gates:
  - converter output for a used top-level positional either compiles or carries a MANUAL marker
  - conv.x* spec tests assert the chosen behaviour (no silently non-compiling output)
---

# sash-convert — top-level positional uses need bless

## Resolved (DG, 2026-07-08) — option 1 (hoist + placeholder bless + MANUAL)

For each top-level positional **read**, sash-convert emits the env-var-style
hoist (the `$HOME` model already in `convert_simple_expansion`): a prefix
line that captures the source, a **placeholder bless** with a permissive
predicate, and a `# MANUAL(sash-convert):` marker telling the user to
tighten it. The inline use is replaced with the hoisted var.

- **scalar read** `echo "$1"` →
  ```bash
  arg1=$(arg 1); bless arg1 [[ -n $arg1 ]]   # MANUAL(sash-convert): tighten predicate
  echo "$arg1"
  ```
- **for-in** `for x in "$@"; do …` → after the syntactic rewrite to
  `for x in $(argv); do …`, inject a body-leading
  `bless x [[ -n $x ]]  # MANUAL(sash-convert): tighten predicate`.
- **command-arg splat** `cmd "$@"` → snapshot to an array, bless-all, and
  pass: `argv a; bless a in e [[ -n $e ]]  # MANUAL(sash-convert): …; cmd @a`
  (once `argv-snapshot-to-outparam` lands; until then a `MANUAL` marker).

The placeholder predicate (`[[ -n $x ]]`) is permissive-but-compiling so the
converted script builds; the `MANUAL` marker forces human review. This
beats option 2 (MANUAL-only, no working scaffold) and option 3 (status quo,
non-compiling output). Convert-tier work.


## Re-scoped (2026-06-16)

Originally filed as `converter-command-arg-splat` (just `cmd "$@"`). On
review the gap is **broader**: under DD#60 + DD#24, *every* top-level
positional that is **read** becomes a tainted source requiring a `bless`,
and the converter cannot author the bless predicate (it doesn't know the
user's validation intent). So the converter's syntactic rewrites produce
**non-compiling** sash-native for almost every positional *use*:

| bash (top level) | converter emits | compiles? |
|---|---|---|
| `echo "$1"`      | `echo "$(arg 1)"`              | ✗ SB5020 (tainted use, no bless) |
| `echo "$@"`      | `echo $(argv)`                | ✗ SB5025 (splat in arg position) |
| `for x in "$@"; do echo "$x"; done` | `for x in $(argv); do echo "$x"; done` | ✗ SB5020 (body reads tainted x) |
| `echo "$#"`      | `echo "$(($(argc) - 1))"`     | ✓ ($(argc) is clean) |
| `x=$1` (x unread)| `x=$(arg 1)`                  | ✓ (assignment-from-source) |

So only `$#` and bare un-read assignments survive. Anything that *reads* a
positional needs a bless statement the converter can't write.

## Precedent: the converter already does this for env vars

`convert_simple_expansion` hoists `$HOME` into a prefix line:

```bash
home=$(getenv HOME); bless home [[ $home =~ <known pattern> ]]
# … and replaces the inline $HOME with $home
```

using a pattern from `known_externals`. The same hoist applies to
positionals — there's just no known predicate, so it must be a placeholder
plus a MANUAL marker.

## Wanted

For each top-level positional **use**, choose (DG to decide):

1. **Hoist + placeholder bless + MANUAL** (env-var style):
   `echo "$1"` →
   ```bash
   arg1=$(arg 1); bless arg1 [[ -n $arg1 ]]   # MANUAL(sash-convert): tighten predicate
   echo "$arg1"
   ```
   for-in: inject a body `bless x [[ -n $x ]]  # MANUAL` after the rewrite.
   command-arg splat `cmd "$@"`: snapshot + bless-all + pass — or a MANUAL
   marker (no single-line form).
2. **MANUAL marker only** — rewrite the syntax, mark every use as needing a
   hand-written bless, don't fabricate a predicate.
3. **Status quo** — emit the rewrite and let it fail loudly at compile
   (SB5020/SB5025). Worst per the argv-trio-converter design note
   ("silently producing non-compiling sash-native is worse than an error")
   — though it does fail loudly, just one step late.

## Detection

`has_ancestor_kind` already gives function-context. For "is this a read vs
a bare assignment-RHS", inspect whether the positional's expansion sits
inside a `command` arg / `for` word / test vs. the RHS of a top-level
`variable_assignment` whose target is never later read (the last is hard;
a conservative MANUAL marker on all reads is simpler and safe).

## See also
- `notes/decisions/DD60-argv-trio.md`, DD#24
- `sash-convert/src/convert.rs` (convert_simple_expansion, convert_expansion,
  splat_replace_node; the `$HOME` hoist is the model)
- spec/argv.sh av.s5/av.n5 (the taint-on-use behaviour the converter must respect)
