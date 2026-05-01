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
