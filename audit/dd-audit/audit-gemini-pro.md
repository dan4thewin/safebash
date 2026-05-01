# Design Decision Audit — Gemini 2.5 Pro
2026-04-30

This audit addresses three questions: accuracy of the design decisions, contradictions between documents, and gaps in the safety model.

## Q1: Accuracy

The design decisions are largely accurate, but several are stale or misrepresent their implementation status.

1.  **Stale Decision Counts:** The prompt and documents frequently cite outdated numbers of DDs, tests, and auto-blessed items. The prompt mentions "44 design decision notes (DD#01-52)," which is mathematically impossible. The audit context provided 52 distinct DDs. `docs/GUIDE.md` claims "45 design decisions" and lists test counts (247, 116, 37, 59) that differ from the text's total (457 vs. 459). These inconsistencies suggest documentation is not updated automatically with the codebase.

2.  **DD#32 (Honest trace + embedded source):** Status is `partial`, noting `SASH_TRACE=1` is "designed, not built." This is a significant feature gap for a security-focused tool. A trustworthy trace is essential for verifying behavior, and its absence means a core part of the "honest trace" decision is aspirational, not implemented. The documentation should be clearer about what "partial" means here.

3.  **DD#41 (Sash-native array syntax):** Status is `partial`. It claims `@arr` for iteration and builtins like `len arr` are "designed, not fully wired." `docs/SUBSET.md` does not list this syntax, which is consistent, but it highlights that native, ergonomic features are lagging, forcing reliance on more verbose bash-compatible syntax.

4.  **DD#42 (getenv and argv as builtins):** Status is `designed`. The note states "Codegen supports them but not exposed as standalone commands yet." This is a critical gap. The `GUIDE.md` and `SUBSET.md` state `$(getenv VAR)` and `$(argv N)` *are* the way to access environment variables and arguments, implying they are fully implemented. The discrepancy is severe: the primary mechanism for introducing external data into the system is not actually available as documented. This makes the entire taint model, which relies on these functions as taint sources, questionable in its current practical application.

5.  **DD#19 (Runtime Function Selection via SSA Phi):** Status is `rejected`, with the rationale being "YAGNI — use `const int flag` + branch instead." This is a reasonable design choice, but the documentation provides no example of the alternative, leaving it to the user to derive the correct, safe pattern.

## Q2: Contradictions

Several contradictions exist between the design decisions and the specification/architecture documents.

1.  **Subshell Rejection vs. Acceptance:** `DD#38` states unequivocally: "`( cmd )` rejected." However, `docs/SUBSET.md` lists `( cmd )` under "Accepted Constructs," with the caveat "subshells (limited, prefer explicit)." This is a direct contradiction. Which is it? If subshells are accepted in any form, `DD#38` is wrong and the limitations need to be precisely defined. If they are rejected, `docs/SUBSET.md` is wrong. This ambiguity is dangerous for a security tool.

2.  **Taint Propagation:** The `SUBSET.md` Taint Model states: "There is no propagation: tainted values cannot be copied, concatenated, indexed with, or otherwise read except inside a `bless` test expression." However, `DD#50 (Function free-read tracking)` describes a form of inter-procedural taint analysis: "At call sites, check whether any of those globals are currently tainted." This implies a global variable's taint *does* flow or affect the analysis of a function call, which contradicts the "no propagation" claim. The model is more complex than the simple per-definition classification described.

3.  **`$(( ))` Behavior:** `DD#31` states: "`$((x=10))` does NOT update `x` — this is intentional, not a bug." This is a radical, correctness-breaking departure from decades of shell behavior. While intentional, it is deeply counter-intuitive and violates the principle of least astonishment. More importantly, `docs/SUBSET.md` lists `$(( expr ))` as "Arithmetic expansion" without mentioning this critical semantic divergence. A user reading the subset spec would be misled into writing buggy code. This is a documentation failure that borders on sabotage of the user's intent.

