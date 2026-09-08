---
title: DD#96 — sed edits chain with `,`
status: accepted
---

Settled with DG (2026-09-08), from: *"I like the sed-like edits, e.g. `$foo =~
s/a/b/`; I want to be able to chain them, e.g. `$foo =~ s/a/b/ s/c/d/`; or with
some other syntax (commas/semicolons/brackets). What seems consistent and
logical?"* — answered **"yes to comma, yes to both forms, only `s///`"**.

## The rule

A **comma** separates edits, in both spellings:

```sh
v =~ s/a/1/, s/b/2/          # statement form
w = ${v:s/a/1/,s/b/2/}       # expansion form
```

Edits apply **left to right**, each to the result of the last — `v =~ s/a/b/,
s/b/c/` on `a` yields `c`, not `b`. Per-edit flags are per-edit: in
`s/a/1/g, s/b/2/` only the first is global.

A chain continues while an edit is *owed* — i.e. while the text so far ends
with `,` — so it may cross whitespace **and a line break**:

```sh
v =~ s/one/1/,
     s/two/2/,
     s/three/3/
```

This is deliberately the same pending-separator continuation rule DD#94 gave
`(( ))`: a separator continues while an operand is owed; a complete item ends
at the newline. A trailing comma with nothing after it is an error, not a no-op.

## Why comma

`,` is already sash's sequencing separator — array literals, argument lists,
and (since DD#94) `(( a=1, b=2 ))`. Reusing it costs no new syntax and no new
rule to learn. The alternatives all had a specific defect:

| candidate | defect |
|---|---|
| juxtaposition `s/a/b/ s/c/d/` | invisible separator; a typo'd `/` silently makes it one long edit |
| `;` | reads as *statement* end everywhere else in sash |
| `\|` | reads as pipe |
| brackets `[s/a/b/ s/c/d/]` | new bracket form for one feature |
| sed's own `;` | matches sed, but see `;` above — sash is not sed |

## Only `s///`

`,` chains `s///` and nothing else. The other modifiers (`:1:3`, `:-`, `:?`,
`:~`, `#`/`%`, `^`/`,`) are unaffected and do not chain. DG's grounds were
practical: substitution is the one edit people actually want to repeat, and a
general "chain any modifier" rule would have to settle what `${x:1:3,:-y}`
means before it could ship. Left open, not foreclosed.

## The bug this exposed

Writing the chain surfaced a **silent-drop**: `split_operator` handed the whole
tail of an edit to the flags slot without validating it, so anything after the
closing `/` was swallowed *as flags*:

```sh
w = ${v:s/a/1/,s/b/2/g}   # before: second edit dropped — AND the FIRST
                          #         became a gsub, because the tail
                          #         contained a `g`
```

Wrong output, no diagnostic. `split_sed_edit` now returns the unconsumed rest
explicitly; the chain folder consumes it, and a rest that is not a `,`-led edit
is SB4032 rather than flag soup. This is the same shape as the ICEs found
earlier in the week (journal 252): **a path that silently accepted junk because
no spec pinned the rejection.**

## Not in scope

- Other delimiters. `/` remains the only one (`s|a|b|` → SB4032); `:` is taken
  by substring.
- Address/range prefixes (`1,3s/a/b/`). A leading `,` is a separator, full stop.
- Chaining across the two forms, e.g. `v =~ s/a/b/, ${w:s/c/d/}`.

## Specs

`spec/var_modifiers.sh` 96.1–96.11: both forms, order of application, per-edit
flags, the multi-line chain, the trailing-comma reject, and the tail-junk
reject that used to be swallowed.
