---
title: "DD#67 sweep: re-add name() func sugar, the convert switch, review rejected bash forms"
status: ready
priority: P3
kind: feature
depends: [DD67-native-syntax-sugar-policy]
gates:
  - "name() { } AND function name { } accepted as 1-to-1 sugar for fn name { } (same function IR)"
  - "flat backticks `cmd` accepted as 1-to-1 sugar for $(cmd); nested backticks rejected with a diagnostic"
  - "sash-convert gains a canonicalize mode emitting native-only syntax; compiler default preserves the user's form"
  - "the keep/drop table for all rejected bash forms is produced and RATIFIED BY DG before any sugar is implemented"
---

# DD#67 sweep — re-adds, the convert switch, and the rejected-form review

Implements DD#67 (accepted 2026-06-21). DG resolved all open items.

## 1. Re-add `name() { }` and `function name { }` (approved — both)

Both are textbook 1-to-1 aliases of `fn name { body }` — same function
IR, unambiguous. sash currently *rejects* them (only `fn name { }`).
Re-add both as accepted input; `fn` stays canonical.

- Spec: `f() { echo hi; }; f` → `hi`; `function g { echo hi; }; g` → `hi`;
  `fn h { echo hi }; h` → `hi` (all three work).
- Thin parser alias onto the existing function IR — no new semantics.

## 2. Backticks (approved — keep flat, reject nested)

- Flat `` `cmd` `` → accepted as 1-to-1 sugar for `$(cmd)`.
- **Nested** backticks → **reject with a diagnostic** (a new/!existing SB
  code): nesting requires `\``-escaping, which DD#64 removes, so it can't
  map 1-to-1. Direct the user to `$(…)`.

## 3. The canonicalize switch — in `sash-convert` (approved)

DG: the switch lives in **`sash-convert`**, not the compiler. There is no
`--emit-source` flag (removed when the converter split out; the compiler
rejects it and points at `sash-convert`). Add a `sash-convert` mode that
rewrites *all* sugar → **native-only** (`then…fi`/`do…done` → `{ }`,
`name()`/`function` → `fn`, flat `` `…` `` → `$(…)`). Compiler default =
**preserve** the user's form.

## 4. Sweep rejected bash forms → keep/drop table (RATIFY FIRST)

Enumerate **every** currently-rejected bash form, apply the DD#67 1-to-1
test to each, and produce a keep/drop table. **DG ratifies the table
before any sugar is implemented** — no ad-hoc keep/drop. Candidates:

| form | native | likely |
|------|--------|--------|
| `name() { }`, `function name { }` | `fn name { }` | keep (§1) |
| flat `` `cmd` `` | `$(cmd)` | keep (§2) |
| nested `` `…` `` | `$(…)` | drop/reject (§2) |
| `[ … ]` (single bracket) | `[[ … ]]` | drop — different quoting/word-splitting |
| `let x=…` | `(( x = … ))` | review |
| `$[ … ]` (old arith) | `$(( … ))` | review |
| `for x; do` (no `in`) | `for x in "$@"` | review |
| `${var-x}` (no colon) | `${var:-x}` | check converter (already handled, spec 57.7) |

Record verdicts in DD#67 (table). After ratification, file individual
keep-implementations as needed. This is a *review* task — it produces
ratified decisions + small re-add tasks, not one big change.

## Notes

- Semantic breaks are NOT in scope (DD#64 no-backslash, DD#62 `==` exact):
  they change meaning, not spelling, so there is no sugar to keep.
- Keep each re-add a thin parser alias onto an existing IR node — no new
  semantics.

## Keep/drop table — RATIFIED (DG, 2026-06-21)

**A. Re-add as 1-to-1 sugar:**
- `name() { }` → `fn name { }` — **KEEP** (re-add; currently SB4014).
- `function name { }` → `fn name { }` — **KEEP** (already accepted today).
- flat `` `cmd` `` → `$(cmd)` — **KEEP** (re-add).

**B. Single-bracket `[ … ]`:** removed as a test construct. `[` becomes an
**ordinary literal character** (like backslash under DD#64) — it no longer
*starts a test*. `[[ … ]]` is the only test form. The distinct constructs
`[[ … ]]` and `arr[i]` subscript keep their meanings.

**C. Stay dropped (fail the 1-to-1 test):** nested backticks (need
`\``-escaping, DD#64 removes); `?`/`[…]`/`**` globs (DD#39, only `*`);
heredocs `<<`/`<<-` (use `<<<`/vars); `export`/`declare -x` (use
int/const/array/map); `coproc`; `[ -v ]` (use `-n`); `IFS=` (no word
splitting); prefix `++x` (use `x++` / `(( x += 1 ))`).

**D. Owned by other DDs:** `case…esac` → DD#66; brace-form control flow +
`then…fi`/`do…done` canonicalization → DD#68; `${var}` modifiers → DD#65.
