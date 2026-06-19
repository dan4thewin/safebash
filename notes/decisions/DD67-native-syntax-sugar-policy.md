title: DD#67 — Native syntax, with unambiguous bash forms kept as 1-to-1 sugar
status: candidate

sash prefers clean, regular native syntax, but to avoid turning away
bash users it does **not** gratuitously break familiar forms. The policy
(from the 2026-06-19 discussion):

> When a bash form is a **1-to-1, unambiguous, structure-preserving**
> alias of a native sash form, sash **accepts both**. The **default
> preserves** whatever the user wrote (familiar bash passes through
> unchanged); a **convert switch** rewrites to native-only for those who
> want the clean form. A bash form that can **not** map 1-to-1 — it needs
> semantic translation, or is ambiguous — is **dropped** (and the
> converter rewrites it into the native form).

## The 1-to-1 test

Keep-as-sugar **iff** the bash↔native mapping is a pure alias: same
structure, same semantics, unambiguous, reversible. Otherwise drop.

| bash form | native | 1-to-1? | verdict |
|-----------|--------|---------|---------|
| `then…fi` / `do…done` | `{ }` block | yes — pure bracket alias | **keep** (DD#68) |
| `name() { }` | `fn name { }` | yes | **re-add candidate** (currently rejected) |
| `case…esac` | `[[ ]]` dispatch | **no** — glob patterns → `==`/`=~` is a translation | **drop** (DD#66) |
| `;&` / `;;&` | (none) | no native equivalent | **drop** (DD#66) |
| `` `cmd` `` | `$(cmd)` | ~yes (modulo nesting) | review |
| `[ … ]` | `[[ … ]]` | no — different quoting/word-splitting | drop |

**Semantic** breaks are *not* sugar candidates — they change meaning, not
spelling. E.g. DD#64 (no shell backslash) and DD#62 (`==` exact) stay
native-only; there is no bash-compat sugar for them.

## Default + convert switch

- **Default:** accepted bash sugar is **preserved** on output —
  `--emit-source` round-trips what the user wrote.
- **Convert / canonicalize:** a switch (an `--emit-source` mode or a
  `sash-convert` option) rewrites *all* sugar → native-only — the
  one-true-form for users who want the clean look. Built on the existing
  `--emit-source` / `--annotate` / `sash-convert` tooling.

## Why

Adoption hedge. The uglier warts (`case…esac`, `;&`/`;;&`) are dropped —
no one will miss them and they don't map cleanly. But the ubiquitous
`if…then…fi` / `for…do…done` keep working verbatim, so existing muscle
memory and scripts aren't a barrier to trying sash. Anyone who wants the
clean native form runs the converter; the difference is cosmetic, not
semantic.

## Applications / relations

- **DD#66** (`case` → `[[ ]]` dispatch): a *drop* — fails the 1-to-1 test
  (glob ≠ exact/regex; structural change).
- **DD#68** (brace-block control flow): a *keep* — `then/fi/do/done` are
  1-to-1 sugar over `{ }`; both supported.
- `fn name { }` already replaced bash `name()`; under this policy
  `name()` is a re-add candidate (see Open).

## Open / to confirm

- **Re-add `name() { }`** (and ksh `function name { }`) as 1-to-1 sugar
  for `fn name { }`? `fn` is currently the only accepted form.
- **Backticks** `` `…` `` as sugar for `$(…)` — 1-to-1 modulo nesting;
  keep or drop?
- The convert-switch spelling (flag name; e.g. `--emit-source --native`).
- A **sweep** of currently-rejected bash forms against the 1-to-1 test,
  so keep/drop is applied consistently rather than ad hoc.
