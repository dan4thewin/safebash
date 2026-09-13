# 296 — One bash spelling, four sash answers

`converter-retire-bracket-splat` looked like a lookup table. DD#75 retired
`${a[@]}`, `${a[*]}`, `${#a[@]}` and `${!a[@]}`; the filing listed what each
becomes; write the mapping.

The mapping is not a function of the input.

## Why there is no table

Three sash spellings replace the bracket forms, and they are not
interchangeable:

- `@a` is a **splat**. It is legal as a for-in source and nowhere else —
  `echo @a` is SB5030 (echo would flatten it), `cmd @a` is SB5032 (an empty
  array would run the command with too few arguments).
- `$(join a)` is a **scalar**. Always legal, never a splat.
- `$(keys m)` is a **splat that cannot be joined at all** (SB5025) — valid
  only as the sole source of a for-in loop.

So `"${a[@]}"` has four different right answers depending on where it sits,
and the converter has to read the context to pick one:

| context | answer | why |
|---|---|---|
| for-in source | `@a` | the only place a splat is legal |
| `echo` argument | `$(join a)` | echo rejects splats; join is what bash prints |
| other command argument | MANUAL | no faithful answer exists |
| value, or spliced in a string | `$(join a)` | bash's quoted form is one word |

The `echo` exception is the one I want to defend, because a special case for a
single command name looks like a hack. It is not: `echo` *rejects* a splat and
its own diagnostic directs you to a join, and bash's `echo "${a[@]}"` prints
the elements space-separated, which is exactly `echo $(join a)`. The exception
is not "echo is special", it is "echo is the one command whose argument
handling makes the join faithful rather than a guess".

The external-command case gets a MANUAL, the same wall `"$@"` already hits.
Bash runs the command with zero arguments when the array is empty; neither
sash spelling that states the empty case (`@{a:-WORD}`, `@{a:?WORD}`) does
that. There is no mechanical answer, so the converter says so instead of
picking one. That question is filed (`converter-splat-empty-case`) and is
DG's.

## A gate that asked for something retired

Gate 3 wanted `${#a[@]}` → `` `${#a}` (or `$(len a)`) ``.

`${#a}` is itself retired — SB4038, the `${#…}` length sugar. So half of that
gate asks the converter to emit something the compiler rejects. The other
half, `$(len a)`, was already being produced: `${#a[@]}` hits
`length_expansion` before anything bracket-related runs.

I wrote the spec for it anyway (`57.101`), and it was the only one of the nine
that passed under fail-first. That is worth a spec even though it required no
code: the behaviour is correct today by an accident of ordering, and nothing
was pinning it.

This is the fourth filing this session with a materially wrong claim. The
pattern is consistent: they are wrong about *targets*, not about *problems*.
The problem each one names is real and still there; the sash spelling it
proposes has moved. A filing records a fix in the language of its moment.

## Displacing a diagnostic that was lying

`${!a[@]}` used to produce:

```
# MANUAL: ${!var} indirect expansion not supported in sash
```

It is not indirect expansion. `${!a[@]}` is the **keys** form, and sash
supports it — `$(keys m)` — for maps. The check was `text.starts_with("${!")`,
which is true of both constructs, so the rarer one was wearing the commoner
one's message.

This is the third instance in three days of the same shape: **one arm serving
two populations** (SB5011 in journal 288, "expected `]]`" in 294, and now
this). What makes it hard to notice is that the message is *plausible* — it
names a real unsupported feature, just not the one in front of you. A
diagnostic that is wrong-but-plausible costs more than one that is obviously
wrong, because the reader believes it and goes looking for indirect expansion
in a script that has none.

Spec `57.99` asserts both halves: the new note is present *and* the old one is
absent. Asserting the absence matters — without it the spec would pass on a
converter that emitted both.

## What I did not convert

Gates 1 and 2 also name `$a[@]` and `$a[*]`. Those are not bash array splats.
Bash reads `$a[@]` as `$a` followed by the literal text `[@]`, and tree-sitter
agrees — it emits four sibling nodes, `simple_expansion` and three `word`s.
Converting them to `@a` would mean giving them a meaning bash never gave them,
which is the one thing a mechanical converter must not do.

Recorded in the task rather than silently skipped, because "the filing asked
for this and I declined" is exactly the kind of thing that should be visible.
