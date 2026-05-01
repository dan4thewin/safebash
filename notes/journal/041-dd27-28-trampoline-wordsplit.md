# Design Decision #27: Trampoline model for functions

**Decision:** Bash functions are implemented as blocks within the single
Cranelift `_start` function, not as separate Cranelift functions.

**Rationale:** Analyzed all six potential drawbacks of the trampoline model:

1. **No recursion** — eliminated. Safe subset only allows tail recursion,
   which is just a `Goto` back to the function entry block. Guaranteed
   flat loop, better than relying on Cranelift tail-call optimization.

2. **Register pressure** — not real. Cranelift's liveness analysis sees
   that variables in disjoint function blocks are never simultaneously
   live. Registers are reused, not competed for.

3. **No `local` isolation** — solved by the transform. `local x` inside
   `fn foo` is renamed to `foo__x`, a different base variable with its
   own Cranelift Variable pair. Non-local variables share the caller's
   CL Variables, matching bash semantics (functions modify globals).

4. **`$(func)` capture** — turned into a compiler advantage. When the
   call site is `X=$(func)`, the compiler emits duplicate function
   blocks where `echo` appends to a buffer instead of writing to fd 1.
   No fork, no pipe. This is something bash cannot do.

5. **Compile time** — irrelevant. Longest target script is 898 lines.
   Cranelift compilation is near-instant at this scale.

6. **No separate compilation** — not a use case. Safebash compiles
   self-contained scripts, not reusable function libraries.

**Avoided alternative:** Real Cranelift functions (option 2) would require
moving all bash variables from Cranelift Variables to a global memory
struct with explicit load/store — a significant rewrite of the codegen
for no practical benefit at this scale.

**Implementation plan:**
- Transform: `fn_*` blocks already emitted, stop skipping them in codegen
- Call site: save `argc`/`argv`, set new positional params, `Goto(fn_entry)`
- Return: `Goto(return_site)`, restore `argc`/`argv`
- `local x`: transform renames to `funcname__x`
- `$(func)`: emit `fn_foo__capture` variant with buffer writes (option A)

## Design Decision #28: No word-split loops, no glob loops

**Context:** `for x in $(cmd)` and `for x in *` are two of bash's
most dangerous patterns. The first word-splits command output on IFS
(spaces in filenames break it, buffering scales poorly). The second
expands globs at runtime with behavior that varies by `nullglob`,
`failglob`, `dotglob` settings.

**Decision:** Both forms are coerce-mode rewrites (SB4020, SB4021):

- `for x in $(cmd)` → `cmd | while read -r x` — streams one line
  per iteration, no word splitting, handles arbitrary filenames.
- `for x in *` → `find ... | while read -r x` or internal
  `sash_readdir` — same streaming model, no glob ambiguity.

**Rationale:** The pipe-while-read form is strictly superior:
1. Streams (O(1) memory vs O(n) for command substitution buffering)
2. One entry per iteration (no word-split bugs)
3. Handles filenames with spaces, newlines, etc.
4. Makes the splitting rule explicit (newline-delimited)

The safe subset eliminates word splitting entirely (DD#22). These
coerce rules are the for-loop consequence of that decision.

**New runtime needed:** `sash_read_line(fd) → (ptr, len, eof)` for
the `while read` loop body.

### Design Decision #29: Coerce-on-failure, not separate tool

**Context:** Previously planned a standalone `sash-coerce` tool as a
separate pipeline step. Reconsidered.

**Decision:** `sash` is one tool. On compilation failure due to unsafe
bash, it emits `script.sash` — a valid bash script with:

1. **Safe coercions applied** as code changes, each annotated with a
   comment citing the rule (e.g., `# [SC2086] unquoted → quoted`).
2. **Rejected lines left as-is**, with an annotation comment above
   (e.g., `# [SB0001] REJECT: eval with dynamic argument`).

The `.sash` file is always valid bash that runs identically to the
original. The only code changes are safe mechanical rewrites. Rejected
lines are untouched — the user must delete or rewrite them manually.

**Workflow:**
1. `sash script.sh -o binary` → fails, emits `script.sash`
2. `diff script.sh script.sash` → review coercions, resolve rejects
3. `sash script.sash -o binary` → succeeds or shows remaining rejects
4. Repeat until clean.

**Exit codes:** 0 = compiled. 1 = coerced file emitted (all auto-fixable).
2 = coerced file with rejects (needs human). 3 = unparseable.
