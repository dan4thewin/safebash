title: "sash-convert: update empty-splat guidance for :~"
status: open
priority: P3
kind: work
depends: []
gates:
  - "guidance for Bash positional and array splats includes :~ as the zero-argument empty case, distinct from :- and :?"
  - "guidance follows current collection, argument and taint rules without claiming that new language syntax is needed"
  - "focused converter specs pin the guidance and unchanged literal/non-splat controls; no automatic scaffold or taint policy is selected by this reconciliation"

## Remaining work (reconciled 2026-11-16)

[DD84](../decisions/DD84-optional-args-elision.md) already supplies
`@{a:~}`: an empty array contributes zero arguments. `@{a:-}` instead
contributes one empty argument, and `@{a:?}` aborts. The language fork in
this filing is settled; it is not blocked on DG inventing a fourth form.

The converter work is not complete. Current `convert_splat` and
`convert_bracket_splat` guidance still names only `:-`/`:?` for `"$@"` and
`"${array[@]}"`, omitting the available zero-argument case. Correct that
advice and its regression coverage. Retain DD57's manual-fallback boundary;
this cleanup does not choose or implement an automatic scaffold.

## Historical fork

`converter-positional-use-needs-bless` is implemented except for this case.
DG's 2026-07-08 resolution said to emit:

```bash
argv a; attest a in e [[ -n $e ]]   # MANUAL(sash-convert): tighten predicate
cmd @a
```

That resolution **predates SB5032**, which now rejects a bare `@a` splat into
an external command:

> a bare `@a` array splat into an external command is unsafe — an empty array
> would run it with too few arguments. State the empty case: `@{a:-WORD}`
> (empty → the single arg WORD) or `@{a:?WORD}` (empty → abort with WORD)

The scaffold as written does not compile. The filing considered only the
two alternatives below, both of which diverge from Bash's **zero** arguments
for an empty splat:

| spelling | empty `"$@"` behaviour | matches bash? |
|---|---|---|
| `cmd @{a:-WORD}` | runs `cmd WORD` — one argument | no |
| `cmd @{a:?WORD}` | aborts with `WORD` | no |
| (bash) | runs `cmd` with no arguments | — |

## Historical emitted example (journal 289)

A MANUAL marker naming the fork, and no rewrite:

```
# MANUAL: `"$@"` as a command argument — snapshot it (`argv 1 a`;
# `attest e in a [[ -n $e ]]`) and pass `@{a:-WORD}` or `@{a:?WORD}`,
# which states what an empty argument list should do (SB5032)
/bin/echo "$@"
```

This satisfies the parent task's gate ("either compiles or carries a MANUAL
marker") and does not silently change what the script does. Pinned by
`spec/convert.sh` conv.x7.

## Historical options

1. **Keep MANUAL-only.** Honest; the author is told exactly what to decide.
   Costs: converted scripts using `cmd "$@"` need a hand edit before they
   build. Recommended, and what is shipped.
2. **Scaffold with `@{a:?…}`.** Compiles; an empty argument list becomes a
   runtime abort. Loud, so it will not be missed — but it is a behaviour
   change the author did not ask for, and `cmd` with no arguments is a
   perfectly ordinary thing for a script to do.
3. **Scaffold with `@{a:-…}`.** Compiles; an empty argument list silently
   grows one argument. This is the worst of the three: it changes behaviour
   quietly.
4. **A fourth sash spelling** meaning "empty → pass nothing". DD84's `:~`
   subsequently settled this part; it is no longer an open language choice.

The empty-case expressiveness question is resolved. Converter guidance and
any separately selected automatic rewriting must use the current rules,
not this obsolete two-choice inventory.
