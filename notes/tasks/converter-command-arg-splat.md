---
title: sash-convert — command-arg $@ has no clean sash-native target
status: superseded
superseded-by: converter-positional-use-needs-bless
priority: P2
kind: feature
depends: []
gates:
  - top-level `cmd "$@"` → a converter error/MANUAL marker (not non-compiling output)
  - conv.x1 spec test asserts the loud-failure behaviour
---

**SUPERSEDED by [converter-positional-use-needs-bless](converter-positional-use-needs-bless.md)**
— the gap is broader than the `$@` command-arg splat: *every* top-level
positional that is read needs a bless the converter can't author (DD#60 +
DD#24). The original (narrower) write-up follows.

# sash-convert — command-arg $@ has no clean sash-native target

## Background

The argv-trio converter work (argv-trio-converter / DD#60) made
sash-convert rewrite top-level positionals context-awarely:
  - `for x in "$@"` → `for x in $(argv)`     (splat OK in a for-in header)
  - `echo "$1"`     → `echo "$(arg 1)"`       (scalar OK anywhere)
  - inside-function forms preserved (DD#46)

But a top-level `$@`/`$*` **splat in command-argument position** has no
clean sash-native equivalent — `$(argv)` is splat-context-only and
rejected (SB5025) as a command arg:

```bash
some_cmd "$@"      # converter currently emits:  some_cmd $(argv)
                   # which fails to compile:      SB5025 (splat in arg pos)
```

Per the original argv-trio-converter design (option 2): mechanical
conversion silently producing non-compiling sash-native is worse than a
converter error. Today the converter produces the non-compiling
`some_cmd $(argv)` (a loud SB5025 at *compile* time, not convert time).

## Wanted

Detect a `$@`/`$*` splat in command-arg position (vs for-in header /
array-RHS) during conversion and emit a `# MANUAL:` marker or a converter
error directing the user to snapshot+iterate (`array a=$(argv); bless x in
a …; cmd $a[0] …`) or pass an explicit list. Add `conv.x1` to
`spec/convert.sh`.

## Detection sketch

In `convert_simple_expansion` / `convert_expansion`, when rewriting a
`$@`/`$*` splat, inspect the ancestor chain: if the nearest meaningful
ancestor is a `command` (not a `for_statement` word list or array
assignment), emit the MANUAL marker instead of `$(argv)`.

## See also
- `notes/tasks/argv-trio-converter.md`
- `notes/decisions/DD60-argv-trio.md`
- `sash-convert/src/convert.rs` (convert_simple_expansion, splat_replace_node)
