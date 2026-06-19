---
title: "DD#67 sweep: re-add name() func sugar, the convert switch, review rejected bash forms"
status: ready
priority: P3
kind: feature
depends: [DD67-native-syntax-sugar-policy]
gates:
  - "name() { } accepted as 1-to-1 sugar for fn name { } (both compile to the same function IR)"
  - "a convert/canonicalize switch emits native-only syntax; the default preserves the user's form"
  - "each currently-rejected bash form is recorded keep (1-to-1 sugar) or drop, per the DD#67 test"
---

# DD#67 sweep — re-adds, the convert switch, and the rejected-form review

Implements DD#67. DG approved (2026-06-19): **re-add `name()`**, do a
**light sweep**, and provide the **convert switch**.

## 1. Re-add `name() { }` (approved)

`name() { body }` is a textbook 1-to-1 alias of `fn name { body }` — same
function IR, unambiguous. sash currently *rejects* it (only `fn name { }`).
Re-add it as accepted input; `fn` stays canonical.

- Spec: `f() { echo hi; }; f` → `hi`; `fn g { echo hi }; g` → `hi` (both work).
- Optionally ksh `function name { }` — same alias; include if trivial.

## 2. The convert / canonicalize switch (DD#67)

Default = **preserve** the user's form (`--emit-source` round-trips
`then/do`, `name()`, etc.). Add a switch that rewrites *all* sugar →
**native-only** (`then…fi`/`do…done` → `{ }`, `name()` → `fn`, …) — the
one-true-form for users who want it. Build on existing
`--emit-source`/`--annotate`/`sash-convert`. Confirm the flag spelling
with DG (e.g. `--emit-source --native`).

## 3. Sweep currently-rejected bash forms against the 1-to-1 test

For each, record **keep** (unambiguous 1-to-1 sugar) or **drop** (needs
translation / ambiguous), per DD#67. Candidates to check:

| form | native | likely |
|------|--------|--------|
| `name() { }`, `function name { }` | `fn name { }` | keep (§1) |
| `` `cmd` `` | `$(cmd)` | review — 1-to-1 modulo nesting; likely keep |
| `[ … ]` (single bracket) | `[[ … ]]` | drop — different quoting/word-splitting |
| `let x=…` | `(( x = … ))` | review |
| `$[ … ]` (old arith) | `$(( … ))` | review/keep |
| `for x; do` (no `in`) | `for x in "$@"` | review |
| `${var-x}` (no colon) | `${var:-x}` | check converter (already handled, spec 57.7) |

Record verdicts in DD#67 (table) and file individual keep-implementations
as needed. This is a *review* task — it produces decisions + small re-add
tasks, not one big change.

## Notes

- Semantic breaks are NOT in scope (DD#64 no-backslash, DD#62 `==` exact):
  they change meaning, not spelling, so there is no sugar to keep.
- Keep each re-add a thin parser alias onto an existing IR node — no new
  semantics.
