# Design Decision #22 (candidate): No word splitting, no IFS, arrays and read

**Date:** 2025-04-15

**Principle:** Word splitting doesn't exist in safebash. `$var` and `"$var"` are
identical — quotes are purely cosmetic. IFS doesn't exist as a concept. This
eliminates an entire class of bugs.

**Consequences:**

1. **Unquoted `$var` is never split.** No implicit splitting on spaces, tabs,
   newlines, or any other character. An unquoted expansion always produces
   exactly one value.

2. **`for x in $choices` is a compile error.** Space-separated strings as
   pseudo-lists are replaced by proper arrays: `for x in "${choices[@]}"` (or
   a simpler syntax TBD).

3. **`IFS` doesn't exist.** No global variable controlling field splitting.
   The only place splitting occurs is `read`, which specifies its separator
   explicitly.

4. **`read` / `mapfile` taxonomy (finalized 2026-04-17):**

   Four distinct operations, all IFS-free:

   | Form | Semantics | Delimiter |
   |---|---|---|
   | `read -r line` | One line → one variable | newline (implicit) |
   | `read -d: -r a b c` | One line → named fields | explicit via `-d` |
   | `read -d: -a arr` | One line → array of fields | explicit via `-d` |
   | `mapfile -t arr` | Many lines → array of lines | newline (default) |

   - `-r` is always implied (SC2162 coerces bare `read` → `read -r`).
   - `-d` replaces all `IFS=x read` patterns. No IFS variable exists.
   - `<<<` (here strings) supported for feeding variables to `read`:
     `read -d: -a path <<< "$PATH"` — compiler lowers to memory buffer,
     not a temp file.
   - `read -a` is NOT redundant with `mapfile`: `read -a` splits one
     line into fields; `mapfile` loads many lines as whole elements.

5. **Array syntax (TBD):** The `"${array[@]}"` syntax is widely considered
   awkward. A simpler iteration syntax is desirable but deferred to a dedicated
   design session.

6. **Multi-dimensional arrays (TBD):** Currently faked with composite string keys
   (`payload[foo:version]`). Whether safebash needs proper multi-dimensional
   arrays or whether composite keys are sufficient is deferred.

**Migration:** The coerce tool (DD#8) converts `IFS=x read` to `read -dx`,
converts `for x in $var` to array iteration, and makes quotes optional
(since they're no longer semantically meaningful for variable expansion).

