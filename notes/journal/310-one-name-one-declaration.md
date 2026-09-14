---
title: One name, one declaration
---

# One name, one declaration

DG asked, at the end of the END-block work: *should SB5049 forbid all
redeclaration regardless of scope?*

I measured what SB5049 actually did before answering. It fired only
**cross**-scope — a typed declaration inside a function of a name already
global. At top level every one of these was accepted:

| case | before |
|---|---|
| `str x` twice | accepted |
| `str x` then `int x` — **type change** | accepted |
| `str x` then `const x` — **acquires const** | accepted |
| `const x` twice | SB4044 |

The middle two are the ones that matter. A variable's type changed, or a
mutable variable became immutable, partway down a file, with no diagnostic.
DD#86 says a name is declared once per scope; half the rule was enforced.

The literal reading of DG's question would have broken two things that must
stay legal: two sibling functions each declaring `str x` (genuinely different
scopes), and a repeated `for x` (not a declaration at all). I put that
caveat in front of the recommendation rather than silently narrowing the
question. DG chose the same-scope rule.

## The rule

A typed declaration (`str`/`int`/`array`/`map`/`const`) of a name already
declared in the **same** scope is SB5049, regardless of whether the type
matches. `check_redeclaration` is the companion to `check_no_shadow`: one
handles same-scope, the other cross-scope, and together they say one name,
one declaration, per scope.

Control-flow bodies share the enclosing scope's map — DD#86 gives control
flow no scope of its own, so `if … { str x } else { str x }` is one global
declared twice, not two locals. A function body gets a fresh map. The top
level and the DD#56 END body share one map, because END's declarations *are*
globals.

`for` needed no special case: a loop variable is its own `StmtKind`, never a
`Declaration`, so it never enters the map.

## SB4044 was describing the wrong act

`const x=1; const x=2` reported SB4044: *"readonly variable `x` cannot be
reassigned"*. The user had not tried to assign to anything. They declared a
name twice, and the diagnostic named the wrong mistake.

Both codes fired once the new check landed, so I removed `flag_if_const` from
the *declaration* path. SB4044 now means only what it says — a bare
assignment writing a const.

That is a removal, so it needed proof it lost nothing. A thirteen-case const
battery, diagnostics captured before and after: six cases dropped the
redundant SB4044, **none became accepted**. The SB4044-only cases are all
genuine assignments, including a `for` variable rebinding a const, which is a
different path and untouched. Spec 35.7 pinned the old code and now pins
SB5049 with `$err != *SB4044*` — the point is that exactly one diagnostic
fires, and it is the accurate one.

## The bug the new specs found

Spec 86.37 and 86.38 failed, and not because of the rule:

```
$ sash -c 'fn f { str x="a" }'
error [SB4014] expected variable name in declaration
```

`parse_declaration_assignments` is a loop, because `str a b c` declares three
names. It stops at a newline, a `;`, EOF, or a redirect. It does **not** stop
at `}`. So a declaration that was the last statement in a brace block
consumed the closing brace as a candidate name and reported "expected
variable name" about a declaration that was already complete.

Every brace form: `fn`, `for`, `if`, `while`, and DD#56 `END`. I confirmed it
at `be36208` and earlier, so it long predates this session's work.

Only the *last* statement, which is why it survived. `spec/brace.sh` has
twenty-seven tests and none caught it, because every one of them either put
the declaration before something else — so a `;` stopped the loop — or used a
command rather than a declaration as the final statement. The bug lived
exactly in the gap between "declaration" and "last in block", and nothing
sat there.

Fix: add `Tok::RBrace` to the stop set. The redirect stops stay, and
`int x=5 > f` is still the clean SB4046 rather than regressing to the same
misleading SB4014 — that is `bc.f5`.

I found it only because I wrote the new specs in the compact brace form. Had
I written them the way the existing scope specs are written, with each
declaration on its own line, the suite would have gone green and the bug
would still be there.

## Specs

- `scope.sh` 86.31–86.33 — redeclaration, type change, const acquisition
- `scope.sh` 86.34–86.36 — controls: sibling-function locals, repeated `for`,
  one declaration of two distinct names
- `scope.sh` 86.37 — top level and END are one scope
- `scope.sh` 86.38 — branches share the enclosing scope
- `types.sh` 35.7 — `const` twice is SB5049, and *not* SB4044
- `brace.sh` bc.f1–bc.f5 — a declaration last in each brace form, plus the
  redirect control

Sweep: **4611 passing, 10 skipped, 0 failing** (+26). fmt, clippy, and the
complexity gate green; `cargo test --release` 77 passing.

## Note

`:ir-version` stays at 1 — DG: no consumers yet.
