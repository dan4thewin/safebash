title: DD#22 — No Word Splitting, No IFS
status: implemented

1. Scalar expansion never implicitly splits into words; iteration over stored
   elements uses an explicit collection splat.
2. Quotes produce one string, including explicit integer-to-string conversion;
   they cannot enclose a collection splat.
3. Explicit splitting and line input use DD99's flagless `read` and `readline`
   contracts.

---

~~`$var` and `"$var"` are identical.~~ No implicit splitting. IFS doesn't
exist. `for x in $choices` is a compile error — ~~use
`for x in "${arr[@]}"`.~~ Use `for x in @arr` (DD75).

<del>

Read taxonomy (finalized):

| Form | Semantics | Delimiter |
|---|---|---|
| `read -r line` | One line → one var | newline |
| `read -d: -r a b c` | One line → named fields | explicit `-d` |
| `read -d: -a arr` | One line → array | explicit `-d` |
| `mapfile -t arr` | Many lines → array | newline |

`-r` always implied. `-d` replaces all `IFS=x read` patterns.

</del>

DD99 replaces this taxonomy with `read [SRC] TARGET` and `readline TARGET`.

## Addendum 2026-06-13: quotes always yield a single string

Quoting an array splat is a compile error — no exceptions:

    "${a[@]}"    rejected
    "$a[@]"      rejected
    "$(argv)"    rejected
    "$(argv N)"  rejected

The previous tolerance for decorative quotes (the `DoubleQuoted`-unwrap
path in `transform/control_flow.rs::detect_array_iter`) is removed. Quotes
exist for string concatenation, interpolation, and int-to-string
conversion only. Array-context splat is a separate syntactic category
expressed by `@arr` / `$(argv)` / `$(argv N)` (DD#41, DD#60).

Bash-mode source uses the bash-style quoted spelling; sash-convert coerces
it to the sash-native unquoted form.
