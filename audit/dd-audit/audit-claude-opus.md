# Design Decision Audit — Claude Opus 4.6 (primary agent)

Date: 2026-04-30

## Methodology

Compared all 44 files in `notes/decisions/` against source material:
- Full journal (4886 lines across 47 files)
- `docs/GUIDE.md` (408 lines)
- `docs/SUBSET.md` (295 lines)
- `docs/TIMELINE.md` (169 lines)
- `AGENTS.md`

Three questions:
1. Are decisions accurately and completely captured in their final state?
2. Are there contradictions or incoherencies?
3. Are there gaps or flaws where decisions fail to cover or sabotage safebash goals?

---

## 1. Accuracy and Completeness

### Inaccuracies found

**DD#01-09 item 3 — Namerefs.** The decision note says "compile-time
substitution in lowerer (designed, not built)." But the audit run-2
added SB5003 which *rejects* `declare -n` / `local -n`. The decision
note should reflect rejection, not "designed substitution." The journal
(046-session-audit-run2.md, T8) confirms rejection. Status should note
SB5003.

**DD#01-09 item 4 — Export.** Listed as an early decision but export
rejection was formalized later as SB5001 in DD#45 amendment. The early
decision was about `declare -x`; the full story includes the DD#45
amendment that made `export` itself an error. Cross-reference is missing.

**DD#20 — Shell options.** The note says "extglob superseded by DD#39."
But the journal (DD#20 amendment in 042-phase2-dd29-45.md) has extensive
detail on globstar, nullglob, failglob, dotglob that the decision note
omits. The final state is: extglob rejected, globstar rejected, nullglob
moot (arrays handle it), failglob on for inline globs. The note
oversimplifies.

**DD#26 — "Bump allocator."** The note calls heap_4 a "bump allocator."
The journal (038-dd26-heap4-allocator.md) describes it as a first-fit
free-list allocator with coalescing, which is specifically NOT a bump
allocator. heap_4 supports free(); a bump allocator does not. This is
a factual error.

**DD#30 — Strict types.** The note omits the amendment about bare
numeric assignment being a hard error (`x=5` without type keyword →
compile error). This is a significant behavioral rule absent from the
decision note.

**DD#33 — Unified hash table.** The note says "integer keys hashed as
raw i64, not converted to strings." The journal amendment says this
explicitly. But the first paragraph says "String keys always
(`arr[5]` → key `"5"`)." These two statements contradict each other.
The journal's amendment 2 supersedes the "string keys always" rule —
integer keys are hashed as raw bytes, not as string "5". The note
has both the old and new rules without marking the old one superseded.

**DD#38 — Reject subshells.** The note says `( cmd )` rejected. But
SUBSET.md lists `( cmd )` under "Accepted Constructs → Lists and
Subshells" as "subshells (limited, prefer explicit)." This is a
direct contradiction between the decision and the spec.

**DD#46 — Positional local.** The note accurately captures the final
state. However, the bash-mode coercion (top-level `$1` → `$(argv 1)`)
is listed as a task (notes/tasks/bash-mode-positional-coercion.md) with
status "ready" — meaning it's NOT implemented yet. The decision note
doesn't distinguish between the decided behavior and the implemented
behavior.

### Missing amendments

**DD#21 — Error suppression.** The journal has an amendment about
pipeline coercion semantics (DD#21 amendment in 042-phase2-dd29-45.md):
`- cmd1 | - cmd2` for per-segment suppression, and how scripts without
pipefail are coerced. The decision note mentions none of this.

**DD#24 — Taint.** The note references DD#43 and DD#44 but not
amendment 2 (no propagation, only classification — from
043-bugfix-taint-dd24-amend2.md). This amendment fundamentally changed
the taint model from flow-sensitive propagation to per-definition
classification. The note's "tainted variables cannot be used at any
site" is correct for the final state but doesn't capture that the
*mechanism* changed.

**DD#39 — Glob/regex.** The note omits amendment 2 (expression form
simplified from `${$var =~ s/...}` to `${var:s/...}`) and amendment 3
(non-colon parameter expansions coerced to colon forms). Amendment 3
is actually documented under DD#29 instead, which is where it
chronologically appeared in the journal, but logically it belongs
with DD#39 or DD#24 (since it relates to the unset/null distinction
being dead).

**DD#45 — Source emitter.** The note mentions the declaration axes
amendment but not the export-as-error amendment (`export` → SB5001,
cannot be mechanically coerced). This is documented in the journal
but missing from the decision note.

---

## 2. Contradictions and Incoherencies

### Explicit contradictions

**SUBSET.md vs DD#38 on subshells.** SUBSET.md accepts `( cmd )` under
"Lists and Subshells." DD#38 explicitly rejects it. One of these is
stale. Given that SB rules don't include a subshell rejection code and
DD#38 says "reject," this needs resolution. The SUBSET.md text looks
like it predates DD#38.

**SUBSET.md vs DD#30 on numeric comparisons in [[ ]].** DD#30 says
`-eq`/`-ne`/`-lt` etc. are "rejected inside `[[ ]]`" and coerced to
`(( ))`. SUBSET.md lists `-eq`, `-ne`, `-lt`, `-le`, `-gt`, `-ge` as
accepted operators under `[[ ]]` tests. These can't both be right.
The implementation likely accepts them in `[[ ]]` (since coercion
would require restructuring the test expression), which means DD#30's
aspiration was never enforced.

