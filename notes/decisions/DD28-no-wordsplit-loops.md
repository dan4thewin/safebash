title: DD#28 — No Word-Split Loops
status: implemented

`for x in $choices` is a compile error. Use `for x in "${arr[@]}"`.
Consequence of DD#22 — no word splitting means unquoted expansion
in loop headers cannot split into multiple items.
