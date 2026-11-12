title: DD#67 — Native syntax, with unambiguous bash forms kept as 1-to-1 sugar
status: accepted

1. Accepted syntax aliases must preserve structure and semantics
   unambiguously; a form requiring semantic translation is not mere sugar.
2. Brace control-flow bodies and their keyword spellings share one meaning.
   `fn name { }`, `name() { }` and `function name { }` share one function
   contract, including current name, placement and result-kind rules.
3. Preserve accepted input spelling by default. Source canonicalization and
   semantic migration belong to `sash-convert`, not the compiler.
4. DG ratifies new keep/drop classifications before sugar is implemented.

---

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
| `name() { }` | `fn name { }` | yes | ~~**re-add candidate** (currently rejected)~~ accepted below |
| `case…esac` | `[[ ]]` dispatch | **no** — glob patterns → `==`/`=~` is a translation | **drop** (DD#66) |
| `;&` / `;;&` | (none) | no native equivalent | **drop** (DD#66) |
| `` `cmd` `` | `$(cmd)` | **no** — nesting needs `\``, which DD#64 removes | **drop** (2026-09-14) |
| `[ … ]` | `[[ … ]]` | no — different quoting/word-splitting | drop |

### Backticks: resolved as a drop (DG, 2026-09-14)

The table entry read "~yes (modulo nesting) | review" for two years. The
modulo was the whole answer. `` `cmd` `` maps to `$(cmd)` only while it does
not nest; nesting requires `` \` ``, and DD#64 removed the shell backslash.
So the mapping is not total, which is exactly what the 1-to-1 test excludes.

Backticks are now **rejected** (SB5076) in every position where they were
previously command substitution, and `sash-convert` rewrites them to `$( … )`
— the standard shape of a drop under this policy. A backtick inside `"…"`
is unaffected: it has been a literal character since 2026-08-28, and stays
one.

**Semantic** breaks are *not* sugar candidates — they change meaning, not
spelling. E.g. DD#64 (no shell backslash) and DD#62 (`==` exact) stay
native-only; there is no bash-compat sugar for them.

## Default + convert switch

- **Default:** accepted bash sugar is **preserved** on output —
  ~~`--emit-source` round-trips what the user wrote~~.
- **Convert / canonicalize:** a switch (~~an `--emit-source` mode or~~ a
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
  ~~`name()` is a re-add candidate (see Open).~~ The resolution below accepts it.

## Resolved (DG, 2026-06-21)

- **Re-add `name() { }` AND ksh `function name { }`** as 1-to-1 sugar for
  `fn name { }`. `fn` stays canonical; both bash spellings are accepted
  input that lowers to the same function IR.
- ~~**Backticks**: keep **flat** `` `…` `` as 1-to-1 sugar for `$(…)`;
  **reject nested** backticks with a diagnostic.~~ **Superseded
  2026-09-14** — the same reasoning that rejects the nested form rejects
  the flat one, because a form that cannot nest is not a total mapping.
  All backticks are dropped; see above.
- **Convert/canonicalize switch lives in `sash-convert`**, not the
  compiler. There is no `--emit-source` CLI flag (it was removed when the
  converter was split out; the compiler rejects it and directs to
  `sash-convert`). `sash-convert` is the one source-rewriting tool and the
  natural home for sugar→native canonicalization. Default still = preserve
  the user's form.
- **Sweep**: do a one-time systematic pass over every currently-rejected
  bash form, applying the 1-to-1 test, and produce a **keep/drop table**.
  DG **ratifies the table before any sugar is implemented** (no ad-hoc
  keep/drop). See task `dd67-bash-sugar-sweep`.