**DD#22 vs SUBSET.md on `[ ]`.** DD#22 and DD#40 imply `[ ]` compiles
identically to `[[ ]]` (sash has no word splitting so they're equivalent).
SUBSET.md says `[ ]` is "rejected (coerced to `[[ ]]`)." The GUIDE says
the compiler handles it silently. The decision notes don't have an entry
for this — it falls between DD#29 (coercion framework) and DD#40
(shellcheck gate). The actual behavior is likely silent coercion, but
the "rejected" language in SUBSET.md is confusing.

**DD#33 internal contradiction on key representation.** As noted above,
"String keys always" vs "Integer keys hashed as raw i64" within the
same note.

### Implicit incoherencies

**DD#15 (Janet oracle) vs DD#31 (arithmetic disjoint).** DD#15 says
the Janet backend is the normative semantics — discrepancies are bugs.
DD#31 introduces deliberate divergence from bash (`$((x=10))` not
updating `x`). If the Janet backend implements DD#31's semantics, that's
fine. But if the Janet backend follows bash's `$((x=10))` behavior
(updating `x`), then DD#15 and DD#31 are in tension. The normative
semantics should be sash's, not bash's — DD#15's wording needs to
acknowledge that sash deliberately diverges from bash.

**DD#42 (getenv/argv) status vs actual implementation.** DD#42 is
"designed" but the taint model (DD#24/43/44/46) assumes getenv/argv
exist as the canonical way to access external data. The taint
diagnostics guide users to `$(argv N)` but the feature isn't fully
exposed. This creates a usability gap — the compiler tells you to use
a feature that isn't fully available.

**DD#27 (trampoline) vs DD#38 (reject subshells).** DD#27 says
`$(func)` capture emits duplicate function blocks where echo appends
to a buffer. DD#38 rejects `( cmd )`. But `$(func)` IS a subshell in
bash. The distinction is that sash implements it differently (buffer
capture, not fork), but this is never explicitly stated in the
decisions. A reader could wonder why `$(func)` is allowed but `( cmd )`
is not, when both are subshells.

**DD#18 (local fn) depends on DD#38 (reject subshells).** DD#38 rejects
`unset -f` and `declare -f`, with `local fn` planned as the replacement.
But DD#18 is only "designed." Until DD#18 is implemented, there's no way
to do function mocking/shadowing — the old mechanism is rejected and the
new one doesn't exist.

---

## 3. Gaps and Flaws

### Gaps where decisions don't cover safebash goals

**No decision on signal handling model.** `trap` is accepted (listed in
SUBSET.md builtins) but there's no DD covering how signals interact with
the safety model. Can a signal handler read tainted data? Can it call
external commands? The taint pass presumably walks trap bodies, but
there's no decision documenting the intended semantics.

**No decision on `source`/`.` safe loader.** DD#01-09 item 2 says
"safe loader deferred." SB4002 rejects `source`. But there's no
decision describing what the safe loader would look like when built.
For a compiler whose goal is replacing bash scripts, inability to
compose multiple files is a significant limitation. This is a gap in
the design, not just the implementation.

**No decision on error handling model beyond `- cmd`.** DD#21 defines
`- cmd` as `cmd || true`. DD#20 bakes in `set -e` semantics. But
there's no comprehensive decision about: what happens when an external
command fails? Is the exit status always checked? Is `|| true` the only
way to ignore failures? How does `set -e` interact with pipelines,
command substitution, `if` conditions? Bash's `set -e` semantics are
notoriously complex and context-dependent.

**No decision on `exec` semantics.** `exec` replaces the process (exp
042). But `exec` without arguments in bash modifies file descriptors
for the current shell (`exec 3>file`). Is this supported? Rejected?
There's no decision.

**No decision on here-string taint edge cases.** The taint pass
(post-audit) marks `read x <<< "clean"` as clean and
`read x <<< "$tainted"` as tainted. But there's no DD documenting
this distinction. It emerged from audit finding T1 and was fixed, but
the rule isn't captured as a decision.

**No decision covering dynamic array sizing / sparse arrays.** DD#33
covers the implementation (hash table) but not the semantic rules: can
you assign `arr[1000]=x` without assigning 0-999? What does iteration
order look like? Bash allows sparse arrays; does sash?

### Potential sabotage of goals

**Auto-bless undermines audit visibility.** DD#24 says bless is "a
documentation/audit construct — auditors grep for `bless`." But
`known_externals.rs` auto-blesses 26 env vars and 25 commands. These
bless points are invisible in the source code. An auditor grepping
for `bless` won't find them. `--emit-source` inserts explicit bless
lines, but the compiled binary has no visible bless point for
auto-blessed data. The auto-bless list itself becomes a security-
critical surface that's maintained in Rust code, not in the script
being audited.

**DD#23 (compile-time PATH) has no enforcement mechanism documented.**
The decision says "scripts cannot modify PATH at runtime" and "external
command lookup resolved at compile time." But there's no validator rule
(no SB code) that catches `PATH=...` assignment. It may be implicitly
caught because `PATH` is a known env var, but the mechanism isn't
explicit.

**DD#40 (shellcheck gate) is optional.** `--nolint` skips shellcheck.
If a developer always uses `--nolint`, the shellcheck safety net is
absent. The decision doesn't discuss whether certain shellcheck rules
should be non-skippable.

---

## Summary

| Category | Count |
|----------|-------|
| Factual inaccuracies | 4 (DD#01-09 namerefs, DD#26 allocator type, DD#33 key contradiction, DD#38 vs SUBSET.md) |
| Missing amendments | 5 (DD#21 pipeline, DD#24 mechanism change, DD#30 bare numeric, DD#39 expression simplification, DD#45 export error) |
| Explicit contradictions | 3 (subshells, numeric comparison operators, `[ ]` handling) |
| Implicit incoherencies | 4 (oracle vs divergence, getenv status, subshell vs cmdsub, local fn dependency) |
| Design gaps | 6 (signals, source loader, error model, exec semantics, herestring taint, sparse arrays) |
| Goal-undermining patterns | 3 (auto-bless visibility, PATH enforcement, optional shellcheck) |
