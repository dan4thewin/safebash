# Design review: source, indirect expansion, namerefs — 2026-04-11

DG reviewed the design decisions AI had made unilaterally in
experiments 006–007. Three decisions were resolved through
discussion. Recorded here as durable design rationale.

### Decision 1: source / . — compiled away, not rejected

**Problem:** Scripts need to acquire key-value configuration from
the local host at runtime (e.g., compile on build box, deploy to
node, node has a config file the script sources). The sourced file
is not available at compile time.

**Resolution:** The script author writes idiomatic `. ./vars`. The
compiler replaces it with a safe runtime mechanism:

1. Fork a Rust helper (`sash-expand`) on the file path.
2. The helper validates every line is `KEY=VALUE`, comment, or
   blank. Dies on the first non-conforming line, printing the
   offending line and line number.
3. Values may use the safe subset of variable operations —
   the same operations safebash allows in compiled code:
   - `$VAR` / `${VAR}` — simple expansion
   - `${VAR:-default}` — default value
   - `${VAR:+alternate}` — alternate value
   - `${VAR%%pat}` / `${VAR%pat}` — suffix stripping
   - `${VAR##pat}` / `${VAR#pat}` — prefix stripping
   The helper evaluates these against its inherited environment
   and any variables set by preceding lines in the same file.
   Dies on any disallowed form (`$(cmd)`, backticks,
   `${!indirect}`, arithmetic, etc.).
4. The helper outputs a trivial format: `KEY=value-to-newline`.
   No quoting, no escaping. Values cannot contain newlines.
5. The compiled binary reads the helper's stdout, sets the
   variables. If the helper exits nonzero, the binary dies too.

**Security argument:** The helper is a small, auditable Rust binary.
The shell is not involved in parsing the config file. If someone
puts code (functions, loops, commands) in the sourced file, the
helper dies loudly at runtime. The trivial output format means even
a buggy helper can't inject commands.

**Future:** Static linking of safebash modules for sourcing code
(functions, libraries). That's a separate feature — the kvp loader
handles the config use case.

### Decision 2: ${!var} indirect expansion — rejected

**Problem:** `${!var}` accesses a variable whose name is determined
by the runtime value of another variable. The compiler cannot know
which variable is being accessed without tracing every assignment
to the name variable.

**Resolution:** Reject. The indirection is opaque at compile time.
The variable name is a string that could come from user input, file
contents, or string concatenation. SSA analysis (phase 4) cannot
resolve the referent in the general case.

May revisit after SSA if a narrow safe case emerges (e.g., the name
variable is provably constant), but the default is reject.

### Decision 3: declare -n / local -n (namerefs) — allowed

**Problem:** Namerefs were initially rejected alongside `${!var}`
as another form of indirection. DG uses `local -n` extensively in
shell functions and considers it safe.

**Resolution:** Allow. Namerefs are fundamentally different from
`${!var}`:

- **`${!var}`**: The variable name is a *string value* at runtime.
  Any string could be in there. The binding is invisible.
- **`local -n ref=target`**: The referent is declared at the point
  of creation. The compiler sees exactly which variable `ref`
  points to. Even `local -n ref=$1` is analyzable at the call
  site — the compiler knows what argument was passed.

Namerefs provide the same indirection capability as `${!var}` but
in a structured form the compiler can reason about. The binding is
explicit, visible in source, and traceable through function call
graphs. It's equivalent to passing a pointer in C — the type system
can track it.

Rejecting namerefs while allowing no alternative would push users
toward eval, which is worse.

**Impact on experiment 007:** Remove `declare -n` / `local -n` from
the SB004 rejection rule. SB004 is retired or reassigned.

### Decision 4: $cmd variable-as-command — two-pass resolution

**Problem:** Rejecting all variable-as-command blocks legitimate
patterns like `readonly EDITOR=vi; "$EDITOR" file` and conditional
tool selection (`AWK=gawk` / `AWK=awk`). But allowing all of them
opens the door to runtime-constructed command names from arbitrary
data.

**Resolution:** Two-pass approach.