4.  **`unset` Behavior:** The `SUBSET.md` lists `unset` as a supported builtin, claiming it "assigns empty." Bash's `unset` removes a variable entirely. This coercion is non-obvious and could lead to incorrect logic, as a script writer might expect `unset` to make `${var:-default}` expansions trigger. As sash has no concept of "unset," the command should probably be rejected (like `export`) rather than silently redefined.

5.  **Glob Specificity:** `DD#39` specifies "Star-only glob for files (`*` only, no `?` or `[abc]`)." The `SUBSET.md` is weaker: "`*` only for file matching — `?` and `[...]` restricted (DD#39)." "Restricted" is not "rejected." This ambiguity needs to be resolved. Does it mean they are sometimes allowed? Or are they rejected entirely? The documentation must be precise.

## Q3: Gaps and Flaws

The design has several gaps that could undermine its safety goals. The taint model, while strong, has exploitable ambiguities.

1.  **Taint Laundering via `bless`:** `DD#49` rejects tautological `bless` where the variable is not present in the test, e.g., `bless tainted_var [[ 1 -eq 1 ]]`. This is a good first step, but it is purely syntactic and easily bypassed. An attacker can write `bless tainted_var [[ "$tainted_var" == "$tainted_var" ]]`. This expression is always true, contains the variable, and satisfies the rule, but performs no actual validation. This allows arbitrary tainted data to be laundered into a clean state, completely bypassing the quarantine. The `bless` mechanism needs semantic validation of the test expression's utility, not just syntactic checks.

2.  **Taint Laundering via Arithmetic Context:** `DD#43` (amendment) and `DD#24` state that `(( ))` is a read context, and using a tainted variable within it is a compile error. This is the correct approach. However, `DD#31` splits arithmetic into mutation `(( ))` and comparison `if (( ))`, forbidding crossover. This design, combined with the non-mutating nature of `$(( ))`, opens a potential side-channel. Could a user do `if (( $(($tainted+0)) > 0 )); then ...`? If `$tainted` is not a valid integer, the arithmetic expansion might fail, altering control flow. This leaks information about the tainted variable's type without a proper `bless`. The interaction between arithmetic contexts and taint needs a more rigorous, adversarial review.

3.  **Incomplete Taint Sources:** The `SUBSET.md` lists taint sources as `getenv`, `argv`, `read`, `mapfile`, and external `$(cmd)`. It omits process-related information that can be influenced by an attacker. For example, `$$` (process ID) and `$!` (last background job PID) are listed as clean sources in the `GUIDE.md`. While often innocuous, in a context where an attacker can influence process creation order or timing, these values could be used as a low-bandwidth channel for malicious input. A truly paranoid design would consider all unpredictable runtime values as tainted.

4.  **The `known_externals` Threat:** `DD#24` and the `GUIDE.md` describe an "auto-bless" feature for 25 commands and 26 environment variables based on "conservative regex patterns." This is a massive, built-in attack surface. Any flaw in any of those regex patterns could allow tainted data to be auto-blessed. For example, if `date`'s output is trusted, but an attacker can control the system's locale such that `date` produces unexpected characters, could it bypass the regex? The entire list of patterns should be published and subject to intense scrutiny. A feature intended for convenience significantly weakens the core security promise of explicit validation.

5.  **External Command Execution Blind Spot:** The `GUIDE.md` notes: "safebash does not inspect arguments to `xargs`, `find -exec`, `parallel`, etc. The set of programs capable of executing arbitrary code is unbounded." This is a pragmatic but dangerous limitation. The primary threat of shell injection is passing tainted data to a command that then executes it. While safebash prevents a tainted variable from becoming a command *name*, it does little to prevent it from being a malicious *argument*. The model seems to be: "We ensure `rm` is called, but if you pass `"$tainted_var"` which contains `-rf /`, that's your problem." This is a significant gap in the safety narrative. The `bless` mechanism is the only defense, and as noted, it is currently weak.
