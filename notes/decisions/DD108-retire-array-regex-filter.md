title: DD#108 — Retire the `arr=(GLOB =~ 'RE')` regex-filter shorthand
status: implemented

The array regex-filter shorthand is removed. `arr=(GLOB =~ 're')` is rejected
with **SB5075**, which names the loop that replaces it. The plain single-glob
form `arr=(*.sh)` is unaffected and stays.

## Why it went

The shorthand was a **list literal whose meaning depended on its own
contents**, which is the one thing a list must never be:

| form | meant |
|---|---|
| `arr=(* =~ re)` | glob `*`, filter by regex |
| `arr=(*.ff =~ re)` | **3 un-globbed literal words**, silently — no diagnostic |
| `arr=(x =~ y)` | a plain 3-element array |
| `arr=(* =~ a b)` | a plain 4-element array, silently |

The second row is the bug that started this
(`bug-array-regex-filter-silent-literal`): the transform required element 0 to
be *exactly* `"*"`, so any real glob pattern fell through to an indexed array
of literal text with no error.

The obvious repair — generalize element 0 to any glob — does not survive
contact with the rest of the form:

1. **Quoting could not disambiguate it.** Measured: `(* =~ re)`, `(* '=~' re)`,
   `('*' =~ re)` and `("*" =~ re)` all desugared *identically*. Quoting was
   invisible to the shorthand. Making quotes the escape would therefore mean
   letting quotes change a list's **arity and meaning**, not merely a value —
   which is the bash misfeature the safe subset exists to avoid.
2. **Arity-sensitivity was silent.** Three elements filtered; four did not.
3. **It could not name what it matched.** The runtime matched the regex
   against the *bare filename*, while the elements it produced were DD#73
   *absolute paths*. The form gave the reader no way to tell which, and no way
   to ask for the other.
4. **It did not compose.** No way to filter an array that did not come from a
   glob, to glob another directory, or to invert a match.

## What replaces it

The loop, which was always expressible and needs no new surface:

```text
array a
for f in *.ff { if [[ $f =~ 're' ]] { a += ($f) } }
```

It is longer, and it is unambiguous: `$f` is named, so whether the regex tests
the path or the basename is written down rather than inferred.

## The rule

A **bare `=~` anywhere in an array literal** is SB5075. Uniform, with no
positional or metacharacter special-casing — the alternative ("reject only
when element 0 holds a glob metacharacter") would have re-introduced exactly
the content-dependent reading being removed.

A **quoted** `=~` is an ordinary string element and stays legal:
`arr=(x '=~' y)` is a 3-element array. That is the escape, and the diagnostic
names it.

## Consequence for the runtime

`sash_glob_regex` is **deleted**. Its name was a contradiction (DG): a
function that both globs and regexes is two pattern languages doing one job,
and the only way to generalize the shorthand was to give it a *second* pattern
argument, making the name worse. `sash_glob(dir, pattern)` remains the single
glob primitive.

This supersedes the sketch in journal 042, which proposed `arr=(* =~ re)` as
the "full power" replacement for the dropped `?` and `[abc]` glob characters.
That need is real; the loop serves it.

## Provisional directory-selector counterpart (2026-11-09)

DD131 proposes `rglob ARRAY DIRECTORY REGEX...` for a separate-branch
trial. It selects directory entries using **one** pattern language,
regex, with an explicit directory and documented basename subject.
It is not a glob followed by a regex filter and does not restore the
content/arity-sensitive array shorthand or `sash_glob_regex`.
The loop remains the explicit way to refine existing arrays or choose
a different subject. No new selector is implemented by this recording.

## Not affected

- `arr=(*.sh)` — the single-glob shorthand. Unambiguous, retained.
- `var =~ s/re/str/` and `${var:s/…}` — DD#39 regex mutation.
- `[[ $x =~ re ]]` — the boolean regex test.
- `$(sub S P R)` / `$(gsub S P R)` — DD#63.
