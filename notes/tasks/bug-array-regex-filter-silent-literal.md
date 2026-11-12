title: "`array a=(GLOB =~ 'RE')` silently yields literal words when GLOB isn't bare `*`"
status: done
priority: P2

> **RESOLVED 2026-09-14 — DD#108, journal 305.** Neither of the two fix
> options this task listed was taken. DG's ruling: the shorthand is retired
> entirely (**SB5075**), because generalizing element 0 does not survive
> contact with the rest of the form.
>
> The decisive measurement made after filing: **quoting was invisible to the
> desugar.** `(* =~ re)`, `(* '=~' re)`, `('*' =~ re)` and `("*" =~ re)` all
> produced identical output. So the escape hatch that would have made the
> form tractable could not be wired without letting quotes change a list's
> *arity and meaning* rather than a value — the bash trap the subset exists
> to avoid. Combined with silent arity-sensitivity (`(* =~ a)` filtered,
> `(* =~ a b)` did not) and the form's inability to name whether it matched
> the basename or the DD#73 absolute path, the syntax itself was the defect.
>
> The `for` loop was already expressible and needs no new surface, so the
> replacement costs nothing but verbosity — and it names `$f`, which removes
> the match-target ambiguity by construction.
>
> `sash_glob_regex` is deleted with it.


# `array a=(GLOB =~ 'RE')` silently yields literal words when GLOB isn't bare `*`

Found while writing spec replacements for the Rust test redux
(2026-09-14). `sash/tests/codegen_tests.rs:glob_regex_filter` was the
only coverage and it hardcoded `/home/dangood/src/safebash/sash/src`,
an absolute path from a different host, so it could never have passed
here — the gap was invisible.

## The defect

`transform/expressions.rs::try_array_regex_filter` requires element 0 to
be exactly `*`:

```rust
if star != "*" || op != "=~" {
    return None;
}
```

Any other glob falls through to `SsaValue::IndexedArray`, so the `=~`
becomes an ordinary array element. Measured in a directory holding
`g10a.ff`, `g10b.ff`, `g10c.gg`:

| program | len | elements |
|---|--:|---|
| `array a=(* =~ '\.ff$')`     | 2 | the two `.ff` paths — correct |
| `array a=(*.ff)`             | 2 | the two `.ff` paths — correct |
| `array a=(*.ff =~ 'NOPE')`   | 3 | `*.ff`, `=~`, `NOPE` |
| `array a=(*.?? =~ '\.ff$')`  | 3 | `*.??`, `=~`, `\.ff$` |
| `array a=(*.gg =~ '\.ff$')`  | 3 | `*.gg`, `=~`, `\.ff$` |

The last three are the bug. Note the glob does not even expand: the
array holds the literal pattern text. No diagnostic is emitted, and the
program compiles and runs.

## Why it is a bug either way

Journal 042 records the design intent — the pattern is bare `*` by
construction, and the regex exists precisely because the glob has no
`?` or `[abc]`:

```
arr=(* =~ '\.sh$')         # regex filter — full power
```

So `(*.ff =~ 'RE')` arguably should not be written. But silently
producing a three-element array of un-expanded pattern text is wrong
under any reading: the user asked for a filtered file list and got
literal junk, with `len` reporting 3.

## Fix approach — needs DG

The complication is that `=~` is a legal array element in a plain
literal: `array a=(x =~ y)` is honestly a 3-element array, and must
stay one. So a blanket rejection of `=~` inside `( )` is wrong.

- **(a) generalize** — accept `GLOB =~ 'RE'` for any element 0
  containing a glob metacharacter: glob by the pattern, then filter by
  the regex. No new diagnostic; `(x =~ y)` with no metacharacter stays
  a literal array. Widens the language surface.
- **(b) diagnose** — when element 0 contains a glob metacharacter and
  element 1 is exactly `=~`, reject with a new SB code directing to the
  bare-`*` form. Keeps the surface as designed, costs a diagnostic.

(b) matches the documented intent; (a) is what a user would expect the
form to mean. Either fixes the silent-literal outcome.

## Coverage now in place

`spec/glob.sh` g.10 / g.11 pin the working bare-`*` form (match and
no-match). Neither pins the broken combination — do not add a spec for
the current behaviour, it is the bug.
