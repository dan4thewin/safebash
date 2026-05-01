# Design Decision Audit — GPT 5.4
2026-04-30

## Executive verdict

The design-decision corpus is not clean. It contains stale counts, false claims of implementation, partial amendments that were never propagated into the surrounding documents, and a bunch of architecture/spec prose that still describes a pre-amendment world. The result is exactly the sort of documentation drift that gets people to misclassify bugs as features and features as bugs.

The worst problems are not stylistic. They are trust-boundary problems:

- the taint model is described inconsistently across DDs, `docs/SUBSET.md`, and `docs/GUIDE.md`;
- the command/function capture boundary is described in ways that can mislead auditors about what is or is not tainting;
- top-level argv/positionals are described differently in different places;
- the subset spec flatly claims support for constructs that the DDs explicitly reject;
- the architecture guide still documents runtime/env/PATH behavior that directly conflicts with multiple decisions.

If safebash is supposed to support a chain of trust, this documentation set is currently undermining it.

---

## Q1 — ACCURACY

### 1. The prompt itself is already stale about the corpus size

The prompt says “All 44 design decision notes (DD#01-52).” That is numerically incoherent on its face. The embedded corpus contains DD#01-09 as a bundled note plus DD#10 through DD#52, i.e. 44 files/entries if you count the bundle as one note, but 52 numbered decisions if you count decisions. That may sound trivial. It is not. This is exactly the kind of sloppy counting that later turns into “we audited all decisions” when in reality people audited all files, not all decision points.

### 2. DD#15 is overstated and not accurately reflected elsewhere

**DD#15** says: “The Janet backend is the reference implementation for what sash programs should do. Discrepancies between native and Janet output are bugs.”

That is far too absolute compared with the rest of the corpus.

- `docs/GUIDE.md`, “Backends → Janet,” softens this into “The Janet oracle is the normative semantics of safebash” and immediately admits disagreement requires deciding “which behavior does safebash want, and why?”
- That wording implicitly concedes Janet is not self-authenticating truth; it is an oracle used to pin semantics.

The DD is therefore inaccurate in its absolutism. If the actual rule is “Janet is the oracle unless design intent says otherwise,” then DD#15 is not the final state. If the actual rule is “Janet always wins,” then the architecture guide is lying. Pick one. Right now the project has both.

### 3. DD#17 is at best incomplete and at worst false as stated

**DD#17**: “`$(func args)` is the universal call syntax for function invocation. Works for both string-returning and int-returning functions.”

Problems:

- The subset spec, `docs/SUBSET.md` under “Functions,” still presents ordinary shell function syntax and positional semantics, but it does not explain how non-capture invocation and capture invocation coexist with the “universal” claim.
- The architecture guide does not elevate DD#17 into the language model discussion at all.
- More importantly, DD#17 is stated as if function invocation syntax is fully settled and uniformly exposed. But **DD#42** admits `getenv` and `argv` codegen support exists while standalone command exposure does not. That shows surface syntax and backend ability are not aligned.

If “universal” means “one syntax for obtaining a function result as a string in expression position,” say that. As written, it reads like the entire language has a single invocation syntax. The rest of the docs do not support that reading.

### 4. DD#20 is incomplete because it was not fully amended after later decisions

**DD#20** says shell options are compile-time decisions and lists `nullglob`, `lastpipe`, `set -e`, `set -u`, `pipefail`, with `extglob` superseded by **DD#39**.

This is incomplete in at least two ways:

- `docs/SUBSET.md`, “Lists and Subshells,” still claims `( cmd )` subshells are accepted in limited form, which conflicts with **DD#38**. If shell execution semantics are compile-time decisions, then the decision corpus should explicitly point at other fixed semantic defaults that matter as much as `lastpipe` and `nullglob`—especially where later DDs reject formerly shell-like behavior.
- `docs/GUIDE.md`, “Compilation Pipeline → Taint,” still lists positional params as `$1`–`$N` in a generic way, instead of reflecting **DD#46**’s top-level ban and coercion model. That is another semantic “shell option” class divergence not brought back into the umbrella decision.

DD#20 is not factually wrong in isolation, but it is incomplete as a “final state” record because later semantic hardening was not reincorporated.

### 5. DD#24 is not a stable final statement because DD#43 and DD#44 materially changed it

**DD#24** tries to be the main taint decision, but it is no longer a reliable one-stop statement of the final model.

What is right:

- It acknowledges amendment by **DD#43** (“assign then validate”) and **DD#44** (“all use sites”).
- It states arithmetic contexts are read contexts.

What is not adequate:

- It still compresses the model into a few slogans while `docs/SUBSET.md`, “Taint Model,” adds critical details that are not in DD#24: no propagation, per-definition classification, clean heredoc/here-string reads, user-defined function capture being trusted, and container-element taint.
- `docs/GUIDE.md`, “Phase 3: Taint,” contradicts/amends it again by describing state as `Tainted`, `Blessed`, or `Clean`, and by claiming `if bless` false-branches on failure, which **DD#47** does not say. DD#47 says bless failure writes an error and exits 1.

That means DD#24 is not actually the final authoritative statement. It is a stale trunk with amendments stapled onto it while major semantics escaped into docs prose.

### 6. DD#26 is likely stale on allocator characterization

**DD#26** says “FreeRTOS heap_4 Allocator” and describes it as a bump allocator with no fragmentation.

That characterization is suspect on its face. `heap_4` is the coalescing allocator in FreeRTOS, not the trivial monotonic bump allocator. Even if the project wraps or simplifies it, the DD as written asserts a property that is probably false in the upstream allocator model. The architecture guide compounds this by calling `sash_alloc` a “bump allocator” in `docs/GUIDE.md`, “Runtime → Memory.”

If the runtime really uses a pure bump allocator, then DD#26 is mislabeled. If it really uses `heap_4`, then both DD#26 and the guide are misstating its behavior. Either way, something here is factually wrong.

### 7. DD#27 is incomplete about the cost and semantic consequences of the trampoline model

**DD#27** describes functions as blocks in a single `_start` function and emphasizes inlining and no ABI overhead.

What it fails to capture:

- the interaction with **DD#38** (forking user functions with `&` by jumping to trampoline blocks in the child);
- the interaction with command substitution capture semantics and the later identified task “pipe-free user-function capture”;
- the implications for free-read taint analysis in **DD#50**.

This matters because the architecture guide still describes SSA as if there are “functions, each containing `Vec<BasicBlock>`” (`docs/GUIDE.md`, “SSA”), which sounds like ordinary function-level SSA, not one giant trampoline execution model. That is not merely a missing detail. It can mislead anyone reasoning about call boundaries, taint boundaries, and stack behavior.

### 8. DD#29 / DD#45 are internally consistent enough, but the rest of the docs failed to catch up

**DD#29** and **DD#45** say coercion happens in the lowerer, source emission operates from CST with byte-position tracking, and `.sash_source` / `--emit-source` should match.

The problem is not inside those DDs. The problem is that the architecture guide still frames validation and coercion in a way that lets stale rules appear alive:

- `docs/GUIDE.md`, “Phase 2: Validate,” says coercion rules were removed and only subset gate rules remain. Fine.
- But `docs/SUBSET.md`, “Tests,” says `[ ]` is “rejected (coerced to `[[ ]]`)”. That is self-contradictory. Rejected and coerced are not the same thing.
- `docs/SUBSET.md`, “Command Substitution,” says backticks are “rejected (coerced to `$(cmd)`)”. Same problem.

So the DDs are closer to right than the spec, but the claimed “current implementation” document is wrong.

### 9. DD#31 is accurate but insufficiently defended against predictable misreadings in the docs

**DD#31** is explicit and pinned: mutation and comparison arithmetic are disjoint; `$((x=10))` does not update `x`; statement `(( ))` only mutates; condition `if (( ))` only compares.

The problem is not the DD itself. The problem is that the rest of the docs barely carry this semantic bomb.

- `docs/SUBSET.md`, “Arithmetic,” just lists `$(( expr ))` and `(( expr ))`, which reads like Bash-ish arithmetic support. That is dangerously incomplete.
- `docs/GUIDE.md`, “Transform,” mentions arithmetic evaluation but does not flag that arithmetic semantics intentionally diverge from Bash.
- `notes/pinned.md` has to scream “DD#31 is intentional” because the public docs are too vague.

A design decision that is routinely mistaken for a bug and requires a pinned warning is a documentation failure. The DD is accurate; the corpus is not complete in capturing final state.

### 10. DD#32 is partial, and the architecture guide overstates the current implementation

**DD#32** says embedded source is implemented, runtime trace is designed not built.

`docs/GUIDE.md`, “Source emission,” accurately describes `--emit-source`, `--annotate`, `--extract`, but the earlier “Chain of Trust” and architecture narrative never distinguish implemented provenance features from unimplemented runtime transparency features. That matters because “honest trace” is half the point of the DD title.

The DD itself is honest (`partial`), but the surrounding docs do not make the missing half visible enough.

### 11. DD#33 is not fully reconciled with its own details

**DD#33** says arrays use string keys always, gives `arr[5] -> key "5"`, and then says integer keys are hashed as raw i64, not converted to strings.

That is not impossible, but it is underspecified and easy to read as contradictory:

- Is the logical key model string-only, with an optimization for integer-key hashing?
- Or do indexed arrays use a separate integer-key path while still exposing string semantics?

The architecture guide, `docs/GUIDE.md`, “Arrays,” just says `SashTable` is used for both indexed and associative arrays. It does not resolve the semantic model.

DD#33 therefore captures an implementation trick but not a clean final semantic statement.

### 12. DD#39 is incomplete relative to the spec and known engine limitations

**DD#39** says star-only glob for files, Perl-style regex mutation, expression form and statement form.

Missing or stale aspects:

- `docs/SUBSET.md`, “Tests,” still claims unquoted RHS in `==` gives glob matching, but does not state clearly that glob syntax is reduced to `*` only under DD#39.
- `notes/pinned.md` records a known blocked limitation: Plan 9 regex lacks `{n,m}` quantifiers. Neither DD#39 nor `docs/SUBSET.md`, “Sash-Native Regex,” mention this. That omission matters because the spec otherwise reads like regex support is just there. It is not.

If a regex engine limitation is known, user-visible, and can cause silent non-matches, then DD#39 is incomplete as final-state documentation.

### 13. DD#40 is accurate in spirit, but the architecture guide is still polluted with validator-era assumptions

**DD#40** says ShellCheck is the lint gate; validator is subset enforcement only; quoting rules are irrelevant because `$var` and `"$var"` are identical.

The guide partially matches this in “Phase 2: Validate,” but then the broader document continues to describe Bash-ish accepted constructs too loosely. For example:

- `docs/GUIDE.md`, “IR Statement types,” includes `Subshell`, despite **DD#38** rejecting subshells.
- `docs/GUIDE.md`, “Runtime → Process management,” still says `sash_exec` does PATH lookup, conflicting with **DD#23**.

That is not a DD#40 problem per se. It is a project-wide failure to propagate the “subset enforcement only” worldview into the architecture narrative.

### 14. DD#41 is partial, but the spec presents an unhelpfully mixed surface language

**DD#41** says `array` and `map` declarations are implemented, while `@arr`, `len`, `keys`, `join` are designed/not fully wired.

`docs/SUBSET.md` includes declarations, but the taint section also refers to `join @arr` as though that form is semantically available enough to specify taint rejection. That may be acceptable if partially implemented, but the public “current implementation” claim becomes dubious when designed syntax is used in normative semantics text.

This is exactly how readers get baited into relying on syntax that is not actually wired.

### 15. DD#42 is explicitly designed-not-built, but the rest of the docs keep treating it as if built

This is a major accuracy problem.

**DD#42** says `getenv` and `argv` are designed, codegen supports them, but they are “not exposed as standalone commands yet.” Status: designed.

But:

- `docs/SUBSET.md`, “Taint Sources,” uses `x=$(getenv VAR)` and `x=$(argv N)` as normative syntax, not tentative syntax.
- `docs/GUIDE.md`, “Safety Model,” says environment variables are accessed via `$(getenv NAME)`, positional params via `$(argv N)`.
- `docs/GUIDE.md`, “Phase 3: Taint,” treats them as implemented taint sources.
- **DD#46** says bash-mode top-level positionals are coerced to `$(argv N)` and top-level guidance tells users to “use `argv N`.”

Those documents do not describe a designed future. They describe the present language.

So one of two things is true:

1. DD#42 is stale and should be `implemented`, or
2. the spec/guide are lying about the current language.

That is not a tiny mismatch. `argv`/`getenv` are core to the explicit-origin safety story.

### 16. DD#43, DD#47, and the guide disagree about bless control-flow semantics

This is one of the ugliest accuracy failures.

- **DD#43** defines assign-then-validate semantics for `bless`.
- **DD#47** says bless is a first-class SSA instruction; on failure it writes “bless: validation failed” to stderr and exits 1.
- `docs/GUIDE.md`, “Phase 3: Taint,” says:
  - bare `bless` → fatal exit
  - `if bless` → false branch taken

Those are not the same semantics.

If `bless` can be used as a boolean in condition position, that must be a decision. If it always exits on failure, then the guide is wrong. If there are two forms, neither DD#43 nor DD#47 documents them clearly enough.

Given the explicitness of DD#47, the guide is probably stale. But the corpus as a whole is inaccurate until this is reconciled.

### 17. DD#44 and `docs/SUBSET.md` disagree in emphasis about propagation versus per-use rejection

**DD#44** says “No propagation, only per-definition classification.”

`docs/SUBSET.md`, “Taint Model,” elaborates that tainted values cannot be copied, concatenated, indexed with, or otherwise read except inside `bless`, which is consistent with no propagation because reads are blocked.

The problem is `docs/GUIDE.md`, “Phase 3: Taint,” which still describes variables tracked as `Tainted`, `Blessed`, or `Clean` in a sequential flow-sensitive analysis. That wording invites the exact wrong mental model: ordinary taint propagation over variable state.

This is not an outright contradiction, but it is inaccurate enough to cause mistaken reasoning. Given how central DD#44 is, the guide should speak in terms of definition classification and use-site rejection, not generic “flow-sensitive taint tracking” slogans that sound like conventional propagation.

### 18. DD#45 is cited incorrectly in the guide

`docs/GUIDE.md`, “Phase 2: Validate,” says `export_rejected.rs` catches SB5001 “(DD#45)”. That is wrong. `export` rejection is in **DD#01-09** item 4. **DD#45** is source emitter architecture.

This is not a semantic difference; it is a plain factual documentation error.

### 19. DD#46 / DD#48 are not accurately reflected in the architecture guide’s taint source list

**DD#46** says top-level `$1..$N`, `$@`, `$*`, `$#` are function-local only in sash-native mode; top-level references are compile errors with coercion to `$(argv N)` in bash mode.

**DD#48** adds out-of-range positional detection.

But `docs/GUIDE.md`, “Phase 3: Taint,” still lists positional params (`$1`–`$N`, `$@`) as taint sources without that critical distinction. That is no longer the final state.

This matters because positional semantics are part of the trust boundary: whether a value originates from program argv or function call args is exactly what the design is trying to make explicit.

### 20. DD#50 and DD#51 are undersold relative to their importance

**DD#50** (function free-read tracking) and **DD#51** (SSA taint verification airbag) are solid and important, but neither `docs/SUBSET.md` nor the architecture guide gives them the prominence they deserve.

That is an accuracy problem because the docs make the taint model sound simpler than it is. In reality, the project had to add interprocedural free-read checks and a second-pass ICE airbag to make the taint guarantees believable. Hiding that complexity behind smooth prose is dishonest architecture documentation.

### 21. DD#52 is not properly integrated into the subset spec’s parameter expansion surface

**DD#52** rejects `${var@...}` transforms and directs users to `${var^^}` / `${var,,}` instead.

`docs/SUBSET.md`, “String Operations,” includes `${var^^}`, `${var,,}`, `${var^}` but does not explicitly mention the rejected `${var@...}` family in the accepted/rejected surface area beyond the generic rejection table. That is survivable, but given how similar those syntaxes look, the omission makes the spec less complete than it should be.

---

## Q2 — CONTRADICTIONS

### A. The subset spec flatly contradicts DD#38 on subshells

- **DD#38**: “`( cmd )` rejected.”
- `docs/SUBSET.md`, “Lists and Subshells”: “`( cmd )` — subshells (limited, prefer explicit).”
- `docs/GUIDE.md`, “IR Statement types”: includes `Subshell`.

This is not nuanced. It is a direct contradiction.

If subshells are rejected, the subset spec is wrong and the architecture guide is documenting dead/legacy IR surface. If they are accepted in limited form, DD#38 is false. There is no way to reconcile these three texts without admitting one or more are stale.

### B. DD#23 contradicts the architecture guide’s runtime environment model

- **DD#23**: PATH is compile-time only; scripts cannot modify PATH at runtime; external command lookup is resolved at compile time.
- `docs/GUIDE.md`, “Runtime → Process management”: `sash_exec — PATH lookup, fork/exec.`
- `docs/GUIDE.md`, “Runtime → Environment”: `sash_setenv` exists.

That is an architectural contradiction.

If PATH lookup happens in `sash_exec`, then either compile-time resolution is false or runtime fallback exists and DD#23 is overstated. If PATH is compile-time only, the guide is describing a capability that should either be absent or severely constrained.

Worse, the guide’s mention of `sash_setenv` sits badly next to **DD#23** and the whole explicit-origin model. If scripts can mutate environment state at runtime, what are the semantics for later external calls? Is that allowed for non-PATH vars only? The guide does not say. The decision corpus does not say. That is a contradiction-shaped hole.

### C. DD#42 / DD#46 contradict DD status and current-doc claims

- **DD#42** status: designed.
- **DD#46** depends on `argv N` as coercion target and user guidance.
- `docs/SUBSET.md` and `docs/GUIDE.md` describe `$(argv N)` / `$(getenv VAR)` as current accepted constructs.

Either the feature is implemented enough to be part of the present language, or it is not. The corpus currently says both.

### D. Bless failure semantics contradict between DD#47 and `docs/GUIDE.md`

- **DD#47**: bless failure prints an error and exits 1.
- `docs/GUIDE.md`, “Phase 3: Taint”: in `if bless`, false branch is taken.

Both cannot be true unless there are multiple forms with different operational semantics, and no document cleanly states such a split.

### E. The subset spec contradicts itself on coercion vs rejection

Examples:

- `docs/SUBSET.md`, “Tests”: `[[ ]]` only — `[ ]` is rejected (coerced to `[[ ]]`).
- `docs/SUBSET.md`, “Command Substitution”: backticks rejected (coerced to `$()`).
- Rejected-constructs table repeats the same nonsense.

Rejected and silently coerced are mutually exclusive user-facing behaviors. If the compiler accepts the construct and rewrites it, it is not rejected. If it emits a compile error, it is not silently coerced.

The DDs are clear that these are coercions (**DD#29**, **DD#45**, early decisions item 8/9 context). The subset spec is internally incoherent.

### F. DD#40’s “validator reduced to subset enforcement only” conflicts with the guide’s frozen rule list

`docs/GUIDE.md`, “Phase 2: Validate,” lists only SB4001, SB4002, SB5001, SB5002. But the docs elsewhere refer to additional validator-like errors for taint and other rejections (e.g. SB4012 in **DD#49**, SB4015 in **DD#52**, SB5004 in **DD#37**).

This is not necessarily a contradiction if those later diagnostics come from other phases, but the guide does not draw that line clearly. The result is a false impression that the validator is the complete enforcement surface, when the design decision corpus plainly spreads enforcement across validation, taint, typing, and SSA verification.

### G. DD#15’s “Janet defines semantics” conflicts with the guide’s more nuanced stance

Already noted under accuracy, but it is also a contradiction:

- **DD#15** says discrepancies are bugs.
- `docs/GUIDE.md`, “Backends → Janet,” says disagreement triggers a design question about intended behavior.

Those are different governance models.

### H. The guide’s “current implementation” framing contradicts partial/designed DDs it treats as present

The subset spec says it “reflects the current implementation, not aspirational rules.” The architecture guide similarly presents current architecture.

But both documents lean on features whose DDs are explicitly **partial** or **designed**:

- **DD#32** trace not built.
- **DD#41** `@arr`, `len`, `keys`, `join` not fully wired/designed.
- **DD#42** `argv` / `getenv` not exposed as standalone commands yet.

If those surface forms are truly part of the current language, the DD statuses are stale. If not, the docs are aspirational despite claiming not to be.

### I. DD#33’s semantic model for array keys is internally muddy enough to verge on contradiction

Again:

- “String keys always”
- but “Integer keys hashed as raw i64, not converted to strings.”

Without a sharper explanation, that is half semantic claim, half implementation shortcut, and they are fighting each other.

### J. The architecture guide’s repo/test counts are stale against the prompt and project hints

The guide says “45 design decisions documented in `docs/JOURNAL.md`,” but the prompt contains DD#01-52 in 44 notes/files. The project hints also talk about 52 design decisions in `notes/decisions/`. The guide is plainly stale.

Likewise test counts differ between embedded prompt sections and project hints. Count drift is normal. Leaving stale counts in an architecture guide while presenting them as facts is not.

---

## Q3 — GAPS

### 1. The PATH/environment model is not fully designed, and that is dangerous

This is the biggest architectural gap after taint.

The project goal is explicit data boundaries and compile-time resolution of dangerous ambiguity. Fine. But then the docs leave basic environment semantics half-specified:

- **DD#23** bans runtime PATH mutation and says lookup is compile-time.
- The architecture guide still documents runtime PATH lookup and `sash_setenv`.
- The subset spec still lists `command` builtin support and `unset` semantics.

Missing decisions:

- Are non-PATH environment mutations allowed before exec of externals?
- If yes, are they tainted, trusted, or simply opaque side effects?
- Are per-command env prefixes (`FOO=bar cmd`) the only allowed outbound env mutation mechanism, as early decision 4 implies by replacing `export`?
- Does runtime `unset` for environment-backed names have any effect on external process environments?
- How does compile-time command resolution interact with `command -p`, `env`, or per-command PATH assignment?

This is not pedantry. Environment handling is command execution policy. Leaving it vague sabotages the chain-of-trust story.

### 2. The trusted-function rule in the taint model is under-justified and fragile

`docs/SUBSET.md`, “Taint Model,” says user-defined sash functions are trusted and therefore `$(user_func ...)` capture is clean, because a function that touches taint must bless before use or fail to compile.

That is elegant, but it needs stronger explicit justification in the decisions. Right now the burden is carried by **DD#50** and **DD#51**, plus the implementation architecture of **DD#27**. There is no single decision explicitly stating the declassification boundary for user-function capture and the proof obligations needed to make that trust claim valid.

That is a gap because command substitution is one of the main taint sources. The system sharply distinguishes external command capture (tainted) from internal function capture (clean), and that distinction absolutely deserves its own formal decision. Right now it is a derived property scattered across docs.

### 3. The bless mechanism still lacks a fully explicit semantic contract

You have at least four moving pieces:

- **DD#43**: assign then validate
- **DD#47**: runtime enforcement, exit on failure
- **DD#49**: tautology rejection
- `docs/GUIDE.md`: possible boolean-ish `if bless` semantics

What is missing is one canonical decision answering all of these:

- Is `bless` a statement only, or also a condition form?
- If used in control flow, does failure exit or return false?
- Is the variable considered assigned but tainted before runtime validation runs?
- What is the exact SSA declassification point?
- Are compound validations required to be side-effect free?
- What happens if the test expression itself touches external commands, regex failures, file tests, or arithmetic conversion traps?

Given that `bless` is the only legal way to cross the taint boundary, this lack of one hard, complete contract is a bad gap.

### 4. No decision squarely addresses dangerous external programs despite explicit acknowledgment in the spec

`docs/SUBSET.md`, “Out of Scope,” openly says safebash does not inspect arguments to `xargs`, `find -exec`, `parallel`, etc., and that the set of dangerous programs is unbounded.

That honesty is good. The lack of a corresponding design decision is bad.

Why? Because this is not peripheral. It goes to the heart of goal (2): “eliminate classes of bash bugs by construction.” The project cannot eliminate dangerous execution composition if it deliberately refuses to model it. Fine — but then there should be a decision that states the boundary of responsibility and why this limitation does not invalidate the safety claims.

Without that, the docs read like they want credit for safety while disclaiming one of the biggest remaining attack surfaces.

### 5. The regex/glob story is underdesigned relative to user-visible semantics

**DD#39** picked a sharp direction, but the details are still muddy:

- star-only glob for files,
- glob-to-regex coercion for parameter replacement,
- Perl-style regex mutation syntax,
- Plan 9 regex engine limitation on `{n,m}` quantifiers,
- `[[ ... == pattern ]]` still using glob matching.

This is a semantic swamp unless someone writes the full contract.

Questions the current decisions do not answer clearly enough:

- Is the star-only restriction universal to all glob contexts or only filename expansion and certain pattern operators?
- What exactly is the accepted glob subset in `${var/pat/rep}` after coercion?
- Are regex syntax/flags intentionally limited to what Plan 9 can support, or is the language spec bigger than the engine?
- What failure mode is intended for unsupported regex syntax: compile error, runtime error, silent no-match?

Given the pinned note about silent failure on counted repetition, the current state actively sabotages trust. Silent no-match on unsupported syntax is exactly the kind of thing this project should not tolerate.

### 6. The arithmetic model still lacks enough design around conversion boundaries and traps

You have **DD#16**, **DD#25**, **DD#30**, **DD#31**, and **DD#36**, but they do not yet form a complete arithmetic story.

Open gaps:

- **DD#25** overflow traps are only “designed.” The subset spec still presents arithmetic as an accepted construct without caveating runtime trap semantics.
- The exact accepted expression grammar for statement `(( ))`, condition `if (( ))`, and expansion `$(( ))` is not fully specified in the subset spec.
- It is unclear how string→int conversion failures are surfaced at runtime versus compile time at different boundaries.
- Fixed-point design (**DD#36**) exists but is not integrated with the language contract.

For a language that prides itself on removing Bash ambiguity, arithmetic remains under-specified.

### 7. There is no explicit decision covering shell builtins that mutate process state versus safebash’s compilation model

The subset spec lists builtins including `trap`, `cd`, `unset`, `shift`, `command`. The architecture guide lists SSA instructions including `Trap`, and runtime functions around env and exec.

But where is the decision that says which shell builtins are actually part of the trusted semantic core and why? You have one-off decisions rejecting `export`, `source`, subshells, etc., but no coherent policy for stateful shell builtins.

That gap matters because builtins are exactly where shell languages hide weird ambient state.

### 8. There is no clear decision on whether the spec or the implementation is authoritative when they diverge

This sounds bureaucratic. It is not.

Right now:

- DDs are supposed to capture intent.
- `docs/SUBSET.md` claims to reflect current implementation.
- `docs/GUIDE.md` explains architecture.
- The Janet backend is called normative in one place and merely oracle-guiding in another.

When these disagree, what wins? The project currently has no documented precedence order robust enough to stop confusion.

There should be a hard rule, something like:

1. Decision notes define intended semantics.
2. Tests define implemented semantics where decisions are silent.
3. Spec/guide summarize and must be updated to match.
4. Janet is an oracle, not an independent legislature.

Without that, the corpus is guaranteed to keep drifting.

### 9. The source-embedding / chain-of-trust story is incomplete without a decision on binary/source identity claims

**DD#32** and **DD#45** together imply a provenance story: embed source, extract source, trace behavior. The architecture guide opens with chain-of-trust and signed binaries.

But there is no strong decision on what exactly the embedded source proves:

- Is `.sash_source` the coerced canonical source, not the original input bytes?
- Is byte identity between `--emit-source` and `.sash_source` part of the trust model or just a convenience?
- Is signing intended to attest to the coerced program, the original Bash input, or both?
- What prevents source-extraction ambiguity when coercions normalize semantically distinct-but-accepted Bash input into the same sash-native source?

If the goal is tamper-evident signed binaries, these questions are not optional.

### 10. The project is still underdocumenting known implementation constraints as language constraints

Examples:

- regex counted repetition unsupported (`notes/pinned.md`),
- `[[ -v var ]]` empty-string edge case (`notes/pinned.md`),
- possible allocator/runtime model ambiguity,
- partial support for sash-native conveniences in **DD#41**.

These need to be either:

- elevated into design decisions if intentional,
- put into the subset spec as explicit current limitations if accidental-but-known,
- or fixed.

Leaving them in pinned notes while the public spec speaks in cleaner abstractions is exactly how safety systems lie to users.

---

## High-priority findings

### Priority 0 — immediate contradictions that need repair

1. **Subshells:** reconcile **DD#38** vs `docs/SUBSET.md` “Lists and Subshells” vs `docs/GUIDE.md` IR `Subshell`.
2. **Bless failure semantics:** reconcile **DD#47** with `docs/GUIDE.md`, “Phase 3: Taint.”
3. **PATH/runtime env model:** reconcile **DD#23** with `docs/GUIDE.md`, “Runtime → Process management/Environment.”
4. **`argv`/`getenv` status:** reconcile **DD#42** with **DD#46** and with the subset/guide claiming current support.
5. **Coercion vs rejection language in `docs/SUBSET.md`:** stop saying constructs are both rejected and silently coerced.

### Priority 1 — trust-model documentation failures

6. Write a single canonical taint/bless semantics note consolidating **DD#24**, **DD#43**, **DD#44**, **DD#47**, **DD#49**, **DD#50**, **DD#51**.
7. Add an explicit decision for internal-function capture trust boundaries and why `$(user_func)` is clean while `$(external cmd)` is tainted.
8. Add an explicit decision defining authority order between DDs, tests, spec, guide, and Janet.
9. Document environment mutation semantics and command resolution policy as one coherent model.

### Priority 2 — stale or sloppy factual claims

10. Fix the wrong DD citation for export rejection in `docs/GUIDE.md`.
11. Fix stale decision/test counts in `docs/GUIDE.md`.
12. Clarify allocator description in **DD#26** and the guide.
13. Document regex engine limits under **DD#39** / `docs/SUBSET.md`.
14. Carry **DD#31** semantics into the subset spec instead of leaving it as a tripwire for anyone expecting Bash arithmetic.

---

## Bottom line

The decision set is not garbage, but it is not clean enough to support the confidence level safebash wants to project. The deepest problem is not that there are some stale docs. The deepest problem is that the project’s core claims — explicit origins, taint quarantine, static command resolution, and trustworthy compiled semantics — are described inconsistently across the very documents that are supposed to establish them.

That is how security projects rot: not by one dramatic bug, but by letting the intent/spec/implementation triangle drift until nobody knows which layer is lying.
