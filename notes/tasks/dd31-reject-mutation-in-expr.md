title: DD#31 — reject mutations in $(( )) expression context
status: ready
severity: correctness

transform.rs (~line 1780) handles `$((x=10))` by emitting an SSA
assignment and returning the value. DD#31 says `$(( ))` is a read
context — mutation inside it is invalid sash.

Implementation:
1. **Sash-native**: `$((x=10))`, `$((x++))`, `$((x+=1))` in expression
   context → compile error with diagnostic.
2. **Bash-mode coercion in lowerer**: `$((x=10))` → emit `((x=10))`
   statement before the expression, replace with `$((x))` (value read).
3. **Compound coercion**: `$((x=10, x<y))` → emit `((x=10));` statement,
   replace with `$((x < y))`.

Currently the compound assignment operators (`+=`, `-=`, `*=`, `/=`,
`%=`) and `++`/`--` in `$(( ))` have the same issue — all should be
rejected in sash-native or coerced in bash-mode.
