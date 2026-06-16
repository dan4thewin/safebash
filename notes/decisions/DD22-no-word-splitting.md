title: DD#22 — No Word Splitting, No IFS
status: implemented

`$var` and `"$var"` are identical. No implicit splitting. IFS doesn't
exist. `for x in $choices` is a compile error — use
`for x in "${arr[@]}"`.

Read taxonomy (finalized):

| Form | Semantics | Delimiter |
|---|---|---|
| `read -r line` | One line → one var | newline |
| `read -d: -r a b c` | One line → named fields | explicit `-d` |
| `read -d: -a arr` | One line → array | explicit `-d` |
| `mapfile -t arr` | Many lines → array | newline |

`-r` always implied. `-d` replaces all `IFS=x read` patterns.

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