**Phase 2 (CST walking):** When `$cmd args` is encountered, look
backward in the same scope for an assignment to `cmd`. If the
assignment is `readonly cmd=literal` or `declare -r cmd=literal`
with no intervening reassignment visible in the CST, allow it.
Otherwise, emit a *deferred* marker — not an error yet, but a
signal that SSA must resolve this.

**Phase 4 (SSA):** Deferred dispatch variables get full data-flow
analysis. If SSA can prove the value set is bounded to literal
strings (e.g., both branches of an if/else assign literal values),
allow it. If SSA cannot prove it safe, hard error.

**Design consequence:** The validator framework (experiment 011)
needs the concept of *deferred judgments* — diagnostics that are
not errors at the CST level but become errors if SSA can't resolve
them. This is a three-state system: allow, reject, defer.

**Impact on SB001:** Changes from "reject all variable-as-command"
to "allow if provably literal at CST level, defer to SSA otherwise,
hard error if SSA can't resolve."

### Decision 5: Diagnostic code scheme — reuse ShellCheck, SB4xxx for new

**Problem:** The AI invented SB001–SB005 without consulting DG.
Need a principled code scheme.

**Resolution:** Reuse exact ShellCheck SC codes where safebash
enforces the same rule. Introduce SB codes only for safebash-
specific restrictions that ShellCheck doesn't cover.

**ShellCheck reuse:** When safebash rejects unquoted expansions,
it emits SC2086 — the same code ShellCheck uses. Users already
know these codes. No reason to invent new ones.

**SB4xxx for safebash-specific rules:** ShellCheck's scheme uses
1xxx (parse), 2xxx (code), 3xxx (portability). SB4xxx is a new
category: safe subset violations. It's clearly distinct from SC
codes and will never collide with future ShellCheck additions.

Current SB4xxx assignments:
- SB4001: eval forbidden
- SB4002: source/. compiled away (marker, not error)
- SB4003: ${!var} indirect expansion rejected
- SB4004: $cmd variable-as-command (deferred to SSA if not
  provably readonly+literal at CST level)

**Stability:** All codes (SC and SB) are permanent once assigned.
Users may reference them in ignore directives.

### Decision 6: source and . treated identically

In bash, `source` and `.` are synonyms. The only difference is
portability (`.` is POSIX, `source` is a bashism). Since safebash
targets bash syntax, the distinction is irrelevant. Both are
accepted and both compile to the same safe kvp loader mechanism.

### Decision 7: No minimum bash version — accept whatever tree-sitter-bash parses

The compiler doesn't invoke bash on the target — it compiles to a
native binary. The relevant question is not "what bash version runs
on the target" but "what syntax can the compiler accept." The
answer: whatever tree-sitter-bash can parse. No artificial version
floor.

### Decision 8: AOT strict by default, JIT coerce by default

**AOT (compile to signed binary):** Strict by default. Every
coercible issue is an error. Error messages include the flag to
enable coerce mode: `sash: SC2086: unquoted expansion at line 12
(use --coerce to auto-fix)`. The user sees every issue, fixes
deliberately, ships clean code.

**JIT (run interactively):** Coerce by default. The script runs.
Coercions are applied silently but reported as warnings so the
user knows what was fixed. Does not block execution.

**Workflow:** Develop with JIT (coerce), compile with AOT (strict)
for release. AOT errors show exactly what the coercer was silently
fixing during development.

Reject-only rules (eval, ${!var}, unresolvable $cmd) are always
hard errors in both modes. Coerce only applies to rules that have
a mechanical, semantics-preserving transformation.

### Decision 9: cd without error handling — coerce rule

**Problem:** `cd dir; rm -rf *` is the canonical "delete everything"
bug if `cd` fails. But inserting `|| exit 1` changes the program's
control flow — the author might have intentionally continued after
a failed cd.

**Resolution:** Coerce rule, not reject.

- **JIT (coerce by default):** Add `|| exit 1` and warn. The
  script doesn't silently continue in the wrong directory.
- **AOT strict (default):** Error: "cd without error handling at
  line N." The author chooses the fix: `|| exit`, `|| return`,
  or `|| { handle_error; }`. The compiler identifies the problem
  but doesn't choose the recovery strategy.

This reuses SC2164, not a new SB code.

---

