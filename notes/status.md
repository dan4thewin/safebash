## ════════ STATUS (2026-09-03) — relative external paths rejected (DD#23; SB5051) ════════

**On `hand-parser`.** DG: "look at _tmp/17, I'm getting 2 SSA taint backstop
warnings" (a `sash-getopt` arg parser run as `PATH=.:$PATH ./sash 17`). Root
cause: with `.` in PATH, `sash-getopt` resolved to the **relative** path
`./sash-getopt`, and the CST taint check (`command_name_is_taint_source`) used
`starts_with('/')` (absolute-only), so the capture wasn't tainted — only the
fail-closed SSA backstop caught it (a reject where a clean SB5020 should fire).
A relative external path violates DD#23 ("absolute path only / no search
ambiguity"). DG chose **reject** (not absolutize) + CST-taint hardening. Fixes:
`resolve_command_path` skips non-absolute PATH dirs; a source-written relative
head (`./x`, `foo/bar`) → **SB5051** uniformly (top-level, `$( )`, pipeline
stage); `is_valid_command` defers relative heads so `resolve` owns the
diagnostic; `command_name_is_taint_source` → `contains('/')` (defense-in-depth).
Now `PATH=$PWD:$PATH` (absolute) gives the correct **SB5020** on the unblessed
capture. specs errors.sh relpath.1–6 (plan 98→104). 3-mode sweep: 0 regressions,
18 new passing. SB5051 new. DD#23 addendum; journal 239.

## ════════ STATUS (2026-09-03) — `exit` accepts any int (DD#89) ════════

**On `hand-parser`.** DG: "bless … any int value/variable/return should work as
argument to exit, and a DD/specs … but I don't intend to dictate [tagging] vs. a
global — it's up to them." `exit`'s argument is now an ordinary int sink — a
literal, variable, `$((…))`, or `-> int` function return — no compile-time range
limit. Two gaps closed: native `emit_exit` only handled a literal (non-literal →
0; c already used `lower_value_as_int`); type_check rejected `exit $intvar`
(removed). Full i64 internally (readable in END via `$?`), truncated to 8 bits
only at `exit_group` (`exit 300` → END sees 300, process exits 44). DD#89 blesses
the mechanism, NOT an idiom (out-of-range-tag vs plain global = programmer's
choice). `emit_exit` routing flattened (cog 8→2). specs exit_arg.sh 89.1–89.8
(89.8 native — the c exit-skips-END gap). journal 238. 3-mode sweep: 0
regressions.

## ════════ STATUS (2026-09-03) — `$?` readable in the END block (DD#56) ════════

**On `hand-parser`.** DG: reading `$?` as the first action of `END { }` gave
SB5012, but an abort/`exit N` jumps to END with a triggering status — so it
should be readable. DD#56 already designed this ("In END: the status that
triggered END"); the runtime already carries it (`__end_status = __exit_status`
at END entry, `__exit_status` not reset). Validator-only fix: `check_dq_function`
starts the `__sash_end__` body (the END block) in the `Available` `$?` state —
first-action readable like a handling branch; other functions stay `Undefined`
(no leak). `$?` is a full i64 — works in `(( $? > 255 ))`, `int code=$?`.
Behavior: abort → `$?`=failing code; `exit N` → N; normal → last status.

specs signals.sh 56.30–56.33 (plan 19→23; 56.33 guards no function leak). Found
+ filed a pre-existing **c bug** (`emit_c::emit_exit` `goto __exit` skips END for
a main-body `exit N`; native routes through it) — `bug-emit-c-exit-skips-end`;
56.31 gated `native`. janet END is a separate pre-existing gap. 3-mode sweep: 0
regressions. DD#56 amended; journal 237.

## ════════ STATUS (2026-09-02) — capture-condition `if x=$(cmd)` shipped (DD#88) ════════

**On `hand-parser`.** DG: "what would it take to enable `if ! x=$(cmd) {`" +
"the actual exit status of the cmd is wanted, and the stdout need only survive
if the cmd succeeded." That narrowing made a shippable subset of the
`multi-assignment-and-capture-trio` decision — DG chose to implement now.

**DD#88 capture-condition:** in a boolean context, an assignment whose RHS is a
single **external** `$(cmd)` (or pipeline `$(a|b)`) branches on the command's
**actual exit status** (no abort) and captures its stdout into the target
(tainted, bless before use). `$?` is available as the first branch action
(DD#55), so `if x=$(cmd) { use $x } else { echo "failed $?" }` reads the real
code. `!`/`else`/`$?` are uniform with a plain command condition (negated form
loses `$?` magnitude, like `if ! cmd`). Untyped only; `while` included; a
truly-unused captured target is SB5050 (use the non-capturing `if ! cmd`).

Mostly wiring (union of `x=$(- cmd)` + `while read x`): new sigil
`SetTestFromStatus`; `try_capture_condition` in transform; pipeline `checked`
honors `state.suppressed`; validate SB4025 carve-out via the structural
external-path signal + `$?`-provider registration. native + c; **janet gated**
(emits from IR not the sigil; `$(cmd)` capture is a stub — filed
`janet-capture-condition`). Rejects backend-independent. `spec/capture_cond.sh`
88.1–88.14. 3-mode sweep vs HEAD: native 0 regressions, c 0, janet 0
(+14/+14/+6). DD#88; journal 236. The general multi-value return / trio stays
deferred (`multi-assignment-and-capture-trio`).

## ════════ STATUS (2026-09-02) — `if ! out=$(cmd)`: decision filed + 2 bug fixes ════════

**On `hand-parser`.** DG asked whether SB4025's "sequence it" advice works for
`if ! out=$(cmd) { echo bang }`. It doesn't — capture stdout AND branch on a
command's status has no working sash form. DG: "capture this as a decision
task; then address only the existing bugs, but not new features" + "I consider
the bad sequencing advice a bug too."
- **Decision (feature, deferred):** filed `multi-assignment-and-capture-trio`
  — general multi-value `return` + external capture as `(stdout, status,
  stderr)`; crux rule = *binding the status toggles catch vs. abort*; owns the
  SB4025↔DD#55 conflict resolution. No impl until DG settles the axes.
- **Bug 1 (fixed):** SB4025's "sequence it" advice was wrong for a `$(…)`
  capture (a failed capture aborts before any follow-up check). Made
  `nonstatus_in_condition_diag` RHS-aware (`value_is_capture`): capture → honest
  advice (`if ! cmd { }`; capture+branch unsupported); literal → keeps "sequence
  it". specs e.br8/e.br9.
- **Bug 2 (fixed):** `x=$(- /bin/false)` aborted, but DD#55 says `-` discards
  the status (no abort) — spec 55.9 only tested a succeeding `$(- /bin/echo)`.
  The single-command capture fast-path dropped the `-` suppression; `CmdSubst`
  gains `suppressed`, detected in `handle_command_substitution`, gating the
  abort in native + c. spec 55.9b.

3-mode sweep vs HEAD: native 0 regressions, c 0, janet 0 (+1/+1 55.9b;
e.br8/e.br9 all modes). journal 235.

## ════════ STATUS (2026-09-02) — bugfix: positional `${N:OP}` == `${args[N-1]:OP}` ════════

**On `hand-parser`.** DG hit a spurious **SB5042** on a function using `${2:~}`
called with one arg. Root cause = two coupled bugs (the positional `${N:OP}`
disposition form was never tested — all DD#84 specs used `${args[k]:OP}`):
1. **Arity** (`sig_scan_pexp`): a positional `${N:-}`/`${N:~}` was marked
   REQUIRED (fell through to `sig_record_name`, ignoring the operator), so
   `${2:~}` demanded 2 args → SB5042. Now classified optional like the
   `args[k]` path.
2. **Value** (`pe_source_value`): a no-index `${N:OP}` read `Var(lookup("N"))`
   — a scalar var literally named "N" that never exists — so `:-` always
   defaulted and `:~` always elided even when `$N` was present. Now routed
   through `positional_to_value` → `args[N-1]`.

`${N:?}` stays required (compile-time SB5042, like `${args[N-1]:?}`); SB5047
mixing / SB5048 trailing now fire for positionals too. specs
`optional_args.sh` 84.19–84.24 (plan 18→24). 3-mode sweep vs HEAD: native 0
regressions, c 0, janet 0 (+6/+6/0). DD#84 addendum; journal 234.

## ════════ STATUS (2026-09-02) — `read -r` no-op; `read -d` = terminator ════════

**On `hand-parser`.** DG: "we add -r as a no-op and we change -d to be
terminator not separator" (the two top recommendations from the
`read-behaviors-not-wired` survey). Both shipped:
- **`-r`** — accepted as an ignored no-op in `ir::classify_read_flags` (sash
  reads are always raw; was SB5015). Fixes the hand-written `read -r x` tax.
  Bundled `-rd ''` still unsupported (sash bundles no flags).
- **`-d`** — now **terminator** semantics: an explicit `read -d` drops the
  trailing empty from a final delimiter (bash `mapfile -d ''` / `find
  -print0`). Threaded a `terminator` arg to `sash_split_into_array`, set iff
  `delim.is_some()`. Interior empties (`a::b`) kept; the **default** space
  split (no `-d`) keeps separator semantics (DG scoped to `-d`). The getopt
  idiom is now clean — `sash-getopt … | read -a opts -d ''` has no trailing
  `[]`, and `bless e in opts [[ -n $e ]]` passes.

specs wordsplit.sh 22.25–22.27 (plan 77→80). 3-mode sweep vs `562fc36`:
native 0 regressions, c 0, janet 0 (+3/+3/+1). journal 233. Survey task marked
done; remaining `-n`/`-N`/`-u`/interactive flags stay unimplemented.

## ════════ STATUS (2026-09-02) — read-flag parsers unified; `read -d ''` = NUL ════════

**On `hand-parser`.** DG: "unify them, support -d '', call out other read
behaviors." Replaced **four** independent read-arg parsers (transform ×2,
validate `collect_read_targets`, type_check `register_read_array_target`) with
one shared `ir::classify_read_flags`. Kills the reject-vs-ignore inconsistency
(unknown flags now → SB5015 in BOTH standalone + lastpipe), the lastpipe `-d`
split-form bug, and a dead-var (SB5050) false positive where a delim value
(`read -d : x` → `:`) was mistaken for a target. `read -d ''` now = NUL (root
cause was `expr_to_string` re-quoting; `read_arg_value` unwraps quotes, empty
→ NUL) — the sash-getopt idiom `sash-getopt … | read -a arr -d ''` works
end-to-end (both pipe + redirect; space-bearing fields stay one element).
`read -z` still rejected but consistently. specs wordsplit.sh 22.21–22.24.
3-mode sweep vs `002b833`: 0 regressions. journal 232.

**Called out (filed `read-behaviors-not-wired`):** (1) `-r` — rejected but
sash is always raw, and bash uses it everywhere → recommend accept-as-no-op;
(2) NUL-terminator trailing-empty (getopt's final NUL leaves a trailing `[]`;
bash mapfile -d '' drops it) → recommend terminator semantics for `-d`;
(3) `-n`/`-N`/`-u`/interactive flags — leave unless needed.

## ════════ STATUS (2026-09-02) — `sash-getopt` vendored (NUL/unquoted) ════════

**On `hand-parser`.** Second half of the readc-drop directive: vendored
`sash-getopt` from `~/contrib/util-linux/misc-utils/getopt.c` (Frodo Looijaard,
GPL-2+; header preserved). Stripped the util-linux deps (nls/xalloc/strutils/
closestream → small inline helpers), kept the getopt(3)/getopt_long core +
`-o`/`-l`/`-n`/`-a`/`-q`/`-Q`/`-T`/`-U`. **Patched the output to NUL-separated,
unquoted fields, no trailing newline** — `emit(s)` writes `s\0`; `print_normalized`
(shell-quoting) removed; `-s`/`-u` accepted-and-ignored. `sash-getopt -o ab: --
-a -b val 'has space'` → `-a\0-b\0val\0--\0has space\0` (space-bearing arg is one
field). Build: `target/release/sash-getopt` (`cc -O2`) in mkfile `build`;
`sash-getopt-test` in `test`; `contrib/test-getopt.sh` (5 checks, all pass).
journal 231.

**OPEN — the NUL-read consumer.** The runtime split is already NUL-safe
(`sash_split_into_array` length-based; `read -a` splits on `flags.delim`); only
the *surface* for a NUL delimiter is undecided (`read -z` is rejected today,
SB5015/cov.20). Flagged in `switch-free-read-family` for a DG decision.

## ════════ STATUS (2026-09-02) — `readc` DROPPED (getopt-NUL route) ════════

**On `hand-parser`.** DG dropped `readc` (cooked read, shipped `0445fe1`) after
an ICE on `… | readc -a out`. Cooked-read parsing of getopt's shell-quoting was
judged too fragile; the sanctioned getopt idiom is **NUL-splitting** — a patched
`sash-getopt` (NUL field separation, no quoting) piped into a NUL-delimited
`read`. Removed all readc + cooked machinery (runtime `sash_split_first_word_
cooked`/`sash_deescape`; the `cooked` SSA field + all plumbing; native/c codegen
cooked paths; readc recognition in BUILTINS/taint/type_check/lastpipe/janet).
Kept the independent `insert_read_target_types` c-fix. `readc` → SB5011; the ICE
command is now a clean error. 3-mode sweep vs `8a67e69`: 0 regressions. journal
230. **Next: vendor `sash-getopt`.**

## ════════ STATUS (2026-09-02) — DD#86 Phase 5: converter drops `local`; docs. DD#86 COMPLETE ════════

**On `hand-parser`.** DD#86 Phase 5 (migration/cleanup) — the converter no
longer emits `local` (it produced uncompilable sash): bare `local x=5` → `x=5`
(auto-local), `local -i x=5` → `int x=5`, `local -r` → `const`. Round-trip
verified; 18 sash-convert tests pass (`coerce_local_i_to_int` updated). Docs:
GUIDE dropped the `local int x` example; DD#35 amended (lifetime axis retired,
`str` added). Shadow-rename refinement filed (`converter-shadowing-local-
rename`). journal 229.

**DD#86 is COMPLETE (phases 1–5):** 1 `str` keyword; 2 drop `local`/`local fn` +
structural scope; 3 no-shadow (SB5049); 4 dead-variable rejection (SB5050);
5 converter + docs. No `local`, no shadowing, structural scope, use-before-and-
after (SB5031 + SB5050).

## ════════ STATUS (2026-09-02) — DD#86 Phase 4: dead-variable rejection (SB5050) ════════

**On `hand-parser`.** DD#86 Phase 4 — the dual of DD#62 SB5031: a variable
assigned but never read is a dead variable (**SB5050**). A global is dead if
read nowhere in the program; a function-local if read nowhere in its function.

**DG rulings (2026-09-02):** (1) more-specific error wins — dead-var runs LAST,
after taint/type/transform (moved out of `validate()` to end of the front-end;
`enforce_transform_diags` now runs for every backend); (2) `read`/`mapfile` targets and `for` variables ARE checked (for-var no longer exempt);
(3) `: $v` is the acknowledgment escape hatch (a real read per DD#81 arity).

**Reuses the read-enumerator (DG):** inverted the SB5031 read-traversal via a
shared `ReadSink` (Flag = SB5031, Collect = dead-var) — one read-recognition,
so they can't diverge. SB5031-specific opt-outs (`${x:-…}`/`@a`, DD#58 sexpr
names) live in the sink so Collect still counts them. Added value-fn bare
operands (`len`/`join`/`keys`) + fixed two enumerator gaps (ArithTest `(( x ))`
reads, bless array/map operand) that benefit SB5031 too.

**Read-completeness proven:** 65 native breaks (hand-rolled, with FPs) → after
the enumerator reuse, **every break was genuinely dead — 0 false positives**.
Rewrote 14 genuinely-dead specs (`: $v`/`: @a`/`bless` to read the var).
specs scope.sh 86.13–86.20. Full 3-mode sweep vs `30be5f6`: **native 0
regressions +8, c 0 +8, janet 0 +8**. fmt/clippy clean; lib tests pass;
complexity gate cleared. journal 228. Filed
`bare-collection-name-operand-asymmetry` (DG asked).

**Next:** Phase 5 (converter `local`→drop; GUIDE/SUBSET; DD#35 amend).

## ════════ STATUS (2026-09-01, cont) — DD#86 Phase 3: no shadowing (SB5049) ════════

**On `hand-parser`.** DD#86 Phase 3 — a typed re-declaration
(`str`/`int`/`array`/`map`/`const`) inside a function of a top-level global name
is a redeclaration error (**SB5049**); a bare assignment writes the global.
`check_no_shadow` in validate.rs collects top-level decls (globals, not
descending into fn bodies) then scans each fn body for typed decls colliding
with a global. specs scope.sh 86.7–86.12 (native,c,janet). Full 3-mode sweep vs
`b935e28`: native 0 regressions +6, c 0 regressions +6, janet +8. journal 227.

**Next:** Phase 4 (dead-var), Phase 5 (converter + docs).

## ════════ STATUS (2026-09-01, cont) — DD#86 Phase 2: drop `local` ════════

**On `hand-parser`.** DG: "let's finish the local-related work." DD#86 Phase 2 —
removed the `local` keyword and **all** its machinery (lexer/parser `Tok::Local`
+ `parse_local` + `is_local`; IR `DeclKeyword::Local` + `Function::is_local`;
transform `collect_local_vars`/`save_caller_locals`/`restore_caller_state`/
`local_fn_scope`/`__local__`; validate/resolve/janet arms). DG confirmed **drop
`local fn` (DD#18) entirely** — folded into Phase 2 since it shared every piece
of machinery. `local x` / `local fn` now → SB5011 (converter drops it, Phase 5);
a nested `fn` is an ordinary top-level function.

Structural scope was already mostly in place (bare `x=v` in a fn writes the
global; a new fn-only name is invisible at top level via SSA versioning +
SB5031). `local` only added a *shadow* — pure deletion.

**Latent bug surfaced + fixed:** a function-local `const int x=5` tripped SB4044
because the body is lowered twice (template + inline) and `readonly_vars` leaked;
`local const` masked it (keyword was `Local`, never populated `readonly_vars`).
Fixed with `enter/exit_fn_body_scope` snapshot/restore of `readonly_vars` around
every fn-body lowering — fn-local const is now isolated while global-const and
within-fn reassignment enforcement are preserved.

Specs: rewrote the 7 `local` specs (functions 18.1–18.4, types 35.1, arithmetic
ivc.8, errors 55.7). Full 3-mode sweep vs `0445fe1`: **native 0 regressions,
c 0 regressions**, janet +2. fmt/clippy clean; lib unit tests pass. journal 226.

**Next:** Phase 3 (no-shadow), Phase 4 (dead-var), Phase 5 (converter + docs).

## ════════ STATUS (2026-09-01, cont) — `readc` cooked read (switch-free step 1) ════════

**On `hand-parser`.** DG: "capture the larger question, plus a survey of all
switch use into a decision task; implement readc now." The decision task
(`switch-free-read-family`, filed earlier) holds the larger direction
(`read`→`readr`/`readc`, drop `-a`/`-f`/`-d`/`-z`). This session lands the first
concrete step: **`readc`**, a cooked read (honors `\`-escapes; `\<ws>` doesn't
split, field de-escaped) — the bash-default `read`, where sash `read` stays raw
(bash `read -r`). Unblocks the eval-free getopt-csh idiom.

Runtime `sash_split_first_word_cooked` + `sash_deescape` (NUL-term per DD#87);
`cooked: bool` on `ReadLine`/`ReadLineMulti`; `readc` recognized everywhere
`read` is (BUILTINS/SB5011, taint, type_check, lastpipe, janet while-read).
Native + c codegen wired (single-var de-escapes the line; multi-var:
cooked-split middle fields, de-escape the remainder). janet recognizes readc
but treats it raw (cooked deferred → `janet-readc-cooked`).

**Pre-existing c bug fixed:** `emit_c::collect_inst_var_types` never declared a
read target that's assigned-but-never-read (native always did) → C compile error
on `… | read a b; echo $b`. Reproduces at HEAD with raw `read`. Fixed via
`insert_read_target_types`.

**Not a bug:** an early spec exposed sash double-quote `\\` keeping two
backslashes vs bash's one — that's DD#64 (no-shell-backslash), ratified; spec
dropped.

specs `spec/wordsplit.sh` 22.20–22.24 (native,c; janet skip). Full 3-mode
sweep vs HEAD `005c7d4`: **native 0 regressions +5, c 0 regressions +5**,
janet 0/0. fmt+clippy clean; complexity gate cleared (extracted
`emit_read_line_call`/`emit_read_last_var`/`emit_read_field_split` native,
`emit_readline_last`/`insert_read_target_types` c). journal 225.

## ════════ STATUS (2026-09-01, cont) — bless-fallback via `||` ════════

**On `hand-parser`.** DG wanted "use DEFAULT when a bless fails" (the unbuilt
journal-036 `${port:~8080}` idea). Instead of a new operator, DG asked whether
`||` solves it — and it should, but `bless v [[t]] || v=dflt` **mis-compiled**
(aborted on fail; ran the fallback even on valid input). Root cause: a bless as
the left of `&&`/`||` lowered to the *abort* form, not the catchable if-bless
form. Fixed `lower_list_stmt` to rewrite `bless … || X` → `if bless {} else {X}`
(and `&& X` → `if bless {X}`), reusing `lower_if_bless`. Now `bless v [[t]] ||
v=dflt` = validated-or-default, clean either way; fallback trusted, not
re-validated (caveat programmer). Bare bless still aborts. specs taint.sh
bfb.1–6; 0 regressions, +12 passing. DD#24 amended; `:~` stays DD#84 elide (the
`colon-tilde-elide-vs-bless-fallback` task is closed, no `:!` operator). journal
224.

## ════════ STATUS (2026-09-01) — DD#86 scope model (Phase 1) + DD#87 runtime NUL fix ════════

**On `hand-parser`.** Two things this session, both pushed:

**DD#87 (pushed `031b5f5`) — critical runtime bug.** Found while testing DD#86:
interpolating an int var + a string var into one string and passing it to an
**external** command produced trailing garbage (`/bin/echo "n=$n s=$s"`), while
the builtin echo was clean. Root cause: sash-rt string builders allocated exactly
`len` bytes with no NUL; execve reads argv as C strings → read past the end.
Masked because specs use the builtin (length-based) echo. DG chose approach **B**
(whole-runtime invariant): every sash string NUL-terminated at `[len]` via a new
`alloc_cstr` helper; `sash_substring` now copies; `sash_itod`/`sash_regex_replace`
re-terminate. specs `spec/extern_args.sh` xa.1–8; 0 regressions, +16 passing.
DD#87, journal 222.

**DD#86 Phase 1 (this commit) — the `str` keyword.** DD#86 (variable scope: no
`local`, no shadowing, use-before-and-after) is fully ratified with DG and is a
5-phase change. Phase 1: `str` declares a string with meaningful-zero `""`
(parallel to `int`), so a string global can be declared at top level without
`=""`. DG picked `str` over `var` (names the type; 0 spec collisions vs 50).
Also fixed a pre-existing janet bug (`int n` no-init → malformed `(var n )`;
now `0`). specs `spec/scope.sh` 86.1–6 (all modes); 0 regressions, +18 passing.
DD#86, journal 223.

**Phases 2–5 remain** (filed `notes/tasks/dd86-scope-phases-2-5.md`): drop
`local` + structural scope (top-level=global, new-in-fn=local, frictionless
global R/W); no-shadowing/redeclaration error; dead-variable rejection (dual of
DD#62, no warnings); abandon `local fn` (DD#18); converter + all `local` specs +
DD#35 amend. Each phase lands with its spec updates to keep the gate green.

## ════════ STATUS (2026-08-31, cont.5) — comments are lexical trivia (DD#85) ════════

**On `hand-parser`.** DG: "trailing comments seem to be broken." They were —
`echo hi # c`, `int foo=1 # c`, `for x in a b # c`, `if true # c`,
`echo hi | cat # c`, `return 0 # c` all → SB4014. Root cause: value-collecting
parse loops recognized only `Newline|Semi|Eof` as terminators, so a trailing
`Tok::Comment` hit the value parser.

DG chose **B — comments as lexical trivia** (over patching every parse
position): the Scanner now drops `Tok::Comment` (`next_token_skip_comments`),
and `StmtKind::Comment` + `parse_comment_stmt` are removed. Safe because
`.sash_source` embeds source **verbatim** (round-trip unaffected), comments were
already no-ops in native/c, and `--emit-source`/`--annotate` are already
rejected. Only loss: non-gate janet stops rendering comments (DG accepted).
DD#85; journal 221.

`next_token` left byte-identical (grandfathered); the filter is a tiny wrapper.
Behavior matches bash (`# ; x` runs nothing after `#`; same-line `# c }` eats
the `}`; shebang dropped; `#` in `$(( ))` stays a Word per DD#72). specs
`spec/comments.sh` cmt.1–11 (all modes; cmt.11 native,c). Full 3-mode sweep vs
`e638e84`: **0 regressions, +32 passing**. Lib tests green.

**Open follow-up (filed, needs a DG decision):**
`notes/tasks/arith-block-separators-and-comments.md` — DG observed that
multi-line `(( ))` disallows end-of-line comments, and that the `;`/`,`/newline
separator story inside `(( ))` is inconsistent (for-header allows `;`, standalone
rejects it; `,` works). Three linked decisions: (1) is multi-line `(( ))` one
expression or a sequence block (newline ≡ comma)? (2) `;` ≡ `,` in a standalone
`(( ))`? (3) allow `#` comments inside arith (DD#72 freed `#`)? All are
deliberate-divergence-from-bash calls; today sash matches bash on each.

## ════════ STATUS (2026-08-31, cont.4) — `!` on the terminal pipeline segment ════════

**On `hand-parser`.** DG's `… | ! grep -q z` (journal 219's line) uses a `!` on
a non-first pipeline stage. Designed + implemented **terminal-segment
negation**: `a | ! b` negates the FINAL stage's status (0↔nonzero) before the
DD#55 worst-of; a non-terminal `!` (`! a | b`, `a | ! b | c`) stays SB4040;
`-`/`!` don't combine on one segment (SB4019 both directions); `return ! X | Y`
→ SB4040 (was SB4014).

Bug fixed: `append_pipeline_stage` flattened the parsed `! b` stage and **dropped
the negation**. New `StmtKind::Negated` per-segment wrapper (symmetric with
`Suppressed`) → `negated: Vec<bool>` mask on SSA `Pipeline`; native/c/janet all
invert the negated stage's status in the pipeline wait. Standalone `! cmd`
(`Pipeline.negated: bool`) untouched. `parse_return` cc 14→7 (extracted producer
head).

DD#70 addendum + DD#55 pointer. specs `spec/errors.sh` bng.7–12 (plan 91→97), all
three modes. Full 3-mode sweep vs `25d7d66`: **0 regressions, +18 passing**. Rust
suites: failure set identical at baseline (stash-verified; pre-existing rot).
journal 220. The `X | ! Y` open item from cont.3 is now **resolved**.

## ════════ STATUS (2026-08-31, cont.3) — `return <composite producer>` (SB4037 fix) ════════

**On `hand-parser`.** DG (demo): `fn f { return echo abc | ! grep -q z }`
reported **SB4037** but is clean. Root cause: `parse_return` parsed only the
first producer unit, so `return A | B` folded to `Pipeline([return A, B])` — the
`return` trapped in stage 0, `B` uncaught (SB4037 in a predicate; a silent abort
as a statement). Whole class `return A|B` / `A&&B` / `A||B` / `A>f` broken.

Fix: `parse_return` now folds the producer through `maybe_wrap_redirects` (the
same composite-folding a statement gets) before `ReturnProducer`. Lowering
already handled `ReturnProducer(Pipeline/List)` (condition-context lower +
un-check last). One-line-ish parser change; cc 12 / cog 4 clears the gate.
Verified vs bash (pipeline match/no-match, `&&` both-true/second-false, `||`).

specs `spec/functions.sh` 74.19–74.23 (plan 63→68): pipelines 74.19/20 all
modes; `&&`/`||` lists 74.21–23 native,c only. **0 regressions** (3-mode diff vs
HEAD `496ece5`); +12 passing (5 native + 5 c + 2 janet), 0 new failing. DD#74
producer table amended; journal 219.

janet gated: its AST return-producer emit **collapses** an `&&`/`||` list to one
operand (emits `(if true 0 1)` regardless) — filed `janet-return-composite-
producer`. Pipelines are genuine in janet.

**Open (raised separately, needs DG):** `X | ! Y` — a negated **non-first**
pipeline stage silently drops the `!` (inverted result) + aborts as a statement.
Pre-existing, inconsistent with the SB4040 rejection of `! X | Y`. DG's line uses
it. Fork: reject (SB4040/DD#70-consistent) vs support (negated final stage
governs status).

## ════════ STATUS (2026-08-31) — DD#84 optional trailing args + `:~` elide (native+c) ════════

**On `hand-parser`.** DG-designed feature (discussed + ratified this session):
make a trailing argument optional and let a scalar token *disappear* from a
list. Two capabilities: **(A)** a trailing `${args[i]}` read only via `:-`/`:~`
is optional (arity `[min,max]`; mixing bare+disposition → SB5047, required-after-
optional → SB5048); **(B)** `${x:~}` / `${args[i]:~}` *elides* — zero tokens when
empty, one otherwise — in an elidable position (command arg / array element),
rejected elsewhere (SB5046) and never takes a word (SB4051). DD#84; journal 218.

Lowering: `:~` → `SsaValue::ElideEmpty`; native + c list builders do a runtime
0-or-1 on emptiness (argv + array literal, compacting). specs
`spec/optional_args.sh` (84.1–18). Full 3-mode sweep vs `d88969b`: **0
regressions; +18 native/c, +4 janet** (arity refactor touched every fn def —
clean). janet deferred (no `args` model; `janet-optional-args-elision`); int-cell
identity deferred (elides on empty string).

## ════════ STATUS (2026-08-31, cont.2) — dispatch compact-op dead branches removed ════════

**On `hand-parser`.** The last two grind never-runs (`extract_from_literal`,
`extract_leading_operator`) resolved to shadowed-dead: an eprintln probe over a
full `[[ ]]` battery showed neither ever fires — the lexer always splits `==`/`=~`
into its own token, so a compact operand always arrives as `Concat([subj, "==",
pat])` (middle-part branch), never a glued `Literal` or glued last part. Removed
both fns + the two dead call-site branches; remove-then-run 0 regressions (3-mode
sweep vs `d398170`). Task done.

**Grind endpoint reached** — spec-only native-core never-run is now only the two
unit-tested intentional ICEs + `type_check::fmt` (documented artifact).

## ════════ STATUS (2026-08-31, cont.) — grind tail resolved (CLI + ICEs + queue) ════════

**On `hand-parser`.** Closed out the coverage grind tail per DG:
- **CLI switches → `spec/cli.sh`** (new; cli.1–6, all modes): `--manifest`
  (emit_manifest), `--extract` (dump_sash_source), `--sysroot=` (set_sysroot +
  sysroot_file_exists). All four lib.rs handlers flipped 0→>0.
- **Intentional ICEs → unit tests**: `panic_payload_message` (3 cases);
  `redirect_expr_zero` **removed** → the three silent-empty `(0,0)` redirect-target
  fallbacks are now a **loud ICE** `redirect_target_ice`, with a unit test that
  builds a FunctionBuilder and asserts the panic. Full sweep after making it loud:
  0 regressions (nothing real reached the silent-empty path).
- **Queued**: `extract_from_literal`/`extract_leading_operator` (dispatch
  compact-operator Literal/leading-op branches) → `grind-dispatch-compact-operator-branches`.
- **Explained**: `type_check::fmt` (uncalled `Display for TypeError` — errors
  surface via `.message`/`.span`, not `{}`).

Full 3-mode sweep vs `cd97212`: **0 regressions, +6 cli each**. 16 lib unit tests
pass. Spec-only never-run endpoint = the two unit-tested ICEs (can't be
spec-driven) + the two queued dispatch branches; all else covered or documented.

## ════════ STATUS (2026-08-31) — pipeline-stage redirect targets fixed (grind) ════════

**On `hand-parser`.** Resumed the coverage grind; probing never-run redirect fns
found a bug (DG ruled "fix to support"). Pipeline-stage redirects silently
dropped every non-literal target (`echo hi > $f | cat` etc. leaked to the pipe
instead of the file) — the stage path had its own `ir_expr_to_ssa_value` that
hardcoded SSA version 0 and dropped `${x:-f}`/`$a[i]`. Fixed: route
`emit_stage_redirects` through the top-level `lower_ir_redirect_expr`; spill
stage targets in `push_command_stage` (threaded `block` through the pipeline
builders); deleted `ir_expr_to_ssa_value`. Journal 217.

specs commands.sh 14.26–31 (stage `$f`/`${x:-f}`/`$a[i]`; plus never-run
`&>`/`> '…'`/`> "$x.txt"`; plan 40→46, native,c). Full 3-mode sweep vs
`5ea5ceb`: **native/c 0 regressions, +6 each; janet 0**. Coverage re-measured:
native-core never-run **25 → 21** (4 flipped/removed).

**Grind continues** (21 left, triaged in `coverage-accounting.md`): bucket-A
diagnostic specs (`find_dollar_question`/`expr_invokes_random`/
`check_arg_default_taint`), bucket-B `strip_matching_quotes` (remove), defensive
`redirect_expr_zero`/`panic_payload_message`, CLI-only fns.

## ════════ STATUS (2026-08-30) — here-strings `<<<` dropped (DD#83, SB4050) ════════

**On `hand-parser`.** DG ruled `<<<` out of the language. It's exact sugar over
`producer | consumer` in sash — no subshell (so `echo $x | read v` already
persists `v`), provenance-based taint (clean/tainted is the source, not the
channel), no word-splitting → `<<< X` ≡ `echo-of-X | consumer` for every
operand, byte-for-byte. Pure sugar, so removed. DD#83, journal 216.

`<<<` → **SB4050** at parse (message names the rewrite). Removed
`RedirectOp::HereString` + all downstream (native/c/janet emitters, the
`sash_herestring_redirect` runtime symbol, the validate taint exception, the
`ssa_taint` clean-literal source, the transform special-case) — 9 sites, driven
by exhaustiveness. specs commands.sh 14.25 (SB4050 all modes; plan 39→40);
taint.sh 24.6 rewritten to `echo hello | read x`. Full 3-mode sweep vs
`be6837e`: **0 regressions, +1 each**. Supersedes
`bug-herestring-nonscalar-data-empty`; filed `converter-herestring-to-pipe` (P3).

**Next:** resume the never-run grind (`grind-continue`) — native gate clean.

## ════════ STATUS (2026-08-30) — redirect target must be a scalar (SB4049 + spill) ════════

**On `hand-parser`.** DG's ruling on the redirect-target silent-empty bug
(`bug-redirect-nonsimple-target-empty`, surfaced by the grind): **support
scalars, reject complex.** Journal 215; bug task marked done.

`> ${x:-f}` / `> $a[i]` silently redirected to an empty filename; `> $(cmd)` /
`> $((x))` too. Fix in two parts: (1) **parse reject SB4049** —
`reject_nonscalar_redirect_target` on every file-redirect target
(`>`/`>>`/`<`/`<>`/`&>`; not `<<<`, not `>&`/`<&` fd-dups); allowlist =
literal/quoted/`$x`/`${…}`/`$a[i]`/concat-of-those. (2) **transform spill** —
`lower_redirected_stmt` lowers a ParameterExpansion/Subscript target via the
normal `expr_to_value` into a synthetic `__redir_target_N` scalar and rewrites
the redirect to read it (codegen already handles `$var`); no new SSA type,
native+c inherit it. janet unchanged (no file redirects; reject still applies at
parse). specs commands.sh 14.19–24 (plan 33→39); taint.sh 60.41/60.52 rerouted
through a scalar (direct `$(join …)` target now SB4049 before SB5020). Full
3-mode sweep vs `65b4375`: **native/c 0 regressions, +6 each; janet 0, +2**.
fmt+clippy clean, complexity gate clear.

**Filed:** `bug-herestring-nonscalar-data-empty` (P2) — the `<<<` DATA path has
the same silent-empty class + a likely missing-newline bug; left for a DG ruling
(here-strings feed stdin data, so "reject complex" doesn't transfer 1:1).

**Next:** resume the never-run grind (`grind-continue`) — native gate clean.

## ════════ STATUS (2026-08-29) — `$(( args[i] ))` in a function fixed (arith args resolve) ════════

**On `hand-parser`.** DG "fix now" — the latent native bug flagged while
answering "were there more bugs". Journal 213.

`$(( args[0] ))` in a function was an SB5031 false positive masking a codegen
ICE: the arith-subscript lowering never resolved `args`→`__argsN` (the word path
does). Fix: (1) `subscript_read_to_ssa` calls `resolve_args_name` (mirror
`subscript_to_value`); (2) `check_function_scope` adds `args` to `fn_known`
(DD#75 §5). The defensive `find_array_slot` ICE stays (it's a guard, not dead
code). `f(){ echo $(( args[0]+1 )); }; f 5` → 6. Unmasking routes arith args
through the pre-existing positional rules (non-contiguous → SB5044, over-arity →
SB5043; parity with `${args[i]}`); top-level `args` still SB5031. specs
arithmetic.sh bsx.8/9/10. Full 3-mode sweep vs `499859c`: **0 regressions**; +3
native/c, +1 janet. janet arith subscript still gapped (`janet-arith-subscript-
read`).

**Native gate has no known latent bugs now — clear to resume `grind-continue`.**

## ════════ STATUS (2026-08-29) — redirect fd-dup target validated (SB4048) ════════

**On `hand-parser`.** Answering DG's "were there more bugs before resuming the
never-run grind?", I probed the sibling operator-dispatch parsers for the
silent-degrade class. arith/`[[ ]]`/subscript are clean (reject); **redirects
had the bug**. Journal 212.

`>&`/`<&` fd-dup targets weren't validated — `emit_dup_output` did
`s.parse().unwrap_or(2)` / `_ => 2`, so `>&foo`, `>&999x`, `2>&1x`, `>&-` all
silently dup'd to fd 2. Fix (DG: **reject-only**): `reject_nonfd_dup_target` in
`parse_redirect` → **SB4048** for any non-numeric/malformed target; `>&N` /
`N>&M` / `<&N` still work. `RedirectOp` gained `Copy`. specs coverage.sh
cov.86-88 (all modes). Full 3-mode sweep vs `ad5c557`: **0 regressions**, +3
each. Follow-up filed: fd dup/close as **builtins** (`fd-manipulation-builtins`).

**Next:** resume the never-run grind (`grind-continue`) — the native gate is
clean (only latent native bug left is `arith-args-subscript-codegen`, masked by
SB5031; the rest are c/janet baselines).

## ════════ STATUS (2026-08-29) — no-ambiguity param fallthrough (SB4047) ════════

**On `hand-parser`.** DG: "need to prevent ambiguity — if the compiler doesn't
know what something does, that's very bad." (Journal 211, follows 210.)

`parse_parameter_expansion` used to fall through to a bare `${name}` for any
unrecognised trailing content, **silently dropping** it (`${x&y}`, `${x:foo}`,
`${x.y}`, `${x:}`, `${x y}` → `$x`). Now the fallthrough carries the raw
remainder on a sentinel `unrecognized` operator and rejects → **SB4047**.
Two-layer model: SB4034 = known-dropped op (specific guidance); SB4047 =
genuinely unknown content. Everything sash understands (`:-`/`:?`/`:off:len`/
`:s/`, subscripts, `@`→SB4015, `!`→SB4003, strlen) is recognised earlier, so
only garbage reaches SB4047. specs var_modifiers.sh vm.un1-4. Full 3-mode sweep
vs `df6fe84`: **0 regressions, 0 gone** — nothing relied on the silent-accept.

## ════════ STATUS (2026-08-29) — non-colon param family rejected + SUBSET.md deleted ════════

**On `hand-parser`.** DG-directed (journal 210). "Isn't `${x-4}` invalid sash?"
— yes, and it was a silent-degrade gap.

- **Bug:** `split_operator` didn't recognise the non-colon default family
  (`-` `+` `=` `?`), so `${x-4}` parsed as bare `${x}` with `-4` **silently
  discarded**; `${x+4}` even returned `$x` (5) not the alternate (bash 4) — a
  silently wrong answer.
- **DG:** "clarity favors a single spelling. DD65 gives the answer." → reject all
  non-`:-`/`:?` forms **SB4034**, no compiler coercion (conversion `-`→`:-` is
  sash-convert's separate job). Kept spelling = the colon forms `:-`/`:?`.
- **Fix:** add `-`/`+`/`=`/`?` to `split_operator` one-char ops +
  `is_dropped_param_operator`. Array path self-guarded; converter (tree-sitter)
  unaffected. specs var_modifiers.sh vm.dr19-22. DD#65 non-colon addendum.
- **SUBSET.md deleted** (DG: "worse than useless" — claimed non-colon coercion,
  listed dropped `:=`/`:+` as supported). Repointed README/AGENTS/GUIDE/REVIEW at
  `notes/decisions/` as the subset authority. Corrected the journal-209/DD#65
  bare-`-` note (was wrong).

**Verification:** full 3-mode sweep vs `174164e` → **0 regressions**; +4 each
(vm.dr19-22). fmt+clippy clean.

## ════════ STATUS (2026-08-29) — int `:-` identity-element default (all 3 backends) ════════

**On `hand-parser`.** DG-directed, closing the open bug
`bug-cjanet-default-on-int-var`. Journal 209.

- **Semantic (DG):** `:-` triggers the default on the **identity element** of the
  subject's type — `""` for strings, **`0` for ints**. `${int_x:-D}` ≡
  `(x == 0 ? D : x)`. DD#62 meaningful-zero preserved; `:-` just picks 0 as its
  trigger. Was a **native behavior change** too (native returned `0` for
  `int x=0; ${x:-99}`) — all 3 backends had wrongly string-emptiness-tested an int.
- **Literal-only default (DG):** an int `:-` default `D` must be an **int
  literal** (decimal/`0x`/`0o`/`0b`) → else **SB4030** at type_check. Unifies with
  the arith-context rule (DD#77 D4).
- **Impl:** transform lowers int `:-` to `Ternary{x==0 ? D : x}` (native+c via
  SSA); janet `emit_pe_default` int branch `(if (= name 0) …)`; type_check fourth
  pass `check_int_value_defaults` (treats `$(( ))` opaque — no dup with the arith
  default check). DD#65 addendum; new task `janet-int-array-and-cell-default` (P3,
  janet int arrays broken independent of `:-`).

**Verification:** full 3-mode sweep vs HEAD `328e529` → **0 regressions** native/
c/janet; +6 native, +6 c, +5 janet new passing, c:cov.78 skip→pass. coverage.sh
cov.78/80/81 (positives), 82/83/84 (rejects), 85 (int-cell, native+c). fmt+clippy
clean; new fns pass the cog/cc gate.

## ════════ STATUS (2026) — 2 more grind bugs fixed (#3 arith, #4 redirect) ════════

**On `hand-parser`.** Two more grind-surfaced bugs closed, DG-directed ("fix #3;
reject and delete"). Journal 206.

- **Bug #3 — nested/embedded arith assignment dropped → FIXED (fork A).**
  `(( (a=2) ))`, `(( 1+(a=2) ))`, `(( a=2,(b=3) ))` (statements) used to silently
  leave the var unchanged. `lower_arith_test_stmt` now **hoists** an embedded
  assignment in a `(( … ))` **statement** into a real store
  (`hoist_arith_mutations`), matching bash. Consistent subset rule (SUBSET.md): a
  mutation belongs in a `(( … ))` statement only — a mutation in a `$(( … ))`
  **value** → SB4020, in an `if`/`while` **condition** → SB5007
  (`validate_arith_in_condition` now checks `arith_has_mutation`, so
  `if (( (a=2) ))` joins `if (( x=5 ))`), and an embedded `++`/`--`, `a[i]`, or
  mutation under `&&`/`||`/`?:` in a statement → SB4020 (never silent). The
  half-wired value arms (`incdec_to_ssa`/`compound_assign_to_ssa` + the mutation
  arms of `arith_to_ssa`) were **deleted** — documented `unreachable!`, proven by
  a 0-new-failure sweep. arithmetic.sh 31.25-36.

- **Bug #4 — redirect on a non-command → FIXED (reject + delete).** A redirect on
  a function-def / declaration / assignment is one clean spanned **SB4046** (was:
  misleading SB5011 / SB4014, and **silently dropped** for `x=5 >f`). Rejected at
  `wrap_pipeline_part`; dead `lower_redirected_function` + `function_redirects`
  machinery deleted. commands.sh 14.13-16.

New codes: SB4020 reuse (arith mutation placement), **SB4046** (redirect on
non-command). No new DD. **Verification:** two fresh HEAD-vs-change baselines
(native + c) → **0 regressions** each; 16 new tests pass native+c; `test_status`
0 native/0 c failing. fmt+clippy clean.

**Taint follow-up (2026-08-29):** the P1 I filed
(`bug-nested-bare-cmdsub-bless-taint.md`) was **mischaracterized**. DG: nested
tainted `$( … $() … )` is **correctly** rejected SB5020 — the inner tainted
result is read as an argument to the outer command, and `bless x` covers only
the outer assignment. No bypass / nondeterminism (660/660 deterministic reject);
the "flip-flop" was a **chk-id collision** (commands.sh 14.7/14.8 masked by
wordsplit.sh 14.7/14.8). Fixed: commands.sh 14.7/14.8 now expect SB5020,
renumbered to 14.17/14.18; broader collision filed
(`bug-commands-wordsplit-14x-chk-collision.md`, P2).

**Spec chk-id de-collision (2026-08-29):** audited the whole suite for the same
masking hazard and fixed **every** collision (`decollide-spec-chk-ids.md`, done):
4 cross-file pairs (arithmetic↔types `58.10-15`; convert↔types `30.7`;
misc↔wordsplit `20.3-6`; types↔wordsplit `62.11`) + within-file duplicates
(types.sh had 2-3 blocks reusing `30.8-11`/`33.11-18`; errors.sh `55.30`). Only
ids moved (tests unchanged, plan counts preserved); native+c 0 failing, and no
de-collided pair unmasked a real failure. The task carries a re-runnable audit
(cross-file collisions AND within-file duplicate `chk` must both be empty).

**Coverage grind round (2026-08-29, journal 208):** re-measured per-function
count. bug#3/#4 already removed 3 never-run fns (95.03%→95.30%); this round
flipped 3 more via `coverage.sh` cov.76-78 (`skip_balanced_squote/dquote`,
`parameter_expansion_type`) → **95.50% (67 never-run)**. cov.78 surfaced a real
**c/janet bug**: `${int_var:-default}` miscompiles (c `x_ptr` undeclared; janet
`(length int)`) — filed `bug-cjanet-default-on-int-var.md` (P2), cov.78 guarded
native. Reclassified (accounting): `collection_reader_taint`/`bare_name_tainted`
shadowed-dead (join/keys→intrinsics, removable); `extract_from_literal`/
`extract_leading_operator` need the dispatch-glued form (P3); `scan_base_hash_span`
reachability discrepancy (needs arith-tokenizer trace).

**Next (DG mandate — keep draining the A-bucket via specs):** remaining named
never-run functions (sig_scan_case/sig_record_arith_index, c-backend
emit_dup_output/latest_var_as_str/emit_arr_keys_joined, taint
collection_reader_taint, redirect/arith scanner sub-paths, fully-glued
extract_from_literal). B-candidates to confirm-then-remove: try_keys_intrinsic.
C: sysroot CLI. See `notes/coverage-accounting.md` for the full classification.

## ════════ (prior) coverage accounting reframed; 2 P1 bugs fixed ════════

**Coverage:** lines 87.69% → **92.28%**, functions → **95.03%** (native+c).
Source-based LLVM coverage is **optimization-independent** — the per-function
`count` from `cargo llvm-cov report --json` is **authoritative**. A passing
feature probe does NOT prove a specific fn ran. Two earlier P1 bugs fixed:
- **const-reassign panic** → **SB4044** (journal 204). Fork C: validate
  `check_const_reassignment` (spanned) + `define()` spanless backstop.
- **intrinsic-arity fallthrough** → **SB4045** (journal 205). Fork C: validate
  `check_cmdsub_arity` (spanned) + transform backstop.

## ════════ (prior) trap purged + exec supported ════════

**Pushed @ `e7d83b8`.** Two DG dispositions of the "accepted-in-source bash
forms" the coverage grind surfaced:



**On `hand-parser`, pushed @ `0401ec2`.** Coverage grind (DG: "keep going").
Release line coverage **87.69% → 90.80%** across the session (compiler,
native+c). Latest commits (all 0 native/c regressions, verified by full
3-mode sweep):
- `211c464` removed the dead positional-init cluster
  (`init_numeric_positional`/`init_at_var`, ~90 lines) — top-level positionals
  are validation-rejected and function positionals use the args-array (DD#60),
  so `init_one_positional` only ever hits its DD#53 empty-default. **Verified
  DG's way: remove + full-suite run confirms safety, git checkout reverts.**
- `2346c36` removed the dead `command` builtin codegen (DD#23/SB4001-shadowed).
- `7bd1a81` `spec/trace.sh` (DD#32 `--trace`/SASH_TRACE tracing).
- `88910be`/`86ac293`/`0401ec2` cov.59-61 (`$(< file)` slurp, `trap … EXIT`,
  for-in-cmdsubst pipe-while-read).

**Bugs filed this stretch:** `c-pipe-while-read-eof-extra-iter` (P3 — c does an
extra empty EOF iteration in `for x in $(cmd)`; native correct).

**Remaining uncovered = mostly not-spec-reachable or ambiguous:** `ssa::display`
(no CLI trigger — only Rust tests / debug); `extract_from_literal` (compact
`[[a==b]]`, partly covered, attribution unclear); lexer `scan_dq_backtick`
(backtick-in-dquotes literalized — diverges from bash, possibly a gap not a
feature) / `scan_ansi_c_octal`; exec paths (`build_argv_on_stack`,
`emit_exec_redirects_only`). The clear, safe, gate-verifiable targets are
largely exhausted; further progress needs DG input on the ambiguous forms.



**On `hand-parser`, pushed @ `88910be`.** Coverage grind continued (DG: "keep
going"). Release line coverage **87.69% → 90.36%** across the session (compiler,
native+c; 610 fewer missed lines). Commits after the ternary work (all 0
native/c regressions):
- `2346c36` removed the dead `command` builtin codegen (`emit_command_builtin`
  + `BuiltinCmd::CommandBuiltin` chain, ~79 lines) — DD#23 rejects `command`
  via SB4001 (`check_dynexec`); the codegen was shadowed vestige. cov.58 pins
  it; shrinks the command-exec attack surface.
- `7bd1a81` `spec/trace.sh` (NEW, native-only) covers DD#32 tracing
  (`--trace` + `SASH_TRACE=1` → `emit_branch_outcome_trace` /
  `intern_trace_terminator`, ~68 lines). tr.1-3 (if.then/if.else/while).
- `88910be` cov.59 covers `$(< file)` slurp parse (parse_slurp_form).

**Candidate-dead (needs careful verification, NOT removed):**
`codegen/setup.rs::init_numeric_positional` + `init_at_var` (+ their only caller
`init_one_positional`). Top-level `$1`/`$@`/`$#` are all validation-rejected
(SB5024/SB5045) and function positionals inline via the args-array (DD#60), so
`collect_positional_refs` only ever surfaces `$?`/`$$` (which
`init_positional_params` skips) — the init cluster (~90 lines) appears dead but
removing positional codegen is delicate; verify `scan_value_for_vars` can never
surface a numeric/`@` ref before deleting.

NEXT (still a path forward): the candidate-dead positional cluster; remaining
reachable-untested from the scan (handle_trap, emit_cmdsubst_pipe_while_read,
lexer scan_dq_backtick/scan_ansi_c_octal). Re-measure: `/tmp/rcov.sh`.

## ════════ STATUS (2026-08-27) — ternary fixes + coverage grind (89.66%) ════════

**On `hand-parser`, pushed @ `9218f23`.** Coverage grind continued (DG: "no
rest, continue while a path forward exists"). Release line coverage **87.69%
→ 89.66%** this session (compiler, native+c, no janet; ~447 fewer missed
lines). Commits this run (all 0 native/c regressions):
- `29bdd25` removed dead `$@` transform machinery (post-DD#81, ~82 lines).
- `d66b831` arith-operator coverage cov.29-48 (unary/inc-dec/compound-assign/
  comma).
- `ddeb1d4` + `3971248` **retired `${#…}` entirely** (DG Policy C) — see block
  below.
- `2fcacd8` **c-backend ternary was silently empty** (no lower_value_as_str
  arm) → fixed via lower_value_as_int + emit_ternary_as_int_c (if/else temp,
  lazy). cov.49-56.
- `9218f23` **native ternary was eager** (evaluated both branches → leaked a
  side-effecting untaken branch's div-by-zero/overflow) → both sites
  (lower_ternary_value + new lower_ternary_int) rewritten to brif+phi merge.
  Now native==c==bash (lazy). cov.57 pins it. Closed native-ternary-eager-eval.

3 bugs found+fixed (${#a[N]} silent-0, c-ternary empty, native-ternary eager),
1 filed (janet-unset-len-nil P3). NEXT (coverage grind, still a path forward):
more reachable-untested fns from the "fully-uncovered function" scan
(emit_cmdsubst_pipe_while_read, parse_slurp_form, extract_from_literal,
init_at_var, ssa::display/emit_branch_outcome_trace behind flags). Re-measure:
`/tmp/rcov.sh`; map: `/tmp/rcov_missing.txt`.



**On `hand-parser` (committing).** Coverage grind surfaced dead
`emit_strlen_elem`; probing it found a bug: `${#a[N]}` (element length)
silently returned 0 (element-length was half-wired, no parser producer). DG
chose **Policy C**: drop the `${#…}` length sugar *entirely* — `$(len x)`
(value) / `(len x)` (arith) is the sole length spelling; keep `len`; converter
rewrites bash `${#x}`→`$(len x)`. Impl: parser `${#…}`→SB4038 everywhere
(`len_retired`), removed arith `ATok::LenOf`, deleted dead `emit_strlen_elem`.
`$(len …)` untouched (converges on the same live `strlen`/`arrlen` operators).
Migrated ~10 spec sites (types/arithmetic/var_modifiers); repurposed 75.4-75.6
to assert the retirement. DD#75 §2/§4/§6 amended; SUBSET.md; journal 191. Native
+ c gate 0 regressions; guarded types.sh 30.11 native,c (pre-existing janet
unset+len nil crash → task `janet-unset-len-nil`). Converter rewrite DONE
(Commit 2: `${#x}`→`$(len x)`, spec convert.sh 57.13c).

## ════════ STATUS (2026-08-27) — spec coverage grind (in progress) ════════

**On `hand-parser`.** DG asked to understand spec-suite coverage, delete dead
code, and add specs toward every spec-reachable line (ignoring janet). Reliable
release baseline (native+c, compiler only): **87.69% lines** (2750 missed).
Release coverage IS reliable (a debug-mode detour + "unreliable" claim were
retracted — every verified "missing" line was genuinely dead-or-untested;
the 91%↔48% debug swing is just line-counting granularity). Harness: needs
`cargo llvm-cov clean --workspace` to force instrumentation; rebuild whole
workspace (convert/expand) before those specs; force-rebuild uninstrumented
after. Batches pushed (0 regressions, native 0): `4d94eb9` 4 dead methods;
`0f36b7a` parser-shadowed check_export_or_declare + coverage.sh cov.1-3;
`877d55e` coverage.sh cov.4-10 (7 reachable-untested diagnostics);
cov.11-28 (more reachable diagnostics + redirect forms); `ed95432` dead
`$@`-exec codegen path; **dead `$@`-expansion transform machinery**
(`expand_dollar_at_*`, ~82 lines — all no-ops post-DD#81 `$@`-rejection).
High-yield pattern: recent feature *removals* ($@, case, var-modifiers)
leave whole dead functions still line-counted — delete + verify
behaviour-preserved. Method: per cluster — shadowed-dead → remove+verify;
reachable → spec. Multi-session grind; journals 188-190.

## ════════ STATUS (2026-08-26, cont.) — taint bypass switches (DD#82) ════════

**On `hand-parser` (committing).** DG: a public fork could rip out taint and
overshadow the original — so provide a *supported* bypass (removes the fork
incentive), and use the two-layer split to test the SSA backstop in isolation.
Added `--no-taint-cst` (CST front-end off, SSA backstop on — native) and
`--no-taint` (all taint off). Taint-only: `--no-taint` + `$@` is still SB5045.
`lib.rs` gates `check_taint`/`verify_ssa_taint`; `SASH_CFLAGS` (common.sh)
injects compile flags for tests. specs taint_switch.sh sw.1–sw.7. 0
regressions native/c/janet; native gate 0; 15 new passing. DD#82, journal 186.
NEXT: SSA hardening (cmd_subst_taint operand propagation + argv/mapfile array
classification + Test/Redirect use-checks) so `--no-taint-cst` catches the full
laundering class (task `ssa-taint-cmdsub-operand-propagation`).

## ════════ STATUS (2026-08-26, cont.) — SSA operand propagation (partial) ════════

**On `hand-parser` (committing).** With `--no-taint-cst` able to exercise the
SSA pass, implemented `cmd_subst_taint` operand propagation (echo/dtoi/itod +
join/keys via `collection_reader_taint`) — matches every other transform value;
catches `x=$(dtoi $tainted)` under `--no-taint-cst` (sw.8). SSA messages
neutralised. **Found the SSA backstop is far from a complete gate**: capture-form
builtins (`$(join @a)`) bypass `cmd_subst_taint` (they lower via
`CaptureEnd{tainted}`, a transform-time bool); array taint isn't classified at
SSA (versioning); inline args/`[[ ]]`/redirect positions aren't in
`check_inst_uses`. Shipped the principled propagation; scoped the rest as
`ssa-taint-full-backstop` (P2, supersedes `-cmdsub-operand-propagation`). CST
stays the authoritative gate. 0 regressions; native gate 0. Journal 187.

## ════════ STATUS (2026-08-26, cont.) — taint laundering bypass CLOSED ════════

**On `hand-parser` (committing).** DG demo (`argv 1 a; echo $(join a)`) surfaced
a **taint-laundering bypass class**: a `$( )` that READS already-tainted data
was only checked in argument position; scalar/subscript-assign RHS, array
literals, `[[ ]]` operands, redirect targets, and nested cmdsubs silently
compiled and leaked at runtime. Also found: bare-name `join a` wasn't a reader
use-site (only `keys` was), and `[[ ]]` operands + redirect targets weren't
taint-checked at all (direct tainted vars leaked too). DG's model: every `$( )`
is internally `_tmp=$(…)` reusing the one read-checking machinery. Fixed CST
(authoritative): extracted `check_cmdsub_inner_reads`, applied it position-
uniformly; added a `Test` arm to `check_expr_taint`, redirect-target checks,
and `join` to `check_collection_reader_taint`. SSA defense-in-depth
(`cmd_subst_taint` operand propagation) filed P2 (`ssa-taint-cmdsub-operand-
propagation`) — can't fire on CST-cleared programs, backstop-only. specs
taint.sh 60.37–60.46. 0 regressions native/c/janet; native gate 0; 29 new
passing. Journal 185. DD#24 amended (reads are use-sites in every position).
Gap-fill (DG "protect against regressions"): taint.sh 60.47–60.55 extend the
matrix (echo-scalar, mapfile source, keys-in-array-literal, subscript-in-test,
bare-name in test/redirect, + 2 clean-value positive controls). All pass 3
modes; 0 regressions; native gate 0.

## ════════ STATUS (2026-08-26) — `$@`/`$*` universally rejected (Option A) ════════

**On `hand-parser` (committing).** DG: "haven't we universally rejected `$*`
and `$@` now?" — no. The `$@` drop left three gaps: in-function `$*` **silently
compiled** (SB5045 matched only `@`); top-level `$@`/`$*` were SB5024 but
mislabeled "function-local" (they're rejected everywhere); braced `${@}`/`${*}`
(and `${@:-x}`) never fired SB5045 at all (`scan_param_name` drops the `@`/`*`
sigil → empty-name ParameterExpansion). **Option A** (DG): `$@`/`$*` are *not in
the language* — one identity (SB5045 "sash has no `$@`/`$*`") fires **everywhere**
with a context-sensitive hint (in-fn → `@args`/`$(join args)`; top-level →
`$(argv 1)`/`$(arg N)`); only `$1`/`$#` stay function-local (SB5024). Parser:
`${@…}`/`${*…}` → `SimpleExpansion` (array `[@]`/`[*]` untouched, still SB4038).
`check_dollar_at` now walks everywhere + arith. specs functions.sh 81.15/81.16,
errors.sh e.dd46.t6/t7 (+t1/t3 assertions SB5024→SB5045). 0 regressions
native/c/janet; native gate 0; 12 new passing. Journal 184. DD#46/#75/#81 amended.

## ════════ STATUS (2026-08-25, cont.) — top-level `$@`/`$*` message → `$(argv 1)` ════════

**On `hand-parser` (uncommitted → committing).** Follow-up to the `$@` drop:
the top-level SB5024 diagnostic for `$@`/`$*` directed to `$(argv)`, but that
includes the program name at index 0 (spec/argv.sh av.s1). The function-local
`$@`/`$*` are the **user args** = `$(argv 1)` (av.s2), matching DD#60.
`flag_positional` (validate.rs) now says "use $(argv 1) for the user-arg slice
(or $(arg N) for one)". Rejection itself unchanged (still SB5024); only the
suggested replacement corrected. `$1`→`$(arg N)`, `$#`→`$(argc)` untouched.
DD46-positional-local.md updated; specs errors.sh e.dd46.t1/t3. 0 regressions
native/c/janet; native gate 0. Journal 183.

## ════════ STATUS (2026-08-25, cont.) — drop `$@` (SB5045) ════════

**Pushed on `hand-parser`.** DG caught that `$@` breaks DD#75 §4's "`@` is the
sole splat" (a `$`-sigil that splats). `validate::check_dollar_at` rejects
in-function `$@`/`${@}` → SB5045 (use `@args`, or `$(join args)` joined);
top-level stays SB5024; `$1`/`$#` untouched (scalar). DD#75 §5/line 172 + DD#81
note corrected ($@ removed, not sugar). `"@foo"` stays a literal (no
false-positives on `user@host`); the converter warns — filed
`converter-dollar-at-migration`. specs functions.sh 81.14, control_flow.sh
cf.f11 (→`$(join args)`, native,c)/cf.f12 (→`@args`). 0 regressions; native 0.
Journal 182.

## ════════ STATUS (2026-08-25, cont.) — DD#81 COMPLETE (Parts A + B) ════════

**Pushed on `hand-parser`.** DD#81 "function-call discipline" fully implemented.
Part B (exact arity) landed: `transform/functions.rs::analyze_arity` infers a
per-function `Arity` (Fixed/Variadic) from `args` usage — `$N`/`${args[k]}`
literal → fixed index; `@args`/`$(len args)`/`${#args}`/`$#`/`$@`/dynamic-index →
variadic; **arith scanned** (`$#` inside `$(( ))`). Checked exactly on all three
call paths (plain/pipeline/`$( )` capture): SB5042 too-few, SB5043 too-many,
SB5044 hole (rule (a)). DG's no-op idiom (`: $1` to acknowledge an unused param,
`@args` for variadic/high-index) resolved the rule-(a) tension; specs 67.2/75.20/
75.22 updated. Old min-arity scanner (~130 lines) removed. specs functions.sh
81.9–81.13; 0 regressions native/c/janet; native gate 0. Journal 181.

## ════════ STATUS (2026-08-25, cont.) — DD#81 structural stages COMPLETE; Part B (arity) filed ════════

**Pushed on `hand-parser` (c09a4a3, f9f16f5, 9fc930c) + DD 820f0cd.** DD#81
("function-call discipline") structural work done — three reachable ICEs fixed
and the "one name, one meaning" principle implemented:

- **Part A** (c09a4a3) — reject command/function shadowing + forward refs
  (SB5041); unified the `$(f)` gate onto the plain-call `function_bodies` gate.
- **func_constants removal** (f9f16f5) — the forward-`$(f)` machinery, now
  provably dead, deleted (−216 lines, 8 files).
- **`$(f arg)`** (9fc930c) — the capture path now carries + sets up the callee's
  `args` (was an `undefined array` ICE); unified onto the args-array model.
- **message cleanup** — MOOT: Part A + SB5011 catch every unresolved/forward
  bareword at compile, so the sash-rt `bug: bare command` backstop is now an
  unreachable internal invariant (correctly signals a compiler bug if ever hit).

**Remaining: DD#81 Part B — uniform exact arity from `args` usage**
(`notes/tasks/dd81-part-b-arity.md`, ready, P2). A feature-sized new
signature-inference analysis (fixed vs variadic from `@args`/`${args[i]}`/`$(len
args)`/dynamic-index; holes SB5044; exact checking SB5042/43 on all call paths).
The current `$N`-only min-arity check (plain+pipeline) is the starting point.

0 regressions native/c/janet throughout; native gate 0. Journals 178–180.

## ════════ STATUS (2026-08-25, cont.) — DD#81: `$(f arg)` passes args (was ICE) ════════

**Committed on `hand-parser`.** Third DD#81 stage, a reachable ICE fix. `s=$(f
hello)` / `echo $(f x)` on a defined function reading its args crashed
(`undefined array`): the capture path (`capture_func_into`) used the old
positional model (`setup_func_args`, `$1..$9`) and was passed `&[]`, so the
body's `$1`≡`${args[0]}` (DD#75) found no `args` array. Fix: `SsaValue::FuncCall`
now carries `args`, and `capture_func_into` uses the same args-array model as
`inline_function_call` (`save_caller_args`/`setup_callee_args`). Dead positional
helpers removed. specs functions.sh 81.7/81.8 (native,c); 0 regressions, native
gate 0. Journal 180.

**DD#81 remaining:** Part B uniform arity (SB5042/43/44 — capture path now
carries the arg count); `bug: bare command` → `command not found`.

## ════════ STATUS (2026-08-25, cont.) — DD#81: remove dead func_constants machinery ════════

**Committed on `hand-parser`.** Second DD#81 stage. With forward `$(f)` rejected
(Part A), a bare `SsaValue::FuncCall` can never reach codegen (defined `$(f)`
always lifts to a captured temp; undefined resolves external). Confirmed dead
(instrumented panic → 0 native regressions), then removed `fold_function_outputs`
+ `try_fold_block`, `lower_func_call_value`, the setup intern loop, and the
`func_constants` parameter threaded through the whole value-lowering tree (139
lines / 8 files + the InstCtx field). The `FuncCall` arm now panics
"should-have-been-lifted" (joins IntFuncCall/CmdSubstBody). Net −216 lines; 0
regressions native/c/janet, 0 new (behavior-preserving). Journal 179.

**DD#81 remaining:** `$(f arg)` arg passing; Part B uniform arity
(SB5042/43/44); `bug: bare command` → `command not found`.

## ════════ STATUS (2026-08-25) — DD#81 Part A: reject shadowing (name resolution) ════════

**Committed on `hand-parser`.** First DD#81 stage. Unified the `$(f)` capture
gate onto the plain-call `function_bodies` gate (defined-so-far / execution
order): a not-yet-defined `$(f)` is now an external command substitution, not the
broken `known_functions`→`func_constants` path. Added **SB5041**: a bareword used
as an external command, then defined as `fn NAME`, is a shadowing collision
(`externally_used` set, gated off during dead `fn_` template lowering via
`lowering_fn_template`). Rejects dynamic shadowing + forward refs (both `$(f)`
and plain), keeps helper-below-caller / static-override / normal calls. specs
functions.sh 81.1–81.6; 0 regressions native/c/janet; native gate 0. Journal 178.

**DD#81 remaining (next stages):** remove now-dead `func_constants` /
`lower_func_call_value` / `fold_function_outputs`; fix `$(f arg)` arg passing;
Part B uniform arity (SB5042/43/44 from `args` usage); `bug: bare command` →
`command not found`. Tracked in the session todos.

## ════════ STATUS (2026-08-24, cont.) — bug fix: `[[ -o ]]`/`[[ -R ]]` codegen ICE ════════

**Committed on `hand-parser`.** Reachability audit for the ICE-arms keep+cite
pass surfaced a real bug: the parser's `is_unary_test_op` accepts `-v`/`-o`/`-R`,
but codegen (tests.rs) implements only `-n`/`-z`/`-i`/`-t` + file tests, and
validate rejected only `-v` (SB5028). So `[[ -o errexit ]]` and `[[ -R x ]]`
crashed the compiler (`ICE: unimplemented unary test operator`). Fix follows the
`-v` precedent: `unsupported_unary_test_hint(op)` rejects all three at validate
(SB5028); the now-unreachable tests.rs panic gets a keep+cite `// INVARIANT:`
comment. specs wordsplit.sh 62.20/62.21 (avoided a pre-existing `chk 62.11`
collision). 0 regressions native/c/janet; 6 new passing; native gate 0.
Journal 177. Filed `unary-test-ops-k-g-u` (reverse gap: parser under-accepts
-k/-g/-u that codegen already implements).

## ════════ STATUS (2026-08-24, cont.) — eliminate-ice-arms P1 Pattern-A (VarSlot match) ════════

**Committed on `hand-parser`.** P1 Pattern-A: the five codegen sites that
dispatched on `VarSlot::is_int()` then reached through a panicking accessor
(`as_int`/`str_ptr`/`str_len`) now match the enum directly
(`match *sv { VarSlot::Int(v) => .., VarSlot::Str { ptr, len } => .. }`) — total
by construction. Sites: lower_var_value, index_var_to_int, lower_var_as_int
(values.rs), handle_regular_assign (inst_handlers.rs), emit_capture_end
(dispatch.rs). Consequence: `as_int`/`is_int`/`as_str` fell to zero callers and
were **deleted** — removing 2 more ICE panic arms (`as_int`/`as_str`) outright.
`str_ptr`/`str_len` kept for the ~40 Pattern-B callers (known-Str slots) with an
`// INVARIANT:` keep+cite comment. Behavior-preserving (`match` ≡ the 2-arm
if/else) → no new specs. 0 regressions native/c/janet; native gate 0. Journal 176.

Review remaining: keep+cite for P3 (undefined var/array/index → validate SB5031)
and P7 (should-have-been-lifted); reachability check on the
"unimplemented/unsupported operator" panics (tests.rs test ops,
value_handlers.rs param-expansion op) before cite-vs-fix.

## ════════ STATUS (2026-08-24, cont.) — eliminate-ice-arms P2 (lazy literal interner) ════════

**Committed on `hand-parser`.** P2 of the per-pattern ICE-arms review: the five
codegen sites that `panic!`'d on a `literals`-map miss (`"ICE: missing literal"`
/ `"ICE: assoc array key not interned"`) now go through
`values::intern_or_declare` — read the map, or declare+define fresh anonymous
data on a miss. Total by construction (no ICE). No signature churn: `module` is
already `&mut` everywhere, and `declare_anonymous_data` needs no unique name.
Sites: glob dir/operand (2), join separator, const-folded string, assoc/indexed
key. Behavior-preserving (the panics were unreachable drift backstops) → no new
specs. 0 regressions native/c/janet; native gate 0. Journal 175.

Review remaining: P1 Pattern-A `match` conversions (types.rs flag-checked
`is_int` sites); keep+cite for the table / P3 / P5 / P7 sites.

## ════════ STATUS (2026-08-24, cont.) — eliminate-ice-arms cohort A (join/CheckedArithOp/case) ════════

**Committed on `hand-parser`.** Three ICE arms from the per-pattern review
(reviewed interactively with DG) retired:
- **P8** (`3fb3458`) — `$(join)` 0/>2-arg was a **reachable compiler panic**; now
  `validate::check_join_arity` → SB5040. spec types.sh 33.23-24.
- **P4** — `ssa::CheckedArithOp{Add,Sub,Mul}` makes `emit_checked_arith(_c)` total
  (2 arms gone). Dispatch maps each ArithOp variant directly.
- **P6** — case conversion is now a `bool` (whole-string toupper/tolower), not a
  stringly-typed `"^^"/",,"` match (2 arms gone; the first-char `^`/`,` family
  had no producer since DD#65). Removed dead `toupper_first`/`tolower_first`
  front-end refs; filed `remove-dead-case-first-runtime` (sash-rt twin).

0 regressions native/c/janet; native gate 0. Journal 174. Review remaining:
P2 (lazy interner ~5 arms), P1 Pattern-A `match` conversions, keep+cite for the
table/P3/P7 sites.

## ════════ STATUS (2026-08-24) — index-var is_int-total + $@[i] ICE + atoi audit ════════

**Committed on `hand-parser`.** Follow-through on the P1/ICE-arms discussion (DG:
"arrays/index-vars are always Str-slots" is a convention tests can't prove).
- `codegen/values.rs`: merged the argv/table index-var reads into
  `index_var_to_int`, `is_int`-total (int slot read directly, string slot
  `atoi`'d) — removes the `str_ptr`-on-int panic risk, mirrors `lower_var_as_int`.
- `validate.rs`: `$@[i]` / `$*[i]` (subscripting the positional splat) was a
  codegen ICE (`undefined array: @`); now SB5031 via `check_subscript_index`.
  spec subscript.sh 80.20-21.
- **atoi audit** (DG follow-up): every atoi is a string→int coercion; no atoi on
  an already-int value. Native clean; filed `c-argv-index-isint-parity` (the C
  argv-index atoi's the string `__fi` unguarded — parity gap, not a live bug).

0 regressions native/c/janet; native gate 0. Journal 173. The eliminate-ice-arms
review continues — P1 index-var/Pattern-B done; table sites keep+cite; P2 (lazy
interner), P4 (CheckedArithOp), P6/P8 still open per the per-pattern review.

## ════════ STATUS (2026-08-23) — undeclared collection read ICE → SB5031 ════════

**Committed on `hand-parser`.** `echo ${z[0]}` (undeclared array/map read)
panicked at codegen (`ICE: undefined array`) instead of a clean error — found
during DD#80. `validate::check_expr_reads`'s `Subscript` arm now flags an
undeclared collection *name* (SB5031), mirroring the arith path (which already
did); `args` skipped (top-level → SB5024, in-function → in scope). spec
undeclared.sh ud.19-21 (×3). Filed `arith-args-subscript-codegen` (a masked
codegen gap: bare `$(( args[0] ))` never resolves `args`→`__argsN`; the SB5031
false-positive was protective). 0 regressions; native gate 0. Journal 172.

## ════════ STATUS (2026-08-22) — DD#80 braced subscript key/index discipline ════════

**Committed on `hand-parser`.** DD#80 (companion to DD#79): `${a[K]}` / `${m[K]}`
take a **direct, unambiguous** key — no *implicit* arithmetic; computation uses
an explicit nested context. `bare in (( )) = arithmetic; ${ } = direct key.`

- Parse: `try_subscript` now captures the index balanced/quote-aware
  (`capture_balanced_subscript`) and re-parses it via `parse_expr`, so nested
  `${…}`/`$(( … ))`/quoted keys work and lower to SSA temporaries for free
  (`${a[${a[0]}]}` was SB4035 → now works). A bare dollar-subscript `$b[0]` is
  kept ambiguous (use `${b[0]}`).
- Reject: **SB4041** (new, `validate::check_subscript_index`) for an
  ambiguous/implicit bare index (`${a[a[0]]}`, `${a[i+1]}`, `${a[$b[0]]}`);
  **SB4030** (reused type-check string→int) for a string key on an int array
  (`${a[foo]}`, bare `${a[i]}`, `${a["5"]}`). Catches word AND arith contexts.
- Fixes `bug-braced-nested-subscript-index` (the silent inner-cell read) +
  the wider silent-`0` footgun. Aligned with DD#33 (bare integer on a map is
  quoted). Filed `converter-braced-subscript-index` (follow-up).

spec/subscript.sh 80.1–80.19 (57 = 19×3). 0 regressions native/c/janet; 51 new
passing; native gate 0. DD#80, journal 171. This resolved the design thread that
began with the `arith-bare-subscript-followups` edge-2 finding.

## ════════ STATUS (2026-08-21, cont.) — arith-bare-subscript-followups DONE (all 4 edges) ════════

**Pushed on `hand-parser`.** Task `arith-bare-subscript-followups` complete.

- **Edge 1** (`$(( $a[i] ))`): DG chose the diagnostic route. A read-the-cell fix
  silently mis-read variable indices (`$a[$i]` rebuilt as `"$a[ $i ]"` via the
  arith token→string→re-lex path → read a[0]); reverted. `arith_bad_literal_error`
  now special-cases a stray `[`/`]` → "use `a[i]` or `${a[i]}`". Specs dsx.1-2.
- **Edge 2** (`$(( a[a[0]] ))`): the core word lexer broke words on doubled `]]`
  (for `[[ ]]`), corrupting a bare nested cell read (ends in `]]`).
  `scan_word_chars` now tracks subscript depth: `]]` closes two subscripts at
  depth>0, delimits `[[ ]]` only at depth 0. Specs nsx.1-3.
- **Edge 4** (substring/sed on a cell): parity with scalars. Sed desugars to
  `$(sub|gsub "${a[i]}" …)` (3 backends free); substring lowers via a new
  source-carrying `SsaValue::Substring` (native/c/janet). Specs scx.16-22.
- Filed `bug-braced-nested-subscript-index` (pre-existing, separate path:
  `${a[a[0]]}` returns the inner cell).

Commits `f31d2c9` (edges 1+2), edge 4 (this). 0 regressions native/c/janet;
journals 169, 170. Remaining native backlog: `eliminate-ice-arms-via-types`,
`audit-string-keyed-builtin-dispatch`, doc task.

## ════════ STATUS (2026-08-21, cleanup) — retire legacy sed/regex_replace machinery ════════

**Committed on `hand-parser`.** Task `retire-legacy-sed-regex-replace-machinery`
done. Removed the unreachable legacy `:s`/`=~` `regex_replace` path
(`try_regex_substitution` in transform; the `regex_replace` + `/`|`//` codegen
arms and `emit_pattern_replace`/`emit_regex_replace_op`/`intern_literal_value`;
the dead `intern_param_expansion` guards). The LIVE `$(sub)`/`$(gsub)`
(`SsaValue::RegexReplace` → `lower_regex_replace_value` → `sash_regex_replace`)
path is untouched.

Two bugs fixed in passing (both were masked by the machinery):
- `try_regex_substitution` hijacked any command whose 1st arg looked like
  `s/…/…/` → `/bin/echo s/a/b/` printed nothing. Now runs literally.
- **`is_regex_mutation` SB5011 bypass** (`resolve.rs` + `validate.rs`): since
  `=~` desugars at parse to an Assignment (not a Command), those guards only
  ever matched *real* commands, exempting them from DD#23 resolution → an
  unresolvable single-`s/`-arg command (`nosuchcmd s/a/b/`) compiled then hit a
  runtime `bug: bare command` abort. Removed both guards+fns → proper SB5011.

Specs regex.sh §H (63.h1–h3, ×3 modes = 9 new passing); 0 regressions native/c/
janet. Journal 168. Remaining native backlog: `arith-bare-subscript-followups`,
`eliminate-ice-arms-via-types`, `audit-string-keyed-builtin-dispatch`, doc task.

## ════════ STATUS (2026-08-21, backlog sweep cont².) — args-in-len + len-undefined ════════

Combined effort from the `len-undefined` discussion (DG approved). Three parts,
all 0-regression across native/c/janet:

- **`9ce861e` args in len/join/keys + top-level reject** — `$(len args)`/
  `$(join args)`/`$(keys args)` now resolve the implicit array (was a silent 0);
  `resolve_args_name` at the intrinsic sites + type_check `check_function`
  registers `args` as a function-scope collection. Top-level `@args`/`${args[i]}`
  → SB5024 (function-local). Specs 75.23-75.26.
- **`1fcc94a` `len <undefined>` → SB5039** — a dedicated `validate.rs`
  `check_len_operands` pass, NOT the type_check route the task sketched. Key
  insight (DG): the loop-var gap is only in *type_check*'s env; validate's SB5031
  `collect_scope_names` already tracks `for` vars / decls / read targets, so
  `for e in @a; do $(len e); done` is accepted for free. Also taught
  collect_scope_names about `argv [N] ARR` targets. Specs 75.27-75.30.

Backlog sweep total this session: arg-negative-index (SB5038); bare-nested-cmdsub
(lexer `))` split); arith-disposition 0x/0o/0b; args-in-len + len-undefined.
`const-fold-command-name` discarded (DG: "gpp territory"). Remaining native:
`arith-bare-subscript-followups`, `eliminate-ice-arms-via-types`,
`audit-string-keyed-builtin-dispatch`, cleanups (retire regex_replace, strip
runtime), + the doc task.

## ════════ STATUS (2026-08-21, backlog sweep cont.) — 3rd fix: arith 0x/0o/0b disposition defaults ════════

**Pushed `2e5916f`.** `arith-disposition-hex-octal-default`: the arith `:-`
default now accepts DD#72 `0x`/`0o`/`0b` literals (was decimal-only, SB4030).
New shared `parse::parse_dd72_literal`; type_check D4 widened; native/c convert
to decimal at SSA level (`canonicalize_arith_default`, arith-scoped so string
`${s:-0xff}` keeps "0xff"); janet got its missing `ArithExpr::Expansion` arm
(emits `(_num …)`, was an empty operand → advances `janet-arith-subscript-read`).
Specs d4.2/d4.4-d4.7b. 0 regressions all 3 modes.

**Backlog sweep so far (all 0-regression, pushed):** arg-negative-index (SB5038)
`960c9dd`; bare-nested-cmdsub (lexer `))` split) `caa4a35`; arith-disposition
`2e5916f`. Remaining native: `len-undefined-reject` (needs loop-var/param
tracking), `const-fold-command-name` (feature), `arith-bare-subscript-followups`,
+ args-model polish (`$(len/join/keys args)`, `$*`→`$(join args)`) + the doc task
(`docs-tail-recursion-args-model`). janet residuals tracked in
`janet-arith-subscript-read`.

## ════════ STATUS (2026-08-21, backlog sweep) — native backlog: 2 parser/validator bugs fixed ════════

Working the native backlog (DG: "keep after the backlog"). Two bugs fixed, each
0-regression across native/c/janet:

- **`arg-negative-index-compile-reject`** (`960c9dd`): `$(arg -1)` compiled
  (only aborting at runtime) despite DD#60's compile-time guarantee. Now
  `arg_index_diag` rejects a negative integer literal index with **SB5038**
  (both `$(arg -N)` and `$(arg -N DEFAULT)`). Specs argv.sh av.t7 (strengthened
  to assert SB5038) + av.t7b.
- **`bare-nested-cmdsub-arg`** (`caa4a35`): a bare nested `$(f $(g))` was a parse
  error (SB4014). Root cause: logos lexes `))` as one `DblParenClose`, but that
  token is arith-only; in `$(f $(g))` the `))` are two cmdsub closes.
  `split_dbl_paren_close` splits `DblParenClose`→`RParen` when `arith_depth==0`.
  Specs commands.sh 14.7/14.8.

Process note: a complexity-probe `git commit -F /tmp/j.txt` accidentally landed a
junk `x` commit when the gate passed; squashed it out (soft-reset, not pushed)
and restored `notes/misses.md` (never stage it). Don't probe the gate with a real
`git commit` — the FAIL output already names the function on a real attempt.

Remaining native backlog: `len-undefined-reject`, `const-fold-command-name`,
`arith-bare-subscript-followups`, `arith-disposition-hex-octal-default`, + the
args-model polish (`$(len/join/keys args)`, `$*`→`$(join args)`) and the doc task.

## ════════ STATUS (2026-08-21, later²) — FIX: nested-paren sexpr arg (arith collector) ════════

**Pushed `ca79d0f`.** Fixed the P2 bug found during tail-recursion work: a DD#58
arith sexpr call with a **parenthesised arg** — `(dbl (2 + 1))` — silently gave
0 (and `(dbl (2))` was an SB4020 parse error).

Root cause: the `$(( ))`/`(( ))` content collector tracked paren depth
asymmetrically — `((`/`$((`/`))` counted as ±1 units, but a single `(` pushed
text WITHOUT incrementing depth. So `(dbl (2))`'s two single `(` never raised
depth, and the trailing `))` (one `DblParenClose` token) closed the content
early, truncating `(dbl (2))` → `(dbl (2`.

Fix: count **individual** parens — depth starts at 2 (the consumed `((`/`$((`),
each `(` is +1 / `)` is −1, `((`/`))` are +2/−2 shortcuts. DG's steer: inside
`(( ))` you're already in arithmetic, so the natural form is a plain
parenthetical `(dbl (2 + 1))`, not a redundant nested `$(( ))` — and that plain
form was exactly what didn't parse.

Verified 6 = dbl(3) on native+c+janet; 0 spec regressions all 3 modes. Specs
types.sh 58.14/58.15. (This also clears the accumulator-ergonomics caveat noted
for tail recursion — a nested arith arg now works directly.)

## ════════ STATUS (2026-08-21, later) — RECURSION: reject non-tail, tail→loop ════════

**Pushed `94dac99`.** Recursion crashed the compiler (unbounded inlining → stack
overflow). Now, per DD#27 (amended):

- **Non-tail + all mutual recursion → SB5037** (clean reject, no crash). An
  inline-stack (`VersionState.inlining`) at all three inline sites catches a
  call to a function already being inlined.
- **Direct self tail-recursion → a LOOP.** DG's cut: "support tail recursion,
  reject the rest." A self-call that is a `return` producer is tail by
  definition (nothing runs after a return) — so no tail-position analysis. The
  first inline makes the body entry the loop header; `return F newargs` rebinds
  F's implicit `args` array (DD#75 §5) and branches back (`emit_tail_call`).
  Loop-carried args needs **no manual phi** — all SSA versions of a name share
  one Cranelift Variable (auto-merge at the join, `codegen/setup.rs`).
  - Mechanism (not a max-depth counter — DG's steer): an inline stack is
    precise (catches direct + mutual, no arbitrary limit, identifies the edge).
  - `return F …` is the only tail form (required for `-> int`/predicate per
    DD#74, uniform for void too). Accumulator style: bind locals then
    `return $(( (go n1 a1) ))`.
  - native+c via the SSA loop; janet keeps its own recursive emit (same
    results). All 3 modes reject non-tail/mutual. Specs `functions.sh` 27.1–27.5.

Also: transform diagnostics are now exact-deduped (a body is lowered as a
template AND at each inline site, so a body-level diag repeated).

**Found + filed a pre-existing bug** (`bug-arith-sexpr-nested-cmdsub-arg`, P2): a
nested `$(( ))` arg *directly* to an arith-sexpr call (`(dbl $(( 2+1 )))`)
silently lowers to 0. Independent of the args model / tail recursion (a plain
`(dbl 3)` and a bound-local arg both work). Accumulator style sidesteps it.

0 spec regressions across native/c/janet.

## ════════ STATUS (2026-08-21) — DD#75 §5 args model SHIPPED (0-indexed); `shift` dropped ════════

**Pushed `f505e2d`.** Function positionals (`$1..$9`/`$@`/`$#`) are now backed
by a real **0-indexed** `args` array, not scalar `"1".."9"/"#"` vars. DG's key
insight: `args` is a compiler-managed abstraction, so each inline mints a
**fresh unique array name** (`__args0`, `__args1`, …) materialized once from the
call args — nested/recursive calls never collide, so **no SSA save/restore**
(only the current name, a cheap string).

**Fixed a pre-existing bug:** a function reading a positional *after calling
another function* got the callee's value (`fn f { g Z; echo $1; }` → `Z`).
Reproduced identically at HEAD (arrays are name-slotted in codegen, not
SSA-versioned). Now correct in native+c+janet.

- `$1..$9` → `${args[0]}..${args[8]}` (0-indexed — DG, amended DD#75 §5 from
  1-indexed after checking prior art: matches sash's own arrays + Elvish/Oil/
  Python/Ruby/…; the 1-indexed-args langs, fish/zsh/Lua, are 1-indexed
  *everywhere*).
- `$#` → `${#args}`; expression + arithmetic share `positional_to_value`.
- user `${args[i]}`, `@args`, `${#args}`, `for x in @args` work (native+c;
  janet keeps its `_args` model). `resolve_args_name` maps `args`→`__argsN`.
- `>9` args reachable via `${args[N]}` (old scalar model capped at `$9`).
- **`shift` dropped** — a degenerate slice (DG). A general array **slice** is a
  post-beta research topic (`research-array-slice`, incl. value-vs-view +
  **copy-on-write**).

0 spec regressions native/c/janet (1037 native). Specs `types.sh` 75.17–75.22.
**Deferred polish** (`dd75-args-model-polish`): `$(len/join/keys args)` (intrinsic
recognition still gates on `collections.contains("args")`); `$*` removal;
`$@`→`@args` splat for >9; recursion guard (recursive inlining overflows the
compiler — pre-existing); friendlier `shift`-removed diagnostic.

Native backlog remaining (DG: "any/all remaining native implementation work"):
`bare-nested-cmdsub-arg`, `arg-negative-index-compile-reject`,
`len-undefined-reject`, `const-fold-command-name`, `arith-bare-subscript-
followups`, `arith-disposition-hex-octal-default`, + the args-model polish.

## ════════ STATUS (2026-08-20, later still) — janet STRUCTURAL MAP: Stage 2 SHIPPED ════════

**Stage 2 shipped `48bd47d` (pushed)** — the function surface + typed bindings.
Verified byte-identical to native on 7 programs (functions, positionals, `$@`
splice, return, function-in-condition, elif-in-function, negative int).

- `int n=5` → `(set n 5)` (janet **number**); untyped `x=5` → `(set x "5")`
  (**string**). Type shows through the *value representation*, not an
  annotation (DG). `int h=0xff` is front-end-rejected (SB4030) — prefixed ints
  arrive via `$((…))` (S4).
- `f(){…}` → `(sash/defn …)`; a user-function CALL renders distinct from an
  external: `(f args…)` / `(sash/succeeded (f …))` in a condition, vs
  `sash/run`/`sash/ok?`. `Env.fns` collected up front. Bare nonzero-return does
  NOT abort (matches native).
- positionals `$N`/`$#`/`$@` → `sash/arg`/`argc`/`args` (function-local; SB5024
  bars them at top level). `return N` → `(sash/return N)`.
- `(sash/TODO …)` is now a preamble macro: a partial program compiles and runs
  up to the first unmapped construct, then aborts naming it (`expr:arithmetic`
  etc.) — graceful degradation for DG's experimentation.

**Fix folded in first (`6da6522`):** removed the numeric `[[ ]]` forms
(`sash/-eq…-ge`) — a bashism; sash `[[ ]]` is string/file only, numeric compare
is `(( ))` (S4). Readable TODO labels (`stmt:*`/`expr:*`).

Known S2 gap: flat scope (local-shadowing unmodelled, same as existing
emit_janet) — proper per-scope var/set deferred to S3/S5.

Next: **S3 pipelines / `&` / `$!` / redirects** (+ consider real local scope).

## ════════ STATUS (2026-08-20, later) — janet STRUCTURAL MAP: Stage 1 SHIPPED ════════

DG re-aimed janet: "as near as possible to a 1-to-1 map of how the compiler
interprets the sash source … side-by-side … 'oh, I see that didn't parse the
way I expected'." Structural legibility (the parse shape) over semantic/
idiomatic legibility. Build in **stages**; get an early read after stage 1.

**Stage 1 shipped `2e6ad81` (pushed).** New `--janet-map` backend, additive
(old `--janet` untouched):
- `sash/src/emit_janet_map.rs` — structural emitter: Program → `sash/*` map.
- `sash/janet/sash-rt.janet` — primitive library, prepended as preamble.
- `--janet-map` flag in lib.rs; runs the real front-end (validation-gated).
- DD#15 amended; task `janet-structural-map` (S1 done, S2–S6 outlined); j.167.

Design (DD#15 amendment, DG-ratified): sash-hugging vocabulary; semantics in
sash-rt.janet; control flow via THIN macros (`sash/if` ≈ janet `if`) so emit
keeps supplying structure (IR-stage oracle independence preserved); `bless`
present+functional (`sash/bless-checked` aborts / `sash/bless` branches) but
NO taint flag; purely structural (no taint/type annotations); position-based
reads VISIBLE (`sash/run` bare-checked vs `sash/ok?` condition vs `sash/dash`
for `- cmd`); `($ x)` reads nil-coalesce to "".

Stage 1 surface: assign, echo/`:`, external (run vs ok?), if/else/elif,
while/until, `[[ ]]` string+numeric tests, `$()` single-external capture,
bless, `- cmd`, `$?`. **Awaiting DG's early read + experimentation before S2.**

Next stages (staged, per DG's read): S2 functions/return/positionals; S3
pipelines/`&`/`$!`/redirects; S4 `${…}` modifiers/arith/arrays/splats; S5
abort-channel/signals/case/multi-stmt-capture/regex; S6 oracle re-green +
retire old emit_janet, point `--janet` at the new one.

## ════════ STATUS (2026-08-20, cont.) — c PARITY SWEEP COMPLETE (janet deferred) ════════

DG: "tackle all the c parity work, and wait on janet" → then approved all
remaining (descriptive-abort incl. harness change, bgpid, ivc.8).

**RESULT: the c backend has ZERO failing tests and ZERO actionable skips.**
Started at 13 failing + 63 native-guarded; now 0 failing, 7 skipped — and all
7 are genuinely NOT c gaps: janet tests (15.1/15.2, `run_sash_janet`),
native-ELF-only (32.1/32.2, `.sash_source`/`--extract`), and mode-independent
CLI flags (57.46a-c). ~13 commits `16c5078`..`867e93a`, all 0 regressions.

Batches (journal 158-166):
- bless-fail abort, signum, DD#56 signals/END (+9)
- external `$(cmd)` capture + DD#55 abort (+3); `sash_capture_status` runtime
- bless-all + mapfile-from-file + RedirectPush/Pop + array append (+19)
- function/multi-statement `$()` capture (CaptureStart/End) (+4)
- un-guarded ~28 already-working / newly-working tests
- **cd** (was a no-op — real bug) + fixed `bug-emit-c-splat-vla-jump` (P2)
- **ivc.8** `local int array` (declare_vars "String wins" + table-slot override)
- **descriptive-abort** FULLY: cd + echo-write(2) + redirect-open(2) aborts +
  a harness change (`_run_sash_c` captures `$rerr`) → un-guarded 55.20-30 +
  24.20/25.8/gs.6/e.a70.regcomp
- **bgpid** `&`/`$!`/`wait` (SubshellStart/StoreBgPid/Wait, incl. no-arg wait)

New emit_c machinery: sash_capture_status/sash_chdir/sash_emsg/sash_alloc/
sash_trap/sash_signal_flag/sash_capture_start-end externs; `Emit.{
has_exiting_signals, cur_block, redirect_stack, in_condition}`; sub-dispatchers
`emit_read_inst/emit_bless_inst/emit_capture_inst/emit_builtin_extra/
emit_bg_builtin_c` (complexity gate).

**Remaining is janet parity (deferred by DG) + the c/janet parity tier tasks
that are janet-only.** The c half of every c-janet-* task is done.

## ════════ STATUS (2026-08-20, cont.) — c PARITY SWEEP (janet deferred) ════════

DG: "tackle all the c parity work, and wait on janet." Started at **13 failing
+ 63 native-guarded** c tests; now **2 failing + ~23 skipped**, most of the
skipped being not-c-gaps. 7 batches, all 0 regressions, all pushed on
`hand-parser`:

- **158** `16c5078` bless-fail abort, signum, DD#56 signals/END (+9:
  24.12, sx.7, 56.4-56.11). Signal runtime prims already in sash-rt.
- **159** `2d9230e` external `$(cmd)` capture + DD#55 abort (+3: 55.7/8/9).
  New `sash_capture_status` runtime helper.
- **160** `6fea877` bless-all + mapfile-from-file + array append (+19:
  60.9-60.36, ad.1-3). RedirectPush/Pop stack, BlessAll loop (shadowing,
  if-form, map), BlessRetag, ArrayAppend.
- **161** `95602ec` un-guard 12 already-working (e.b70.tbare/tdash, 63.2e/f,
  63.cap6/7, bng.1-6, ud.16, e.a70.silent).
- **162** `dd1215c` function/multi-statement stdout capture (+4: 17.3/4,
  mcs.3/4). CaptureStart/End via sash_capture_start/end.
- **163(6)** `eb4c63c` un-guard 14.2c/14.3a/b/d (cmdsub, work via 159/162).
- **163(7)** `6b9019c` implement `cd` (was a no-op — real bug); fix
  `bug-emit-c-splat-vla-jump` P2 (heap-alloc splat argv, not a stack VLA, so
  cd's abort goto can cross it). 0 un-guarded but 2 real fixes.

**Still failing (2):** 38.5, 55.19 — `$!` background pid (`&` →
`__subshell`/`__store_bg_pid`, TODO in c). → bgpid task.

**Remaining skipped — NOT c gaps (leave native-only):** 15.1/15.2 (janet
tests, `run_sash_janet`), 32.1/32.2 (native-ELF `.sash_source`/`--extract`),
57.46a-c (mode-independent CLI flags), 24.20/25.8/gs.6/e.a70.regcomp (assert
`$rerr`, which the c harness discards).

**Remaining c gaps needing work (3 areas):**
1. **descriptive-abort** (55.20-30, ~11 tests) — cd done; still need echo
   write(2)-error abort (emit_echo ignores sash_write results, 2 call sites) +
   redirect open(2)-error abort + a harness change so `_run_sash_c` captures
   runtime stderr into `$rerr` (currently `2>/dev/null`). `test_bin:` is a
   substring of c's `./test_bin:` so messages would match once captured. The
   harness change touches ALL c tests → needs a full-sweep re-verify.
2. **bgpid** (38.5, 55.19) — `&` background / subshell exec unimplemented in c
   (`__subshell`/`__store_bg_pid` TODO). A new exec model.
3. **ivc.8** — `local int array` function-local: the SSA restore mistypes the
   int-collection as an int scalar (`sash_atoi`) instead of the table slot;
   declare_vars "Int wins" picks the wrong slot. A transform/type fix.

## ════════ STATUS (2026-08-20, cont.) — P3 partials finished (tilde, dd67) ════════

DG: "finish the p3 partials." Both closed.

- **tilde-vs-dd53 → DONE**: the converter half is now implemented. `sash-convert`
  rewrites bash `~`/`~/path` → hoisted `home=$(getenv HOME); bless home [[ … ]]`
  + `$home`/`$home/path` (mirrors the `$HOME` env-var hoist). Unresolvable forms
  get a MANUAL note so the compiler's SB4036 forces review: `~user` (needs a
  passwd lookup), multi-tilde words `~/bin:~/lib` (no partial rewrite — would
  drop the 2nd `~`), command-position `~/bin/prog` (DD#23 rejects a dynamic
  command path). `~/$x` composes via the CST `concatenation(word "~/", $x)` →
  `$home/$x`. `convert_tilde_word` in `sash-convert/src/convert.rs`. Verified
  end-to-end: convert→compile→run is byte-identical to bash for `echo ~`,
  `echo ~/foo`, `x=~/p`. spec/convert.sh 57.47-57.55 (9 new, native-only; the
  compiler half is taint.sh 53.x). Rust convert_tests.rs tilde_* (4). NOTE:
  first drafted as 53.x → **collided** with taint.sh's 53.x (test_ids are global
  in spec.db, `native:53.1` shared); renumbered into convert.sh's DD#57 namespace
  (global max was 57.46).
- **dd67-bash-sugar-sweep → DONE (compiler scope)**: §1/§2/§4B + the keep/drop
  table were already done. §3 (sash-convert canonicalize mode) was blocked on
  dd68's brace forms; **dd68 is now done**, so §3 is unblocked but distinct
  Tier-6 converter work — **split out** to `sash-convert-canonicalize-mode`
  (status ready) rather than left lingering. dd67 itself is done.

Native sweep: 0 regressions, +9 (57.47-57.55). Change is isolated to
sash-convert + a spec file; the compiler is untouched.

## ════════ STATUS (2026-08-20) — finishing partials ════════

DG: "finish partials." Also fixed a bless-all bug DG surfaced.

- **bug-mapfile-undeclared-len-ice — front-end completion** (bc8940f): the
  transform auto-declare made `$(len a)` work but the FRONT-END didn't register
  an auto-declared mapfile/read array as a collection, so `bless e in a` /
  `${a[0]}` gave SB4030 ("scalar") — while `argv` and explicit `array a=()`
  worked. `type_check::check_command` now registers mapfile/readarray/read -a
  targets (match-dispatched, cog 15→2). Taint gate preserved. taint.sh ad.3/ad.4.
- **intrinsics-lengths-in-arithmetic → DONE**: all gates verified — `(len a)`
  sexpr evaluates, `$(( ${#a}+1 ))` works, a string intrinsic in arith
  (`(toupper s)`) is a clean SB4030. No code needed (already shipped).
- **int-valued-collections (DD#76) → DONE**: Stage C (bare `a[i]` arith
  rvalue+lvalue) shipped via DD#79 this session; A/B earlier. Backend gaps
  (janet-int-valued-collections, c-local-int-collection-decl) filed separately.
- **dd75-len-polymorphic → DONE (core)**: len type-dispatch + `${#x}` Int sugar
  done. The undefined-`len` hard reject is SAFE as-is (0, no garbage) and split
  to `len-undefined-reject` (P3) — confirmed `check_for` doesn't register the
  loop var, so a naive reject would false-positive on `len <loopvar>`.

Remaining partials: c-janet-dd55-abort-channel (P2, c/janet backend),
dd67-bash-sugar-sweep (P3), tilde-vs-dd53 (P3).



P1s: all done/superseded (the lone `fixed` one — arith `${var:OP}` silent-0 —
normalized to done; native/c gate resolved, only the janet arith-value residual
remains, tracked as janet-arith-subscript-read P3).

Working P2 bugs (bugs-first tier order):
- **bug-braced-rejected-var-bypass DONE**: `${RANDOM}`/`${HOME}`/`${HOME:-x}`/
  `${#HOME}` were silently compiling to empty (validate_expr only checked
  `$name`). Now `validate_expr` validates `${name…}` (any operator/index) via
  `validate_var_name` → SB5017/SB5018; normal `${x}`/`${x:-9}`/`${a[i]:-N}`
  stay clean. spec var_modifiers.sh bvb.1-5; +15, 0 regr. Journal 152.

Remaining P2 bugs (next): bug-mapfile-undeclared-len-ice (ICE),
bang-precedence-and-suppress; then c/janet parity + partials.

- **bug-mapfile-undeclared-len-ice DONE**: `mapfile -t a < f; echo $(len a)`
  with no prior `array a=()` panicked (ICE: undefined array). Fixed by a shared
  `autodeclare_bulk_target` (mirrors argv/DD#60) wired into argv/mapfile/read;
  auto-declared target behaves identically to a declared one (len works,
  element reads correctly tainted). native (c/janet mapfile gaps); taint.sh
  ad.1/ad.2; +2, 0 regr. Journal 155.
- **bang-precedence-and-suppress DONE** (DG reviewability call): `!` binds a
  single reviewer-atomic unit, so `! X && Y` = `(!X) && Y` (was wrongly SB4018);
  `!` on a pipeline → **new SB4040** (escape hatch: wrap in a predicate fn);
  `- ! X` → SB4019 at parse time (all modes). Design principle recorded in DD#55
  (reframed "ambiguous" → **sash must be easier to review than bash**) + DD#70
  addendum. spec errors.sh bng.1-6; 0 regr. Journal 156. (c/janet `!` runtime
  parity still separate.)



DG: "let's tackle the decisions." All three implemented + pushed:

- **A — multi-statement/pipeline `$(…)`: IMPLEMENTED** (this commit). `$(x=1;
  echo $x)` / `$(a | b)` were a silent `$(...)` placeholder; now capture
  correctly. New `SsaValue::CmdSubstBody` lifted via `lift_int_func_calls` →
  `capture_body_into` (reuses `CaptureStart/End` — no new codegen); subshell
  isolation via save/restore of assigned vars. **Closed a taint hole**: the
  taint-source scans inspected only `stmts.first()`, so pipeline/multi-stmt
  external output (and `$(- external)`) flowed as clean, bypassing bless —
  now recursive over all stmts/stages + a matching `CaptureEnd{tainted}` flag.
  native (c/janet capture is a pre-existing gap); taint rejects all modes.
  spec commands.sh 14.3*, taint.sh mcs.*, errors.sh 55.9. +17, 0 regr. Jrnl 154.
- **B — compound-condition catch** (fff66b4): see below.
- **C — $LINENO reject** (fe986b6): see below.



DG: "re: string-array cell; I would expect the same behavior as from a string
variable." Then "implement it properly." A cell `${a[i]:OP}` now behaves exactly
like a scalar `${s:OP}`, word + arith.

- **`${a[i]:-N}` / `${a[i]:?msg}` work** (array/map, literal or `$i` index) —
  DD#77 disposition on the cell's value. Closed a **silent-miscompile bug**:
  `try_subscript` dropped the operator after `]`, so `${a[i]:-N}` silently atoi'd
  and `${a[i]:?msg}` never aborted.
- **Bare string cell `$(( a[i] ))` → SB4030** (D1 extends to cells); bare string
  index `a[s]` too. Int cells (DD#76) unaffected.
- Operator parity: `#`/`/`/`@X` → SB4034/SB4015; substring/sed on a cell →
  explicit **SB4039** (staged, follow-up — never a silent drop). DD#78 default
  discipline (taint SB5020 / non-random SB5035) rides for free.
- Representation: `ParameterExpansion.index` + SSA disposition nodes generalized
  `var: VarRef` → `source: Box<SsaValue>` (Var | Subscript). janet word-context
  works; janet **arith** cell disposition = pre-existing `janet-arith-subscript-
  read` gap.

native+c full parity. spec arithmetic.sh scx.1-16 (plan 133→143). 3-mode sweep
**0 regressions, +42 passing**. fmt+clippy clean; complexity gate clear.
Follow-ups: `arith-bare-subscript-followups` gate 4 (substring/sed cell parity);
`janet-arith-subscript-read` (widen scx.12-14). DD#77 "Implemented — string-
array/map cells"; journal 151. **NOT committed at write time; commit + push next.**

## ════════ STATUS (2026-08-18, cont.) — arith subscripts model B Stage 2 + B2 (DD#79) ════════

DG: `(( a[i]++ ))` — shouldn't this work? It's the ratified-but-unbuilt **Stage
2** of arith-subscript-values (model B). Implemented + a design seam DG
resolved. **DD#79** records both parts.

- **Stage 2**: bare `a[i]` in arith now reads + mutates (`$(( a[i]+1 ))`,
  `(( a[i]++ ))`, `(( a[i] op= v ))`, `(( a[i] = v ))`). `BareSubscript` lexer
  token → `ArithExpr::Subscript`/`SubscriptAssign`; lvalue reuses the statement
  subscript-assign write path; rvalue → `SsaValue::Subscript` (arith int key).
- **Bare `m[k]` → SB4030** (was a silent-wrong bug: int-key miss on a
  string-keyed map).
- **B2** (DG): SB5031 extended into arithmetic — a bare name *read* assigned
  nowhere is a typo (`$(( y+1 ))`, an undeclared index). Anything you assign
  (`(( f++ ))`, `for (( i=0;… ))`) is in scope. Function names exempt (DD#58).

native+c; janet arith-value subscript is the pre-existing filed gap
(`janet-arith-subscript-read`); `janet-undeclared-arith-counter` filed. spec
arithmetic.sh bsx.1-7, b2a.1-3; plan 117→127. 3-mode sweep 0 regressions, +24.
Follow-ups filed: `arith-bare-subscript-followups` (`$a[i]` form, nested index,
string-array-cell DD#77 question). DG's line works with `int i=0`.

## ════════ STATUS (2026-08-18) — DD#24 taint bug fixed: scalar assignment no longer propagates ════════

DG, reviewing the DD#78 summary, caught that I'd called `z=$y` a legal "source
assignment" — it's a **read** of `$y`. Investigation confirmed a pre-existing
bug: `taint_assignment` uniquely *propagated* taint (`z=$y` compiled, taint
caught only at a later sink) instead of rejecting the read, contradicting DD#24
§Quarantine ("No propagation", `b=$a` rejected) + journal 036 (`x=$1 # compile
error`). Every other read site rejected; scalar assignment was the lone
offender. It was the root cause of the DD#78 sink-hole class.

**Fixed (this commit):** scalar assignment now rejects a tainted read in the RHS
(SB5020) — `z=$y`, `z="$y"`, `z=${y}`, `z=${x:-$y}`, `z+=$y` — while still
allowing assignment *from a source* (`z=$(getenv X)`). Behavior change:
bless-at-source, not on a copy. Also closed a residual DD#78 hole
(`z=${x:-$(getenv)}`). spec/taint.sh np.1-7; plan corrected 75→85; 3-mode sweep
0 passing→failing, 0 failing→passing (nothing relied on propagation). Journal
149; DD#24 enforcement note.

## ════════ STATUS (2026-08-17, cont.²) — DD#78 disposition-default discipline + DD#60 arg-default ════════

DG proposed `$(arg N)` take an optional default instead of aborting on OOB.
Design ratified **DD#78** (disposition defaults must be clean + non-`random`)
and a **DD#60 addendum** (`$(arg N DEFAULT)`). **Both complete + pushed.**

- **DD#78 Stage 1 (8e522e9):** closed a real taint hole — a `:-` default's
  operand was never taint-checked (`echo ${x:-$(getenv)}` / `${x:-$y}` compiled;
  `z=${x:-$(getenv)}` left z clean). Both taint primitives now recurse into the
  `:-` operand; `check_disposition_defaults` rejects `$(random)` defaults
  (SB5035). Clean-when-read (use rejects; store propagates). taint.sh dd78.1-7.
- **DD#60 arg-default (this commit):** `$(arg N DEFAULT)` returns the default on
  OOB (bare `arg N` still aborts). Runtime `sash_argv_get_or`, 3-backend
  lowering, arity SB5036, DD#78 clean/non-random default (SB5020/SB5035),
  tainted result. argv.sh av.d1-d7, plan 37→44. 3-mode sweep 0-regression +20.

Deferred (filed): `arg-negative-index-compile-reject` (DD#60 negative-index not
enforced), `bare-nested-cmdsub-arg` (general parse limit — quote the nested sub),
`janet-getpid-class-intrinsics` (av.d4 janet guard). Pre-existing `mk test` rot:
`audit-rust-test-suites`.

**NEXT:** back to the DG tier backlog. DD#60/DD#78 need nothing further.

## ════════ STATUS (2026-08-17, cont.) — DD#77 COMPLETE (Stage C1/C2/C3 shipped) ════════

DD#77 (arith string→int conversion discipline) is **complete**. Stage C shipped
this session on top of the mechanism + `-i` work below:

- **3028b65** C1 — un-prune `:?` everywhere (word ctx aborts on unset/empty;
  arith on non-int); `RequireValue` SSA node + `sash_require_value` runtime +
  3-backend codegen.
- **028c325** C2 — arith strict-disposition semantics for `:-`/`:?` via
  `sash_atoi_or`/`sash_atoi_or_abort` (sash_is_int gate); native+c.
- **(this commit)** C3 — **D1** bare string operand in arith → SB4030 (directs to
  `${s:-N}`/`${s:?msg}`); **D4** the `:-` default must be a decimal integer
  literal (was a silent atoi→0 footgun for `${s:-hello}`/`${s:-0xff}`); migrated
  the DD#72 bare-coercion tests (`nl.s*`, `30.2`, `pex.7`) to dispositions;
  amended DD#72. type_check-only (front-end, all modes reject).

Out-of-range *input* is now a disposition concern (`${s:?}` abort / `${s:-N}`
default via `parse_strict_i64`'s range check), NOT a DD#25 arithmetic overflow
(that's for operations like `INT_MAX+1`, still covered by 25.2–25.12).

Guarded gaps (filed, not blocking): janet arith-value `${…}` disposition
(`janet-arith-subscript-read`); full DD#72 grammar (`0x`/`0o`/`0b`) as a `:-`
default (`arith-disposition-hex-octal-default` — D4 currently decimal-only, which
converts the silent-0 into a clean reject). Full 3-mode sweep: 0 passing→failing,
0 gone, 11 new passing; native 0-fail; clippy clean.

**NEXT:** back to the DG tier order (notes/status.md lower blocks) — the T4
c/janet parity / remaining backlog. DD#77 needs nothing further.

## ════════ STATUS (2026-08-17) — DD#77 arith string→int discipline (mechanism + `-i` done; Stage C next) ════════

Design conversation ratified DD#77 (string→int conversion discipline in arith)
+ DD#30 point-1 refinement. Shipped this session (all pushed hand-parser, 0 real
regressions, native 0-fail):

- **61a251e** point-1 — bare numeric literal to a known-int LHS (`int x=0; x=5`,
  `a[1]=99`); DD#30 amended. Fixed a pre-existing janet scalar-`+=` silent
  miscompile inline.
- **87cbd2b** DD#77 A — route arith `${…}` through the real param parser; fixes
  the P1 silent-0 bug (`${s:-0}` → bogus var → 0).
- **489d994** DD#77 B — `sash_is_int` + `[[ -i EXPR ]]` test, 3 backends.
- **a020ade** fix — Stage A opened a pruned-operator ICE hole (`:?`/`:=`/`##`… in
  arith bypassed SB4034 → ICE); walk now descends arith, rejects cleanly.
- **91dd253** DG follow-up — `-i` range-checked (valid AND representable);
  runtime atoi overflow now a clean `sash_arith_overflow` trap (was rc-99 panic).

DD#77 recorded (+ resolved sub-decisions: `:?` un-prune everywhere mirroring
`:-`; D1 as approved, DD#72 bare-coercion superseded; `-i` range-checked).

**NEXT — DD#77 Stage C (unblocked, large):**
- C1: un-prune `:?` (word ctx aborts on unset/empty; arith on non-int) — abort
  node + 3-backend codegen.
- C2: arith strict-disposition semantics for `:-`/`:?` (sash_is_int gate).
- C3: D1 bare-string-in-arith → error + D4 numeric-literal default + migrate
  DD#72 `arith_literals.sh` bare-coercion to dispositions + amend DD#72.
See notes/tasks/dd77-arith-string-conversion.md.

## ════════ STATUS (2026-08-16, cont.) — DD#76 int-valued collections A+B DONE ════════

DG: fold int-valued collections into the arith-subscript effort. **DD#76**
(int-valued collections, `val_len` double-duty) recorded; Stages A+B shipped
native + c.

- **int array a=(10 20 30) / int map m=([k]=5)** now declare (top-level parser
  stacks `int`+structure; `local int array` already stacked via parse_local).
- **Storage (encoding b, DG-chosen):** int cell = `{val_ptr:null, val_len:i64}`.
  Runtime unchanged. Two SSA additions: `SsaValue::IntCell` (write → `(null,i64)`,
  wraps literal elems + assign RHS) and `is_integer_value` on `Subscript` (read:
  string ctx → itoa, int ctx → raw val_len).
- **Slot fix:** a collection var holds a table pointer even for int elements
  (value-shape keys slot allocation) — native + emit_c.
- **Consistency:** bare `a[1]=99` → SB4030 (uniform with `x=5`); use
  `a[1]=$(( 99 ))` (no DD#30 quotes for an int cell). `int x=${a[0]}` works.

specs arithmetic.sh ivc.1-11 (plan 90→101). Full 3-mode sweep: 0 passing→failing,
0 gone, 23 new passing. native 0-fail. clippy clean. Rust == benched baseline.

**Remaining:** Stage C (bare `a[i]` in `(( ))` — arith lvalue/rvalue, the
arith-subscript-values stage 2). Backend gaps filed: janet-int-valued-collections
(janet is_int conflation), c-local-int-collection-decl (function-local table var
undeclared; ivc.8 native-only). Open DG question: const+collection stacking
(DD#35 orthogonality).

## ════════ STATUS (2026-08-16) — arith-subscript reads (BUG 1) fixed; janet oracle rethink ════════

DG reported 2 suspected bugs in array-values-in-`(( ))`:
- **BUG 1 (real, FIXED stage 1):** `${a[i]}`/`${m[k]}` evaluated to 0 in arith
  (parser read `${a[0]}` as a var named "a[0]"). New `ArithExpr::Expansion(Expr)`
  imports the normal `Subscript` Expr via shared `expr_to_value`. native+c
  (janet value-path gap filed: janet-arith-subscript-read). specs asx.1-6.
- **BUG 2 (not a bug):** OOB/missing key → zero-value (sparse table-backed,
  DD#53); DG: keep it.
- Design (arith-subscript-values): `${…}` is an rvalue value-import ONLY (never
  an lvalue — preserves `${…}`="the value of"); **stage 2 remaining**: bare
  `a[i]` int-array arith lvalue+rvalue (`(( a[i]++ ))`), bare `m[k]` → error.

Also this session: **janet oracle rethink** (janet-rt-primitive-library) — build
a `sash-rt.janet` primitive library + thin leaf lowering, keep IR control flow.

native 904/0-fail; c 13 / janet 36 == baseline; 0 regressions.

## ════════ STATUS (2026-08-15) — DD#58/Option C sexpr int intrinsics DONE ════════

bug-arith-sexpr-intrinsics-zero complete (both parts, pushed on hand-parser):
- **Part 1 (9f85526)** — category A int intrinsics compute in a sexpr (were
  silent 0); `$(( (x) ))` grouping bug (→0) fixed; Q3 `random` int-typed.
- **Part 2** — category B + Q2: a string producer in a sexpr → SB4030 type
  error (type_check third pass). Closes the getenv/arg validator-bypass +
  user-string-fn side-effect run. dtoi/itod rejected in a sexpr.

Governed by Option C (`$()` return-type-polymorphic; int producers work in both
`$(f)` and `$(( (f) ))`). specs arithmetic.sh sx.1–17 (plan 62→84). native
898/0-fail; c 13 / janet 36 == baseline; 0 regressions; Rust suites == HEAD.
Deferred/filed: getcpid-in-sexpr, c/janet signum intrinsic.

## ════════ STATUS (2026-08-15) — DD#58/Option C sexpr int intrinsics (part 1) ════════

Implementing bug-arith-sexpr-intrinsics-zero (Option C: `$()` return-type-
polymorphic; int producers work in both `$(f)` and `$(( (f) ))`). Part 1 done:
- **Category A** — int intrinsics compute in a sexpr (were silent 0):
  `(getpid)`→`$$`, `(status)`→`$?`, `(getppid)/(argc)/(random)/(signum)/
  (epochseconds)`→ CmdSubst; native direct-i64 via shared `int_intrinsic_i64`,
  c via atoi fallback. native+c (signum native-only — c/janet TODO stub, filed
  c-janet-signum-intrinsic).
- **Grouping bug** — `$(( (x) ))` on a plain variable returned 0; now returns
  the variable (arith parser defers the `(name)` call/group decision here).
- **Q3** — `random` int-typed (`int n=$(random)` was wrongly SB4030).

spec arithmetic.sh sx.1–11 (plan 67→78). native 892/0-fail; c 13 / janet 36 ==
baseline; 0 regressions; codegen 179/77 + e2e 89/41 == HEAD.

**Part 2 (next):** category B (string producer in a sexpr → type error; closes
the getenv/arg validator-bypass) + Q2 (dtoi/itod rejected). Needs a type_check
arith-walk. getcpid-in-sexpr deferred.

## ════════ STATUS (2026-08-14 unattended, cont.) — DD#75 §1 special-var sugar DONE ════════

DD#75 §1 (special vars are sugar; keyword forms canonical) — compiler core done.
Both spellings already lowered to one node; pinned the equivalence + reframed
the surface: `$$`≡`$(getpid)`, `$?`≡`$(status)`, `$!`≡`$(getcpid)` (specs
misc.sh spv.1–4; spv.1–3 `native,c` — janet execs getpid + mis-tracks arith
status, filed). SB5012 leads with `$(status)`. DD#75 §1 + DD#24 docs updated.
Convert + public GUIDE/SUBSET prose → converter-special-var-sugar (DG voice).
native 881 / 0 fail; c 13 / janet 36 == baseline; 0 regressions.

**Only DD#75 §5 (args-model) remains** — the invasive one (function args →
implicit 1-indexed `args` array; `$1`→`${args[1]}`, `$@`→`@args`, `$#`→`${#args}`,
`$*` removed). Now **scoped** in its task file (current model = scalar SSA vars
"1".."9"/"#", not an array; files: parse/transform/validate/type_check/codegen).
Has an implementation-approach fork needing DG's nod before coding: (a) a real
`args` table (true DD#75 uniformity, larger/riskier) vs (b) surface sugar over
the existing scalar vars (smaller, only partial). Parked before a rushed
invasive edit (same call as the retire-bracket pause).

## ════════ STATUS (2026-08-14 unattended, cont.) — bug fix: special-var arith silent-0 ════════

Stop-everything bug found while assessing dd75-special-var-sugar (§1): `$$`/`$?`
evaluate to **0** as an operand in a compound `$(( ))` expression (standalone +
word contexts were fine; `$#` unaffected — real slot). Pre-existing; violates
DD#24/DD#31 (both list these as clean arith operands). Root cause: the int
codegen path (`lower_value_as_int`) lacked the computed-special handling the
string path has (`$`→getpid, `?`→exit-status). Fix: `special_var_as_int` helper
+ threaded `exit_status_var` through `lower_value_as_int` (native). c already
correct; janet errors (filed). Test-first arithmetic.sh 31.20–24 (`native,c`;
plan 62→67). native 0 failing; c 13 / janet 36 == baseline; codegen 179/77 +
e2e 89/41 == HEAD. Filed bug-arith-keyword-pid-intrinsic-zero (sexpr
`(getpid)`→0) + bug-janet-special-var-arith.

## ════════ STATUS (2026-08-14 unattended, cont.) — dd75-retire-bracket DONE ════════

Retired the `[@]`/`[*]` bracket-subscript collection forms (DD#75 §4). `@a` is
the sole splat; `${#a}`/`$(len a)` length; `$(keys m)` keys (maps only).
**Parse-level reject (SB4038)** — `${a[@]}` and `@a` share the `arrvals` repr,
so they can only be distinguished at parse. Tagged 5 bracket sites in
`parse/expr.rs` (`*_retired` ops); `parser.rs` `retired_bracket_operator_diag`
emits SB4038 with the canonical replacement per form. Canonical `@a` untouched.

specs types.sh 75.10–16 (plan 86→93) + ws.q4/q5 flipped to SB4038; Rust
migrated to canonical forms (brush/c/codegen/e2e, all pass). native 0 failing;
c 13 / janet 36 == baseline (0 regressions). Converter half filed as
`converter-retire-bracket-splat`.

**Remaining DD#75 (2 invasive P3, not started):**
- dd75-args-model — function args → implicit `args` array; `$1`→`${args[1]}`,
  `$@`→`@args`, `$#`→`${#args}`, `$*` removed. Most invasive.
- dd75-special-var-sugar — `$?`/`$!`/`$$` → sugar for `$(status)`/`$(getcpid)`/
  `$(getpid)`; mostly converter/docs/taint framing (compiler already accepts).

## ════════ STATUS (2026-08-14 unattended, cont.) — DD#75: 5 tasks landed ════════

Continued the unattended DD#75 run (test-first, pushed on hand-parser):
- ab184c0 bug-subscript-assign-bare-key (P2)
- dc9f8df dd75-keys-map-only (P3) — + added the missing For-arm to type_check
- a80d94d + 2dad1ab intrinsics-lengths-in-arithmetic gates 1+2 (P2, DEMO):
  `$(( (len a)+1 ))` and `$(( ${#a}+1 ))` evaluate (sexpr + ATok::LenOf)
- 84c28ce string-func-capture-in-arg-ice (P2) — DG: lift-and-print; shared
  capture_func_into; `echo $(greet)` works incl. runtime capture (no ICE)

All native 0 failing; c 13 / janet 36 == baseline. Backend gaps filed:
janet-join-intrinsic-gaps (join/keys/len-in-arith), cjanet-function-stdout-capture.

DG unblocked the invasive DD#75 tasks (unreleased → no migration risk; convert
Qs as follow-ups). Remaining, scoped in their task files:
- dd75-retire-bracket-subscripts — PARSE-level reject (${a[@]} shares the arrvals
  repr with @a) + flips passing wordsplit/codegen/brush tests.
- dd75-args-model — implicit `args` array (most invasive).
- dd75-special-var-sugar — mostly converter/docs.

## ════════ (prior status below) ════════
## ════════ STATUS (2026-08-14 unattended) — DD#75 continuation: 3 more tasks ════════

Unattended session (SOP: ready backlog, dependency order, test-first, skip
decision-gated). Landed on hand-parser:
- ab184c0 bug-subscript-assign-bare-key (P2) — indexed `a[i]=` bare-key reject.
- dc9f8df dd75-keys-map-only (P3) — keys map-only; added the missing For-arm
  to type_check (for-in headers now type-checked).
- a80d94d + 2dad1ab intrinsics-lengths-in-arithmetic (gates 1+2) — DG demo:
  `$(( (len a)+1 ))` and `$(( ${#a}+1 ))` now evaluate (arith sexpr + braced
  `${#x}` via ATok::LenOf → the len FuncCall shape). Filed sub-task
  arith-braced-length (done). string-intrinsic-in-arith reject deferred.

All: native 0 failing; c 13 / janet 36 == baseline. janet len/keys/join-in-arith
gaps filed (janet-join-intrinsic-gaps).

**Remaining DD#75 (3 invasive P3 language changes — not started):**
dd75-special-var-sugar ($?/$!/$$ → sugar; converter+docs+taint framing),
dd75-args-model (implicit `args` array; invasive), dd75-retire-bracket-subscripts
(retire [@]/[*]; migration). Better as focused efforts. string-func-capture-in-arg-ice
is decision-gated (works vs reject) — for DG.

## ════════ (prior status below) ════════
## ════════ STATUS (2026-08-11, late) — DD#75 implementation: 3 parts done ════════

Implementing DD#75 in DG's recommended order, **test-first** with spec tests.
Pushed on `hand-parser`.

**DONE (committed + pushed):**
1. `keys-splat-context-bypass` **P1** (`6742554`) — `$(keys m)` bypassed the
   SB5025 splat guard → silent join + garbage bytes (memory-unsafe). Added
   `is_keys_splat_expr`; SB5025 outside for-in. Also fixed a stray `dp.co6`.
2. `bug-join-explicit-separator-segfault` **P1** (`2c78be7`) — DG chose
   `join arr SEP` (array-first); swapped arg order; `check_join_operand` rejects
   a non-array operand (SB4030) — no more segfault. janet join gaps guarded +
   filed (`janet-join-intrinsic-gaps`).
3. `dd75-len-polymorphic` **P2 core** (`039ed7b`) — `len` dispatches on TYPE
   (collection→count, string→bytes); `${#x}` is Int sugar (`int n=${#a}`→3,
   `int n=${#s}`→5). Demo works. Partial: arith + undefined-reject deferred.

All three: native 0 failing; c 13 / janet 36 == clean baseline.

**REMAINING DD#75 (larger, P3 language changes — not started):**
- `dd75-keys-map-only` (P3) — reject `keys` on indexed arrays.
- `dd75-special-var-sugar` (P3) — `$?`/`$!`/`$$` → sugar; touches DD#24, converter, docs.
- `dd75-args-model` (P3) — function args → implicit `args` array; invasive.
- `dd75-retire-bracket-subscripts` (P3) — retire `[@]`/`[*]`; migration.
- Follow-ons: `intrinsics-lengths-in-arithmetic` (arith len), `bug-subscript-assign-bare-key`.

## ════════ (prior status below) ════════
## ════════ STATUS (2026-08-11) — DD#75 drafted; collections/positionals redesign ════════

**DG design session → DD#75 "Named/keyword forms canonical; `$`-sigils sugar;
collections uniform"** (`notes/decisions/DD75-…md`, status accepted). Captures a
project-wide **pivot**: bash-compat → unambiguous/self-consistent/symmetrical/
transparent. Codifies five rules:
1. `$?`/`$!`/`$$` → sugar for `$(status)`/`$(getcpid)`/`$(getpid)` (inverts
   journal-050; keyword canonical). DG confirmed incl. `$?`.
2. `len` type-polymorphic (array/map→count, string→bytes); `${#x}` Int sugar.
3. `keys` map-only (indexed → reject).
4. `@a` sole splat; retire `${a[@]}`/`${a[*]}`/`${#a[@]}`/`${!a[@]}`.
5. Function args = implicit 1-indexed `args` array; `$1`→`${args[1]}`,
   `$@`→`@args`, `$#`→`${#args}`, `$*` removed→`$(join args)`.

**Bugs surfaced while designing (filed):**
- `keys-splat-context-bypass` **P1** — `$(keys m)` in cmd-arg position bypasses
  the SB5025 guard → silent join + **garbage bytes** (memory-unsafe). Root:
  `check_argv_splat` hard-coded to `$(argv)`.
- `bug-join-explicit-separator-segfault` **P1** — `$(join a SEP)` segfaults.
- `bug-subscript-assign-bare-key` P2 — `a[i]=` bare key accepted, read rejected.

**Task breakdown:** `dd75-special-var-sugar`, `dd75-len-polymorphic` (P2, demo),
`dd75-keys-map-only`, `dd75-args-model`, `dd75-retire-bracket-subscripts`.
Superseded: `length-operator-int-typing`, `len-nonarray-operand-reject`.

**Implementation NOT started** — DD lands first. Next: fix the two P1
memory-safety bugs (independent of the language change), then §2 (len) for the demo.

## ════════ (prior status below) ════════
## ════════ STATUS (2026-08-10, late) — dispatch operator parity COMPLETE ════════

**Pushed `ee3a2fa` on `hand-parser`.** Achieved dispatch-boolean parity for compact
operators (==, =~). Issue: `[[ $x==a { … } ]]` required spaces; `[[ $x==a ]]`
worked fine. Root cause: parser's `parse_test_binary_tail` returned a Unary
(implicit `-n`) when no token-level binary operator followed. Fix: added
`extract_compact_operator` group (4 helpers) to detect and split compact
operators in Literal and Concat expressions, recreating proper Expr structures
and preserving variable references. Dispatch form now works identically to
boolean test: compact, spaced, with variables, with multiple patterns, with `=~`.

Spec: added tests I1–I8 (dp.co1–dp.co8) covering compact parity. Verification:
spec tests PASSED (1696 native passing, net new ~20 from dispatch tests).
Complexity: initially flagged high cog, refactored to flatten nesting
(extract_from_literal, extract_from_concat, match_operator, extract_leading_operator).

Closed task: dispatch-compact-operator-spacing (P3).

## ════════ (prior status below) ════════
---
title: Status
modified: 2026-06-07
---
title: Status
modified: 2026-06-07
---

<!-- BEGIN: mk-status (regenerated by contrib/status-update.sh) -->
## Current branch: hand-parser

## Spec status: 1178 passing / 189 failing of 1411 (spec.db @ fd03369)

_Regenerated 2026-06-19 by `mk status`. Edits inside this block will be
overwritten; edit outside the BEGIN/END markers._
<!-- END: mk-status -->

### Session 2026-08-10 — `[[ ]]` whitespace-optional, like `(( ))`

DG (demo) hit an asymmetry: `return((1==1))` parsed but `return[[a=a]]`
didn't. Root cause: `[`/`]` are word chars (subscripts), so `[[`/`]]` only
tokenized when whitespace-delimited, while `(`/`)` are word-breaks. Fix
(`scan_word_chars`): break a word on a **doubled** bracket (`[[`/`]]`), so
`[[ … ]]` is whitespace-optional everywhere (`if[[…]]`, `while[[…]]`,
`![[…]]`, compact `[[a==a]]`, DD#74 `return[[…]]`); a single `[`/`]` still
absorbs as a subscript char. Safe because sash subscripts need a `$` sigil
(`a[${b[0]}]` ends in `}]`, never `]]`); a repo-wide search found 0 glued
brackets in sash source. Also guard-converted the arm's nested `if`s,
dropping `scan_word_chars` cognitive 14→3 (grandfathered gate violator).
Specs `spec/dispatch.sh` H1–H9; **0 regressions, 27 new passing** (9×3
native/c/janet); Rust failure set identical to HEAD. DD#59 addendum. Dispatch
`==` spacing inconsistency (SB4028) filed separately
(`dispatch-compact-operator-spacing`).

### Session 2026-08-09 — DD#74 predicate functions (phases 1-3)

DG established DD#74: a function used in a *boolean context* (an `if`/`elif`/
`while`/`until` condition, `! f`, or a `&&`/`||` operand) is a **predicate**;
its status comes only from an explicit `return` — no implicit trailing-command
status. Shipped native/c/janet, 0 native regressions:

- **Phase 1** (`81c50dc`) — `return N` drives the enclosing condition (it now
  syncs `test_result_var`, not just `$?`). Fixed c's missing `!` sigil and
  janet's `if <userfunc>`.
- **Phase 2** (`0adfc36`) — `return <producer>`: `return CMD` / `return [[ ]]`
  / `return (( ))`. New `StmtKind::ReturnProducer`; the command form is
  consumed (never aborts).
- **Phase 3** — definite-return in a boolean context (**SB4037**):
  `check_predicate_definite_return` in validate.rs rejects a predicate that can
  fall off its end. A bare-only void call (a procedure) is unaffected. specs
  74.13–74.18. Refactored `validate()` (cog 8→4) to clear the complexity gate
  after touching it.

Prior focus below.

### Session 2026-07-06/07 — backlog prioritization (DG rule)

DG set the working order for all remaining tasks: **(1) finish partially
completed work → (2) fix compiler bugs → (3) implement remaining compiler
features → (4) c/janet backend parity → (5) convert tasks.** "Convert is
secondary to anything compiler-related." Dependencies may pull an item
earlier, but nearly all `depends:` are satisfied (DD docs or done tasks;
`advance-dd23-resolution` is now done, unblocking
`tighten-cmdsub-taint-execve`), so the tiers apply cleanly. **⚠ = blocked
on a DG decision before it can complete.**

**Tier 1 — finish partials (native)**
1. ~~`abort-diagnostics-remaining`~~ — DONE 2026-07-07 (native abort
   diagnostics for external/cmdsub/pipeline/regerror9; bg-exec uses the
   status form, errno-precision filed as `bg-exec-errno-message`).
2. ~~`implement-getpid-builtins`~~ — DONE 2026-07-07 (getpid + random landed
   native+c; `$(random)` = non-negative i64 via getrandom(2). janet class
   gap filed as `janet-getpid-class-intrinsics`, Tier 4). **Tier 1 COMPLETE.**

_Correction 2026-07-07 (autopilot): `dd55-abort-channel-remaining` moved to
Tier 4 — it is entirely c/janet backend work (native landed in `707832f`),
so it follows the native tiers per the native-first rule. `dd67-bash-sugar-
sweep` compiler part is DONE (§1/§2/§4B shipped); its only remainder is §3,
a sash-convert canonicalize mode → moved to Tier 6._

**Tier 2 — compiler bugs (native)**
5. ~~`bug-echo-builtin-array-splat-ice`~~ — DONE 2026-07-07 (SB5030 rejects
   `echo @a`; ICE backstop unreachable)
6. `tighten-cmdsub-taint-execve` — DONE 2026-07-07 (printf/pwd made
   external — removed the Printf/Pwd builtins; execve capture tainted)
7. `bug-test-operand-concat-splice` — DONE 2026-07-07 (NOT A BUG: repro was
   arithmetically wrong, `run`+`ing`=`runing`; bash agrees; guarded ow.11-13)
8. `tilde-vs-dd53` — compiler half DONE 2026-07-07 (SB4036 focused reject);
   converter rewrite half → Tier 6.

**Tier 2 complete** (native compiler bugs); Tier 3 next.

**Tier 3 — remaining compiler features (native)**
9. `reject-undeclared-scalar-read` (P2) — DONE 2026-07-08 (DD#62 Model B:
   meaningful zero value + a scalar typo check. SB5031 rejects a scalar
   read of a name introduced nowhere in scope; `${x:-…}` is the opt-out.
   Single-pass scope-membership, not definite-assignment. Recurses into
   command-substitution bodies with subshell scope. `spec/undeclared.sh`
   ud.1-16 all 3 modes; 0 regressions. Filed janet-scalar-zero-value and
   bug-multistatement-cmdsub-placeholder surfaced by the specs.)
10. `regex-interpolation-delimiters` (P2) — DONE 2026-07-08 (DD#63 D/E/G:
    both `${var:s/…}` and `var =~ s/…` re-routed at parse to interpolating
    `$(sub|gsub "$var" RE SUB)` — `$x` in a field now expands (D7/D8).
    `\xnn` hex escape in the vendored engine (E1/E2). Lexer Path B
    (`scan_braced_content`) scans the `:s/` fields by slash count so a `{`/`}`
    byte doesn't truncate `${…}` (G1/G2/G3), malformed payload → SB4032
    (G5/G8). spec/regex.sh 63.d7/d8/e1/e2/g1-g8; 0 regressions. Filed:
    decide-regex-bare-brace (bare `{` literal vs `\{`), and
    retire-legacy-sed-regex-replace-machinery (the old `:s`/regex_replace
    transform+codegen is now unreachable, remove it).)
11. `dd70-error-model-contract-spec` (P2) — DONE 2026-07-08 (filled the
    gaps: bare `[[ … ]]` is now a must-consume (B) boolean —
    check_boolean_must_consume generalized from match to also cover
    BuiltinCmd::Test → SB4018 bare / SB4019 `- [[ ]]`; consumed (if/&&/||/`|| :`)
    and dispatch `[[ … { } ]]` forms unaffected. Contract tests added to
    spec/errors.sh: e.b70.tbare/tdash/tdisc/tand/tor/fndisc, e.a70.silent/
    regcomp. rerr-asserting + `-> int`-discard cases are native-only (the
    c/janet harnesses discard runtime stderr; c/janet also can't yet compile
    a `-> int` fn in `|| :` — filed c-janet-fn-int-discard). 0 regressions.)
12. `glob-as-array-splat-disposition` (P2) — DONE 2026-07-08 (Part 1 was
    already done; Part 2 == #13, now landed → whole task done).
13. `glob-splat-disposition-word` (P2) — DONE 2026-07-08 (DD#69 Part 2:
    external-splat empty-disposition. Bare `@a` into an external command
    (`/`-prefixed post-resolve head) → SB5032; `@{a:-WORD}` (empty → single
    arg WORD; `@{a:-}` → 0 args) / `@{a:?WORD}` (empty → abort exit 1
    `<progname>: WORD`); only `:-`/`:?` (else SB5033). New `@{…}` lexer token
    (scan_at_brace) → parser (arrvals_default/arrvals_abort) → SsaValue::
    ArraySplat.disposition → runtime sash_splat_disposition, applied by all
    three argv builders. spec/glob_splat.sh gs.1-13; wordsplit 22.14-19
    migrated; 0 regressions. **Tier 3 fully COMPLETE.**)
14. `argv-snapshot-to-outparam` (P2) — DONE 2026-07-08 (DD#60 revision:
    `array a=$(argv N)` → the out-param builtin `argv [N] a`. New
    BuiltinCmd::Argv + SsaInst::ArgvFill (mirrors Mapfile/MatchInto);
    removed SsaValue::ArgvSnapshot. Auto-declares the array (transform emits
    an empty-array Assign to reserve the versioned `a_0` slot, then fills —
    also fixes the mapfile-style ICE on `len` of an undeclared fill target)
    + registers it in the type checker. Fills a tainted array (DD#61):
    taint_read_target extended to argv. Old `array a=$(argv …)` → SB5025.
    native/c/janet all 37 argv specs; 0 regressions. **Tier 3 COMPLETE.**

**Tier 4 — c/janet backend parity** (native is the gate; after tiers 1-3)
15. `c-janet-dd55-abort-channel` (partial — do first) ⧉ reconcile overlap
    with `dd55-abort-channel-remaining` (both are DD#55 abort in c/janet)
16. `dd55-abort-channel-remaining` (P2) — pipefail/c-cmdsub/descriptive
    msgs/janet pipeline (moved from Tier 1: all backend)
17. `c-janet-compound-pipeline-stage`
18. `cf-backend-gaps-brace-surfaced`
19. ~~`echo-read-inline-pipe-cjanet`~~ — DONE (dc10423): DD#20 lastpipe
    read/mapfile for c+janet + bare `array`/`map` decl fix (janet). Extended
    beyond the single-read gate to 22.4-10/22.12. c 47→38, janet 70→61.
20. ~~`janet-arith-string-coercion`~~ — DONE (28db76d): `_num` coercion at
    arith use sites; DD#72 0x/0o/0b; fixes 58.2/3, cf.f5/f8, 16.2, nl.s*.

**Tier 4 extra (not a pre-filed task) — DONE this session (2026-07-14):**
- DD#58 int-func-in-arith c+janet (73de6d2): c 58.1-8 (fixed the `$?`-assign
  → __exit_status mis-wire); janet 58.1/4/5/8 (nullary `(f)` call + `_ret`
label numeric return). janet 58.2/3 → DONE via janet-arith-string-coercion.
- dtoi/itod fixed-point intrinsics c+janet (28948e7): 36.1-4 both modes.
- janet arith string→int coercion `_num` (28db76d): 58.2/3, cf.f5/f8, 16.2,
un-guarded nl.s1-10.
- c_var_name distinct special-var names + enforce transform diagnostics for
  c/janet/manifest (e7c530f): fixed dup-`__ptr` compile error + made the
  oracles reject SB4016-class programs like native. Fixed 0.6/46.4/48.1/
  55.12/13/14/17c/16.2/0.5.
- dtoi/itod (28948e7), janet arith coercion (28db76d), unset collection
  (a355d38: 30.11), read-herestring + multi-part DoubleQuoted (3511fe4: 24.6).
- **DD#55 pipeline semantics pass** (845f494, DG-ratified): context-independent
  aggregate (pipefail over non-suppressed segments); position decides only
  abort-vs-branch. Fixed pipefail-in-conditions, leading-`-` later-stage
  abort, non-first-`-` stage-drop; `pipeline &&/|| C` → SB4023. Per-segment
  `suppressed` mask on SSA Pipeline. specs 55.32-40. native 792→801.
- Session tally: **c 47→13, janet 70→37, native 0** (gate green throughout;
  10 code commits; 0 regressions). Remaining Tier 4 is feature-sized: 56.x
  signals/END, DD#55-abort remainder (c external cmdsub 55.7/8/9, `$!`
  38.5/55.19, descriptive msgs), janet control-flow singletons (cf.f9/f10/
  f12), scattered singletons. See journal 132.
21. `cmdsub-followups-taint-intrinsics-c`
22. `oracle-blessall-mapfile`
23. `janet-getpid-class-intrinsics` — janet execs getpid-class names as
    fake commands; add os/getpid/os/getenv/os/cryptorand/… dispatch

**Tier 5 — hardening / infra / cleanup** (compiler-related → before convert)
22. `eliminate-ice-arms-via-types` (bug-surface; may pull up near tier 2)
23. ~~`excise-dormant-coercion-machinery`~~ — DONE (already excised in
    `4e87034`; verified 2026-07-08, no Coercion refs remain)
24. `audit-string-keyed-builtin-dispatch`
25. `fuzz-validator`
26. `brush-rerun`
27. `r47-validate-rs-per-rule-tests` (P7)

**Tier 6 — convert (last among work)**
28. `converter-positional-use-needs-bless` (P2) — RESOLVED (DG 2026-07-08:
    option 1, hoist + placeholder bless + MANUAL marker, $HOME-style)
29. `converter-case-to-dispatch` (P3, policy resolved)
30. `converter-var-modifier-migration` (P3)
31. `converter-compound-postfix`
32. `dd67-bash-sugar-sweep` §3 only — sash-convert canonicalize mode
    (compiler §1/§2/§4B already done; §3 couples to dd68 brace forms)
33. `tilde-vs-dd53` converter half — rewrite bash `~`/`~/path` →
    `$(getenv HOME)` (+ bless); compiler half done (SB4036)

**Deferred to the very end:** `audit-rust-test-suites` (+ superseded
`rust-test-suite-rot`, `stale-rust-test-suites`) — revisit its specific
actions with DG at pickup.

**Out-of-band (not in the linear sequence):** `cohort-complexity-cog-cc`
(perpetual ratchet, pre-commit-gated), `r47-promote-monitoring-rules`
(process).

### Session 2026-07-05 (cont.) — DD#72 numeric literals in arithmetic

DG questioned arbitrary bases ("we have 0xff/0377 — why base#?"). Became a
full numeric-literal model + fixed **4 silent miscompiles** (`16#ff`→16,
`2#1010`→2, `0o10`/`0b1010`→0) plus 2 silent-error sites (unknown-byte skip,
no leftover-token check).

- **Model (DD#72):** constants in `$(( ))` = decimal + `0x`/`0o`/`0b`;
  reject leading-zero (`010`/`0377`) + `base#` + bad-prefix-digit cleanly
  (SB4035). Runtime string→int (`sash_atoi`) = honor `0x`/`0o`/`0b`, else
  decimal with leading zeros ignored (`x=010`→10, the anti-footgun; makes
  bash `10#$var` unnecessary). DG: "honor prefixes but ignore leading
  zeroes without the x/o/b".
- `arith.rs` (BadLit→SB4035, radix scanner, trailing-token reject),
  `lexer.rs` (`#` not a comment in arith), `sash-rt/inner.rs` (atoi radix
  prefixes). `spec/arith_literals.sh` new (native/c 31/31; janet skips the 9
  string-coercion tests — filed `janet-arith-string-coercion`). Migrated
  arithmetic.sh 26.7 (`017` now rejects) +26.7b (`0o17`). **0 regressions.**

### Session 2026-07-05 (cont.) — P1: functions as pipeline stages

DG picked `bug-function-in-pipeline` (P1, confirmed live: `fn f { echo hi
}; f | cat` crashed with `sash: bug: bare command passed to sash_exec`,
breaking the SB4021-4024 "wrap in a function" escape hatch). **Native
fixed, 0 regressions.**

- A known-function `Command` pipeline stage now inlines as a forked
  `__compound_` child (args bound as $1..$N via `setup_callee_positionals`,
  `$@` expanded, `return` → ret_block, subshell semantics via
  `state.snapshot()`/`restore()`), instead of exec'ing the bare name.
  Fixed both `handle_regular_pipeline` and `build_producer_stages`
  (pipe-while-read/lastpipe); shared `push_command_stage` helper.
- **c/janet**: pre-existing gap — they have no `__compound_` support at all
  (even `{ }`/for/if stages fail: c `bare command`, janet `unknown symbol
  p1`). DG-decided to split it out → `c-janet-compound-pipeline-stage` (P2).
  `spec/pipe_functions.sh` (pf.1-11) is native-only guarded (like brace.sh).

### Session 2026-07-04 (cont.) — DD#65 var modifiers: `:-` word (`:- ≡ "…"`)

DG-directed, spec-first, **0 regressions native/c/janet** (native 648→663
before this entry's tests; c 52 / janet 74 unchanged). DD#65 build-up:
- **toupper/tolower** builtins (bff8f82), **basename/dirname** sugar
  (e755fc4) — earlier this session.
- **`${var:-DEFAULT}` full interpolation** — DG settled the shape as
  **`:- ≡ "…"`**: the default is *exactly* a double-quoted-string body.
  Quotes (`'`/`"`) are ordinary literal bytes (no strip/group), whitespace
  verbatim, `$…`/`` `…` `` interpolate; the **C-string is the any-byte
  hatch** (`${x:-$'a\x7db'}` → `a}b`). For full consistency the `${…}`
  brace matchers (`scan_brace_expansion` + `consume_dq_brace_expansion`)
  dropped their quote/`\`-skip, so the **first unnested `}` closes**
  (`${n:-'a}b'}` = `${n:-'a}` + `b'}`). Bare env var in a default now
  rejects SB5018 (real interpolation context).
- Lowering: operand → `ExprKind::DoubleQuoted` → `SsaValue::DefaultValue
  { var, default }` (native + c select var-or-default; janet interpolates
  at IR level). spec/var_modifiers.sh vm.dv1-11 (+ tu/bn) = 24, all modes.
- DD#65 doc updated to the `:- ≡ "…"` model (superseded the earlier
  "interpolating word context" framing). **drops** now shipped too:
  every `${var:OP}` op except `:-` rejects **SB4034** (one uniform code,
  generic-builtins message; sash-convert owns migration —
  `converter-var-modifier-migration`). The kept `:s/` sed op was
  disambiguated from the dropped `/`-glob by remapping the glob path to an
  internal `glob_replace` operator. 0 regressions native/janet; c +1
  (c:39.4). Remaining: remove now-dead backend dispatch (follow-up).
  **Also dropped the per-element array forms** `${arr[@]#p}`/`[@]/p/r`
  (were an asymmetry vs the scalar drop; DG chose consistency) → reject
  SB4034, migrated `types.sh` 33.16-33.18 to `for x in @a`+sub/gsub, which
  *fixed* them in c/janet (c 52→48, janet 74→71; native still 0). Runtime
  strip-fn cleanup filed (`remove-dead-strip-runtime`).

### Session 2026-07-04 (cont.) — language sweep DD#65-68 part 1

DG: "do the language sweep." Shipped (0 regressions, native clean):
- **dd67** verified already-done (name()/function/backticks/[literal).
- **dd68 brace control flow** (c98ed29) — if/while/until/for `{ }`;
  parser-only, same IR; spec/brace.sh 25 native-only.
- **dd66 dispatch form** additive (5231dd1) —
  `[[ SUBJECT ==|=~ PAT { grp } … || { grp } ]]`; == exact / =~ regex;
  statement-only (SB4028); ir::Case gained `op`; native+c+janet;
  spec/dispatch.sh 22.

Remaining (filed): **dd65 var modifiers** (atomic drop+rebuild).
See journal 126.

### Session 2026-07-04 (cont.) — DD#66 remove `case` (sweep part 2)

DG: "continue — remove case first." Destructive half of DD#66 shipped
(f71c04a) spec-first, **0 regressions native/c/janet**:
- `case`/`esac` → reserved-word reject **SB4029**; `;&`/`;;&` tokens gone;
  `CaseItem.fallthrough`/`resume`, `=glob`, `sash_fnmatch`/`fnmatch_impl`
  all deleted (filename `sash_glob` kept). `case.merge`→`dispatch.merge`.
- control_flow.sh cf.c1-c12 migrated to dispatch (+cf.c13/c14 reject);
  janet cf.c6 flipped green → closes `bug-janet-case-glob` (dissolved).
- Converter: MANUAL dispatch annotation for `case` (subshell convention);
  full auto-rewrite is a DG decision → filed `converter-case-to-dispatch`.

Remaining sweep: **dd65 var modifiers**. See journal 127.

### Session 2026-07-04 (cont.) — DD#71 operand-words design (from dispatch follow-up)

Design session (no code yet). DG steered "how does a `=~` regex include a
space?" into **DD#71** (`notes/decisions/DD71-operand-words-value-bindings.md`,
accepted):
- **`=~` is sugar for `match()`; its RHS has no unique handling** — an
  ordinary *operand word* (absorb metachars to whitespace, quote/`$`-aware),
  same routine as `==`. Delete `parse_regex_rhs` + the `{` gap heuristic.
- **Absorb-mode scope** = value positions that aren't connectable commands:
  `[[ ]]` operands/patterns + assignment/declaration RHS. Command args stay
  operator-mode. **Assignments are value bindings, not commands** (no status,
  not `&&`/pipe/bg-able).
- Falls out: **multi-exact dispatch arms** (`== a b c`, parser-only —
  backends already do multi-pattern); both `=~` bugs fixed by construction.
- Dynamic regex **supported** (validate-static / runtime-abort-dynamic).
- Filed: `operand-word-absorb-mode` (impl), `reject-nonstatus-in-boolean-
  context` (the DD#70 dual — `x=hi && …` becomes a clean reject), plus two
  DG-caught bugs `bug-dispatch-literal-star-wildcard` (P1: `*` wrongly a
  wildcard) and `bug-echo-builtin-array-splat-ice` (P2: `echo @a` ICE).
  `bug-regex-rhs-space-quote` superseded by DD#71.

### Session 2026-07-04 (cont.) — DD#71 Cluster A implemented (spec-first)

DG: "Cluster A — DD#71 operand-words, spec-first." Shipped all five coupled
components, one commit each, **0 regressions native/c/janet** every time.
Native passing 595 → 635. See journal 128.
- **star-wildcard** (95491f5) — dispatch `*` is a literal; `is_default` flag
  replaces the `*` sentinel.
- **operand-word** (9c71a99) — `=~` RHS is an operand word
  (parse_operand_word); deleted parse_regex_rhs. Fixes quoted spaces, the
  silent-`foobar` concat, and dynamic `=~ $pat`; comptime regex validation
  extended to any static operand shape.
- **multi-exact** (ce2dfec) — `[[ $x == a b c { … } ]]` (parser-only).
- **boolean-reject** (fee53a6) — SB4025: a value binding in a boolean context
  (if/while cond, left of &&/||, `!`) is a compile error. Fixes silent
  `x=hi && echo $x`.
- **assign-absorb** (3ede406) — assignment RHS absorbs `& && || |` as
  literals (`a=foo|cat` → literal). AbsorbMode enum; `=(…)` unaffected.

Filed `bug-test-operand-concat-splice` (P3, pre-existing `run$suf`). Tasks
operand-word-absorb-mode / reject-nonstatus-in-boolean-context /
bug-dispatch-literal-star-wildcard → done. Remaining sweep: **dd65 var
modifiers** (vet-var-modifiers).



DG: "tackle the whole batch." Shipped the c/janet parity cluster spec-first,
one commit per change, full-suite diff each time. **Native 0-failing
throughout; zero regressions.** Pre-existing baselines: **c 58→52, janet
106→75.**
- janet pipeline EAGAIN flakiness → blocking `(os/pipe :WR)` (cb6086a).
- janet multi-pattern `case a|b` → match guard alternation (3481512, P1).
- janet argv trio DD#60 via `(dyn :args)` + fixed `for x in @a` splat +
  `join` (430f2e4, −24 janet).
- `&&`/`||` branch on exit status (not value) in c+janet; every command
  sets the status var (032386a).
- DD#55 (A) external-command abort in c+janet (707832f, partial — external
  done, END-routing via `Emit.end_block`; remainder in
  `dd55-abort-channel-remaining`).
- `[[ =~ ]]` test operator implemented in both oracle backends (9ba4bbe);
  `echo|read` guarded to native + filed (c9d67c1).

Followups: `bug-janet-case-glob`, `dd55-abort-channel-remaining`,
`echo-read-inline-pipe-cjanet`. See journal 125.



DG ratified the disposition of out-of-range integer literals. Probing
found the bug was broader than the task: **six** silent `0` miscompiles.
Policy A (bash-faithful): a **decimal** literal whose magnitude exceeds
i64 → compile error (SB4026); **hex/octal** are 64-bit bit-patterns
(`u64`→`i64` reinterpret at scan time, so `0xFFFFFFFFFFFFFFFF` = -1);
**i64::MIN** is expressible in arith via a unary-neg peephole
(`-9223372036854775808`). Non-arith `int x=NNN` out-of-range also → SB4026
(validator). New `ATok::BadNum`; `arith_range_error`. spec/arithmetic.sh
section 26 (10 cases) native+c green, janet guards the i64 boundaries.
Native 0-failing (545); spec.db diff = zero regressions. Also resolved
two DD#25 follow-ups: left-shift = no trap (bits ≠ arithmetic, already
correct); const-fold overflow = runtime trap stands (compile-error not
"essentially free" — detection is discarded into a runtime op). Found +
filed a new bug: based literals (`16#ff`→16) drop their digits because
`#` is eaten as a comment (bug-arith-based-literal-hash-comment). See
journal 124.



DG-directed cleanup. Heredocs (`<<`/`<<-`) are dropped (lexer emits a
`__HEREDOC__` sentinel → validator SB5002 rejects, pointing at `<<<`). But
all the `*heredoc*`-named codegen/runtime/emit functions actually
implement **herestrings** (`<<<`, `RedirectOp::HereString`) — a misleading
holdover. Renamed across the 6 herestring files (incl. the C ABI symbol
`sash_heredoc_redirect` → `sash_herestring_redirect`, consistent across
sash-rt + native setup + emit_c). Left the genuine heredoc-rejection
machinery (`scan_heredoc_sentinel`, `check_heredoc`, `heredoc_diag`,
`__HEREDOC__`, SB5002) untouched. Pure rename: native 0-failing (535);
c=58/janet=106 byte-identical; herestrings verified on native + C,
heredoc still rejected.

### Session 2026-06-26 (unattended, round 3) — fix: `<<`/`<<=` mis-lexed as heredoc

Closed `bug-arith-left-shift-heredoc` (last arith *lexing* gap). `scan_lt`
tokenized `<<` as a heredoc sentinel regardless of context, so
`(( x << 2 ))` / `$(( … ))` / `<<=` dropped the operator (`>>` was fine).
Fix: `Scanner.arith_depth` (set by `collect_arith_content`); `scan_lt`
emits `Word("<<")` in arith mode → arith re-lexer recombines to Shl, and
the `<op>=` separator rule handles `<<=`. spec 31.1j-m; native 0-failing
(535); zero regressions; heredoc rejection / `>>` / `<=` intact. Whether
left-shift *overflow* should trap is left as a DG decision. Journal 123.

### Session 2026-06-26 (unattended, round 2) — DD#25 overflow sweep: Pow + Div

Continued the DD#25 overflow work. Probed the remaining overflow-capable
ops: **Pow** saturated silently (→ now traps), **Div** `MIN/-1` SIGFPE
core-dumped (→ now clean trap); **Neg** already traps (`0-x`→ssub_overflow)
and **Mod** `MIN%-1`=0 (srem, correct) — no fix needed. Fixes: `sash_pow`
calls `sash_arith_overflow`; native+C div guard `MIN/-1`. spec 25.9–25.12;
native 0-failing (531); c=58/janet=106 unchanged. Shl still blocked on the
`<<` heredoc bug. Filed `bug-arith-literal-out-of-range` (out-of-range
literal → 0). Journal 122.

### Session 2026-06-26 (unattended) — P0 bug: compound-assigns other than += were no-ops

Found while verifying the overflow fix. `(( x OP= N ))` for OP ∈
{−,*,/,%,&,|,^} was a **no-op** (only `+=` worked) — pre-existing,
spec-uncovered. Root cause: the shell lexer splits `-=`/etc. into `- =`
(no arith context; `+=` is a single `PlusAssign` token), and
`needs_separator_space` inserted a space that the arith byte-lexer then
mis-read as `x - 0` (a comparison, no write-back). The `<=`/`>=`
special-case existed but didn't cover the compound-assign operator chars.
Fix: extend it to `- * / % & | ^` (parse/arith.rs). spec 31.1d–31.1i;
native 0-failing; zero regressions. `<<=`/`<<` left-shift still broken
(heredoc-vs-shift, `bug-arith-left-shift-heredoc`). Journal 121.

### Session 2026-06-26 (unattended) — P0 bug: runtime overflow silently wrapped

Started `abort-diagnostics-remaining`; its DD#25 overflow gate exposed a
**stop-everything P0**: runtime integer overflow (+, -, *) **silently
wrapped** (cranelift plain `iadd`/`imul`; C plain ops; const-fold
`wrapping_*`), violating DD#25 ("traps at runtime"). Spec 25.2/25.3 only
"passed" by accident — overflow → exactly i64::MIN → `itoa` panicked on
`checked_neg`.

Fixed (spec-first, native + C; janet bignum out of scope):
- `sash_arith_overflow()` → `<argv0>: arithmetic overflow`, exit 2.
- native `emit_checked_arith` (sadd/ssub/smul_overflow); C
  `__builtin_*_overflow`; const-fold `fold_arith_op` → `Option` (defers
  overflow to the runtime trap, decision-free).
- `itoa_inner`/`write_i64`/`itoa_buf` use `unsigned_abs()` — i64::MIN
  echoes correctly.
- spec/arithmetic.sh 25.4-25.8. native **0 failing**; c=58/janet=106
  unchanged (zero regressions). DD#25 note corrected; journal 120.

Follow-up (filed in `bug-runtime-arith-overflow-wraps`): Pow saturates,
Shl/Neg/Div overflow-capable; const-fold trap-vs-compile-error is a DG
sub-decision. The other 5 `abort-diagnostics-remaining` gates remain open.

### Session 2026-06-25 (cont.) — DD#63 captures (out-array + backrefs)

Both capture surfaces, all three backends, after DG resolved D1–D6.

- **`match S P ARR`** out-array: bash-parity length (engine `Reprog.nsub`),
  non-participating group → `""`, clear-on-entry, still (B) must-consume.
  New `SsaInst::MatchInto` + `sash_regex_match_into` + `sash_table_clear`.
- **`\0`–`\9` backrefs** in `sub`/`gsub` (`sash_regex_replace` template
  pass); invalid `\`-escape → **SB4033** (validate, all-modes).
- **Taint airtight (D4):** tainted subject rejected at the read site
  (SB5020) — captures always clean, no DD#61 change.
- **Engine:** `Reprog.nsub` exposes the group count. `build.rs` now tracks
  csrc/headers via `rerun-if-changed` (stale-object gotcha fixed).
- **Backends:** native (codegen) + C (`emit_match_into_c`, shared runtime)
  + janet (`sash-regex captures` CLI op + `string/split`; fixed a
  pre-existing `$(len ARR)` janet gap, +1 janet_test).
- Spec: regex.sh 63.cap1–12. Native **28/28, 0 failing** (gate); c & janet
  24 + 4 native-only skips; zero regressions (c=58/janet=106 baseline).
- **Finding:** the `sash-rt-tests` harness is pre-existing broken
  (references removed `sash_streq`/`sash_arith_eval`; not on `mk test`).
  Cargo core suites have ~122 pre-existing stale-test failures (old
  `declare`/`${#arr[@]}` syntax) — not regressions.

### Session 2026-06-25 (cont.) — DD#70 (A)/(B) write-up + match boolean

- **DD#70 written** (`notes/decisions/DD70-error-taxonomy-A-B.md`):
  (A) runtime abort-if-unchecked vs (B) compile-time must-consume,
  refining DD#55. DG resolved: booleans must be consumed (compiler-
  enforced); ignored-result codes adjacent (SB4018 discarded boolean,
  SB4019 `-` on a (B) construct, next to SB4016/SB4017); verbatim argv[0].
  Contract-spec task `dd70-error-model-contract-spec.md`.
- **`match` boolean command (DD#63 1b-bool)** shipped on all three
  backends (commit bbe5816): `match S P` → exit 0/1, sets the if/&&/||
  boolean. native (sash_regex_match), C (sash_regex_match), janet
  (sash-regex `matchq` — quiet, no stdout leak). Bare `match` → SB4018,
  `- match` → SB4019 (native; via a `suppressed` counter +
  check_match_must_consume mirroring SB4016). Fixed a janet `:` (Noop)
  gap. regex.sh 63.1c-2f.
- **Finding:** transform diagnostics (the whole SB40xx must-consume
  family incl. the existing SB4016) are native-only — `--emit-c`/`--janet`
  don't surface them. Must-consume compile-error specs are `in_modes
  native` until that's fixed.

### Session 2026-06-25 — DD#55 W3: syscall aborts + stderr diagnostics

- **cd / echo / redirect-open now participate in the DD#55 (A)-channel**
  (native; commits `8a40ddc`, `f9d833b`). Three bugs fixed: `cd` never
  set test_result/aborted (`if cd /tmp` took else; failed cd continued);
  `echo` ignored `write(2)` failures ("echo always succeeds"); a failed
  redirect `open(2)` **silently misrouted** output to fd 1.
- **Redirect model (DG):** `> file` is the implicit `exec N>file` that
  runs FIRST; a failed open makes the command fail without writing (no
  misroute), aborting unless in a condition (if/||). `-` binds to the
  command, not the open, so `- echo > /bad` aborts. `in_condition` derived
  at the emit site from the Branch terminator — **no new IR bit**. Fixed
  for echo and external commands.
- **Descriptive diagnostics (DG: all abort cases):** every abort prints
  `<argv0>: <context>: <reason>` to stderr, prefixed with the program name
  (argv[0]). `<reason>` = OS strerror via a hand-rolled no_std errno table
  (`sash_emsg`, `sash_last_errno`, `errno_str`). Fires only on the actual
  abort; caught/`-`-suppressed stay silent. This pass = cd/redirect/echo
  (errno cases).
- **Error model ratified:** (A) runtime abort-if-unchecked for real
  failures (`-` suppresses the verdict); (B) compile-time must-consume for
  pure logical results (`match`/`[[ ]]`/`-> int`; SB4016 already enforces).
- **Harness:** native runner now captures runtime stderr into `rerr`.
  Spec `errors.sh` 55.20-30 (native only). Native 0 failing; c=58/janet=106.
- **Deferred (filed):** `abort-diagnostics-remaining.md` (external/pipeline/
  cmdsub/bg/overflow messages), `c-janet-dd55-abort-channel.md` (C/janet
  lack the whole (A)-channel — un-guard 55.20-30 cross-mode after).

### Session 2026-06-21 (cont.) — dd67 sugar, DD#23 resolution, native green

- **Native spec suite is GREEN (0 failing).** cf.c7/cf.c8 rewritten to the
  **star-only** glob contract (DG, DD#39): `?` and `[class]` are literal
  characters in `case` patterns, not metacharacters — only `*` globs.
- **DD#23 resolution hoisted ahead of the front-end** (`599fa74`): new
  `sash/src/resolve.rs` rewrites external command heads (incl. cmdsub heads)
  to absolute paths before taint/type checks, so the CST taint pass sees the
  real exec form. Unresolved = fatal (SB5011, now incl. cmdsub heads).
  `--sysroot` dir + `.squashfs` honored. Structural rejections (heredoc,
  export/declare) now run before resolution.
- **dd67 sugar sweep**: `name() { }` (`e9849ad`), flat backticks (`def5c40`),
  single-bracket `[` is an ordinary literal (`c3875d1`).
- **Rust suites (sash/tests/) have significant bit-rot** (DG) — they are NO
  LONGER a regression gate. **spec.db is the source of truth.** Filed
  `audit-rust-test-suites`. Their nondeterminism: codegen tests call
  `compile_to_object` in-process, sharing global compiler state.
- **Heredoc removal is INCOMPLETE**: lexer rejects `<<` (→ `__HEREDOC__` →
  SB5002), but `RedirectOp::HereDoc` + its codegen/emit_c/emit_janet/validate
  handling remain as dead code. Filed `finish-heredoc-removal`.


### Session 2026-06-21 (cont.) — DD decision review

Walked every candidate DD with DG; **all now resolved** (DD#57 implemented;
DD#62–69 accepted). Commits `ed4a1e4`…`2a68f2b`. Highlights:

- **DD#64 addendum** (new): `$'…'` is recognized **inside `"…"`** as an
  escape-aware, compile-time-constant expansion — every `$`-form now
  expands in `"…"`. A C-string is a universal literal-byte encoder, giving
  full literal control inside an interpolated string (even a literal `"`),
  no concatenation. Resolves the DD#63/#65 default-vs-replacement context
  uniformly. Task `cstring-in-double-quotes`.
- **DD#62**: undeclared scalar read → compile error under **strict
  definite-assignment** (`${x:-}` is the opt-out). Task
  `reject-undeclared-scalar-read`.
- **DD#63**: `\`+non-digit in a `sub`/`gsub` replacement is an error
  (`Resub` stride = 16B); replacement/`:-default` are one word context
  (replacement adds only the `\0`–`\9` pass); `$(cmd)` allowed (only
  external captures taint).
- **DD#65**: prune accepted; `$(cmd)` allowed in `:-default`; `:?` is
  scalar-only (`@{a:?W}` splat is DD#69); `${#var}` kept, `@X` rejected.
- **DD#66**: `[[ ]]` two-form + `case` removal; **numeric dispatch dropped**
  (==/=~ only). **DD#67**: re-add `name()`/`function name`; flat backticks
  kept (nested rejected); canonicalize switch in `sash-convert`; sweep →
  keep/drop table DG ratifies first. **DD#68**: brace control flow,
  converter default preserves `then/do`. **DD#69**: ratified.

Doc fixes (`c15b430`, `d9f28ce`): corrected stale tree-sitter/CST docs
(hand-parser is AST-based; `emit_source` coercion path is dormant →
`.sash_source` is verbatim). Tasks filed:
`excise-dormant-coercion-machinery`, `cstring-in-double-quotes`,
`reject-undeclared-scalar-read`.

**Still-pending non-DD decisions:** array-splat native runtime-argv ABI
(B1/B2), `bug-janet-int-overflow` approach, `tilde-vs-dd53`.

### Session 2026-06-21 — autonomous backlog sweep (cont.)

Five tasks landed (journal 115), all multi-backend verified with zero net
regression (native cf.c7/c8 only; c/janet diffed per-mode; complexity
gates green):

- **bug-dollar-hash-in-arith** (`4b48920`): `$#`/`$?`/positionals now read
  inside `(( ))` (lexer `scan_arith_dollar` scans the special single-char
  names; DD#46 enforced in arith via `check_positionals`). Fixed a
  pre-existing janet double-paren arith-value crash (repaired 6 specs).
- **bug-janet-arith-condition** (`a3fc02e`): janet `(( ))` conditions were
  constant-`true`. Now `emit_condition_inline` handles `ArithTest`;
  comparisons/logicals yield bash 1/0 ints; int counters stay numeric;
  string `[[ ]]` tests stringify int operands. janet −8 (cf.w1–w7, cf.i3).
- **nonbrace-dynamic-subscript** (`44dc0b3`): `$a[$i]` reads `${a[$i]}`
  (decision B) via `try_split_subscript_arm` (peek_n lookahead).
- **bug-c-janet-concat-in-value** (`15654ef`): c `lower_value_as_str`
  gained a `Concat` arm (`emit_concat_value` via runtime `sash_concat`).
- **bug-janet-indexed-assignment** (`4db609e`): janet `a[i]=v`/`a[i]+=v`
  now `(put …)` with collection-typed index (`EmitCtx::map_vars`).
  types.sh 33.11–33.13 fully un-guarded (all modes).

Filed for review: **array-splat-command-arg** — investigated (feasible
via `sash_alloc`+table iteration); held for DG on the native runtime-argv
ABI choice (B1 codegen-side vs B2 runtime-side). Filed bugs:
`bug-janet-int-overflow`, `bug-janet-array-dynamic-subscript-read`.

### Session 2026-06-21 (cont.) — DD implementation phase (journal 116)

Implemented the resolved DDs in dependency order; all multi-backend
verified, native baseline held (cf.c7/cf.c8 only), rust 184=184 (oracle
order-flakiness only). Commits:

- **cstring-in-double-quotes** (`ba345da`): DD#64 addendum — `$'…'`
  recognized inside `"…"` (lexer `scan_dq_cstring`). Specs wordsplit
  64.4–64.8 (all backends).
- **array-splat-command-arg** (`d24084e`): `@a` expands to one argv entry
  per element (DD#20/#22), all backends. A1 `SsaValue::ArraySplat` (taint
  inherits from the array); B1 native `build_argv_values` (unifies exec/
  pipeline/cmdsub; runtime `sash_alloc` + Cranelift fill-loops); C VLA;
  janet `;`-splice. Specs wordsplit 22.14–22.19. **Folded in a spec-
  hygiene fix**: wordsplit had a misplaced mid-file `tally` (its
  "Additional coverage" ran only in native) → moved to EOF; surfaced two
  pre-existing c/janet gaps (39.10 `\w`, 22.12 echo|read, filed).
- **glob-splat Part 1** (`c8b3530`, `90ed26b`): DD#69 globs-are-arrays.
  1.2 — inline `*` in command-arg position is SB5029 (use `array a=(*)`).
  1.1 — `for f in *` routes through glob→array→per-element
  (`try_dispatch_glob`); **fixed a C-backend glob SIGSEGV** (`emit_glob_c`;
  emit_c had a glob TODO → null table). Specs wordsplit 20.1–20.6
  (for-in glob native/c; janet doesn't glob).

**Decisions now needed from DG (filed):**
- `DECISION-glob-splat-disposition-word` — glob-splat **Part 2**
  (`@{a:-WORD}`/`@{a:?WORD}` disposition + bare-`@a`-to-external reject):
  WORD-interpolation semantics, `:?` abort format, taint compose. Blocks
  the rest of glob-splat.
- Remaining ready tasks are each design-gated per their files:
  `reject-undeclared` (typed-zero-init set + pre-SSA-vs-SSA placement),
  `vet-var-modifiers` (basename/dirname builtin-vs-external; couples to
  regex-builtins for sub/gsub), `dd67-sugar-sweep` (keep/drop table needs
  DG ratification *before* sugar), `regex-builtins`/`dd66`/`dd68`.

Other tasks filed this session: `bug-cmdsubst-builtin-not-resolved`
(`$(printf …)` passes a bare name to sash_exec — pre-existing),
`janet-pipeline-eagain-flaky`, `cjanet-wordsplit-39-10-22-12`,
`spec-wordsplit-count-desync` (done).



Worked the ready backlog in dependency order (journal 115). Landed:

- **for-loop-arith-condition** (`d76b77a`): for-in bound checks moved off
  the synthesized `[[ -lt ]]` test onto the `(( ))` arith path
  (`emit_for_bound`: `ArithBinOp` + `LetExit`); `emit_let_exit`
  generalized to any `SsaValue`; the `[[ ]]` numeric arm deleted in both
  backends (`[[ ]]` now carries zero integer machinery); direct `arrlen`
  → `sash_table_len` arm. Fixed a latent catching-construct bug
  (`for_arr.exit`/`for_at.exit` added to `is_catching_construct_exit`).
- **Real bug fixed:** `(( <= ))`/`(( >= ))` were silently wrong (the bash
  lexer splits `<=` into `<`+`=`; `collect_arith_content` inserted a
  separator space). Fixed in `arith.rs::needs_separator_space`. Spec
  arithmetic.sh 31.15–31.18.
- **subscript-append-broken** (`7ee2854`): `a[i]+=v` now read-modify-write
  (`Concat([${a[i]}, rhs])`). native; spec types.sh 33.11–33.13.
- **bug-until-arith-condition** (`6f97636`): `until` desugar appends the
  `NegateTestResult` sigil instead of patching `SsaInst::Test`, so
  `until (( ))` / `until <cmd>` are negated (and compound conditions
  negate once). 3 until codegen_tests re-enabled.

Filed bugs (pre-existing, surfaced here): `bug-dollar-hash-in-arith`
(`$#` not read in `(( ))`), `bug-c-janet-concat-in-value`,
`bug-janet-arith-condition` (janet `(( ))` cond always-true). Several
ignored c/janet cases are guarded `in_modes native`.

All commits: native baseline (cf.c7/c8), c/janet no new failures, rust
no net change, gates green. **Next ready (large/design):**
`array-splat-command-arg` (runtime-variadic argv → unblocks DD#69 glob
work), the c/janet concat/janet-arith backend gaps, and the `$#`/DD#46
and non-brace-subscript design questions.


### Session 2026-06-20 — DD#62 section A: `[[ ]]` operands → `SsaValue`

The operand→`SsaValue` refactor (the last piece of DD#62). `SsaInst::Test`
/ `Bless` / `BlessAll` now carry `SsaTest`/`SsaTestRhs` whose value
operands are pre-lowered `SsaValue` (only `=~`/`=glob` patterns stay raw
text). `test_to_ssa` does the lowering in transform; codegen `tests.rs`
+ `emit_c.rs` consume it. This routes `[[ ]]` subscripts through the SSA
value path (`subscript_to_value` → `lower_subscript_value`, which knows
`assoc_arrays`), **fixing `[[ ${m[$k]} ]]`** (map dynamic key did an int
lookup) in native **and** C. Deleted the duplicate IR-`Expr` test-lowering
subtree (`lower_subscript` & ~9 siblings, `find_var_slot`/`find_var_key`,
`next_slot_version`, emit_c's `ir_expr_as_int`/`latest_var_as_int`) —
~330 LOC. Closes `dynamic-map-subscript-in-test`. Specs `types.sh`
62.11–62.15. See [journal 114](journal/114-dd62-section-a.md).

Three refactor-introduced bugs caught + fixed before commit (none
shipped): (1) synthetic for-in `[[ __ai -lt ${arr arrlen} ]]` bound
checks ICE'd after I dropped codegen's numeric arm (SB5019 only rejects
*user* numeric ops) — restored it both backends; (2) scalar
`bless x [[ … $x … ]]` resolved `$x` to the not-yet-populated target —
`test_to_ssa` now runs before `state.define`; (3) bless-all binders
mis-versioned (`x_0` vs codegen's `x_4`) — `BlessAll.binders` is now
`Vec<VarRef>` with transform-assigned unique versions.

Zero net regression: native back to baseline (cf.c7/c8 only); c/janet no
new failures (both improved); rust 186 (flaky `oracle_pipeline_grep`
only). Next in the DD#62→68 implementation order: **DD#63** (regex
builtins; absorbs DD#62 C/D).



### Session 2026-06-19 (cont.) — &&/|| left-associativity fix (DD#62 F)

`A && B || C` parsed **right-associative** (`A && (B || C)`), so
`false && echo A || echo B` ran neither branch. Root cause:
`parser.rs::binary_list_op` parsed the RHS via the full `parse_statement`
(recursing into more list ops). Fixed: `parse_trailing_list_op` is now a
left-associative loop over single operands (`parse_list_operand` =
command + pipeline/redirects, no trailing list op); `binary_list_op`
removed. Compound operands still rejected (SB4023). Native verified across
mixed/deep chains. Filed `bug-andor-right-associative` for the analysis.

Surfaced (and filed) a **pre-existing** janet/c backend bug
(`bug-andor-backend-chain-status`): those backends evaluate chains by
language value-truthiness, not exit status (e.g. `print`→nil makes `||`
mis-fire), and don't set the status after every command. Native (SSA
`lower_list_stmt`) is correct. The nested-chain spec tests (wordsplit
62.8–62.10) are guarded `in_modes native`; 62.7 runs all modes.

Zero net regression: native 417/2 (cf.c7/c8), janet 125, c 62, rust 186
(flaky oracle only).

### Session 2026-06-19 (cont.) — DD#62 sub-unit 1 + janet printf fix

**DD#62 sub-unit 1** (of several): `==`/`!=` are now **exact-only** in
native codegen (`emit_eq`/`emit_ne` drop the `fnmatch` glob branch — the c
backend was already exact via `sash_strcmp`, so this aligns native↔c);
`emit_regex` is always regex (dead quoted-literal branch removed); and
`right_is_quoted` is **deleted** (zero callers). Spec `wordsplit.sh`
62.1–62.4. `case`'s `=glob`/`fnmatch` is untouched (its own op). Remaining
DD#62: operand→`SsaValue` + delete `lower_subscript`, remove `-v`, `=~`
quoting/spaces, the `&&/||` chain.

Also fixed a **janet printf regression** the DD#64 escape fix introduced
(`printf '%s\n'` got a literal `\n` once literals were escaped): the
format string now has printf escapes interpreted before Janet-escaping
(`interpret_printf_escapes`). Caught by `oracle_general_command`.

Verified zero net regression: native 417/2 (cf.c7/c8), janet 125, c 62,
rust 186 (only flaky `oracle_*`). Next DD#62 sub-units, then DD#63.

### Session 2026-06-19 (cont.) — DD#64 implemented (no shell backslash in "…")

First implementation phase of the DD#62–68 arc. **DD#64 done.** The only
real change was `lexer.rs::scan_dq_escape`: inside `"…"`, `\` is now a
verbatim byte (only `\<newline>` continuation survives) — `"a\\b"`→`a\\b`,
`"v\$x"`→`v\`+interp, `"a\"…`→`a\`+close. Bare words and regex-in-a-word
were already verbatim (no change). `\}`/`\/` stay deferred to DD#63/65.

Exposed + fixed a **janet backend bug**: `emit_expr_inline` emitted
`Literal`/`SingleQuoted` raw, so backslashes/quotes weren't escaped for
Janet string syntax (`"a\\b"` → 1 backslash at runtime). Now wrapped in
`escape_for_janet_string`.

Spec: `spec/wordsplit.sh` 64.1–64.3 (moved before the mid-file `tally`;
note the file has uncounted "bonus" tests after `tally`). **Zero net
regression**, verified against a clean stash-baseline: native 417/2 (only
cf.c7/c8), janet 125 (baseline), c 62 (baseline), rust 186 (only flaky
`oracle_pipeline`). Next: DD#62.

### Session 2026-06-19 — DD#68 brace-block control flow + sweep task

DG green-lit brace `{ }` bodies for `if`/`while`/`until`/`for` (matching
`fn name { }` and DD#66 dispatch), with `then/fi`/`do/done` kept as DD#67
1-to-1 sugar. **Parser-only** — both forms build the existing
`If`/`While`/`For` IR, so `$?`-catching, `break`/`continue`, redirects are
unchanged.

The `if`-condition ambiguity was resolved: **the condition is a command
list ending at the first top-level `{` (the body); a `{ }` group may not be
a condition.** Unambiguous because sash already (1) reserves `{` (`echo {`,
`echo a{` are SB4014; literal is `'{'`), (2) has no `{a,b}` brace-
expansion, (3) bans dispatch-`[[ ]]` as a condition (DD#66 b). Group
conditions cost nothing to drop — `if { a; b; }; then c` ≡ `if a; b { c }`.
`else if` accepted (= `else` + nested `if`); no mixing `then`↔`{` in one
construct.

Captured: **DD#68** (`brace-control-flow`, candidate) + spec-first task
`dd68-brace-control-flow`. Also filed `dd67-bash-sugar-sweep` (DG-approved):
re-add `name() { }` as 1-to-1 sugar for `fn name { }`, add the convert/
canonicalize switch (default preserves, `--native` normalizes), and sweep
the rejected bash forms (backticks, `[ ]`, `let`, …) against the DD#67
test. No compiler code yet.

### Session 2026-06-19 (cont.) — DD#67 native-syntax / 1-to-1 sugar policy

DG set an adoption-hedging **syntax policy**: a bash form that is an
**unambiguous 1-to-1 thin sugar** over a native sash form is **kept** (both
accepted); the **default preserves** the user's familiar form, and a
**convert switch** normalizes to native-only. Forms that can't map 1-to-1
(need semantic translation or are ambiguous) are **dropped**, and the
converter rewrites them.

The 1-to-1 test resolves the open `then/fi/do/done` question (**keep** —
pure bracket alias over `{ }`, DD#68) and confirms the `case…esac` /
`;&`/`;;&` **drops** (DD#66 — glob≠exact/regex isn't 1-to-1). It also
surfaces re-add candidates the policy invites reconsidering: `name() { }`
(currently only `fn name { }` is accepted) and backticks `` `…` `` for
`$(…)`. Semantic breaks (DD#64 no-backslash, DD#62 `==` exact) are NOT
sugar candidates — they change meaning, not spelling.

Captured as **DD#67** (`native-syntax-sugar-policy`, candidate). Default =
preserve (sugar round-trips through `--emit-source`); convert switch =
native-only (build on `--emit-source`/`sash-convert`). Next: DD#68
brace-block control flow applies it (`if/while/until/for` brace bodies,
`then/fi/do/done` as sugar). Open: `name()` re-add, backticks, flag name,
a rejection sweep. No compiler code yet.

### Session 2026-06-19 — DD#66: `[[ ]]` dispatch form replaces `case`

The `case` redesign converged. Instead of a new `switch` keyword (brace-
overload) or regularized `case`, **`[[ ]]` becomes sash's single match
construct in two forms** (DG's idea):
- **boolean test** `[[ $x == foo ]]` — yields `$?` (unchanged).
- **dispatch statement** `[[ $x == foo { … } bar { … } || { … } ]]` —
  runs one matching group. `{` after the first pattern selects dispatch.

Decisions (DD#66): `==` is **strict exact equality** in both forms (no
alternation — that would make the boolean form a degenerate glob); `=~`
is **unanchored** regex (alternation = `^(a|b)$`, not `^a|b$`); first-
match-wins, **no fall-through**; `|| { }` is the optional default (`||`
can't collide with a pattern; literal is `'||'`). The dispatch form is a
**catching construct** — `$?` resets to 0 after (the existing
`is_catching_construct_exit` machinery, `codegen/entry.rs:263`), so there
is no ambiguity over "which group set `$?`"; the boolean form yields the
test result. This is DG's clean statement-vs-test separation.

**Removed:** `case`/`esac` (keyword → reserved-word diagnostic), `;&`/`;;&`
(`SemiAmp`/`DblSemiAmp`, `CaseItem.fallthrough/resume`), the `=glob` op +
`emit_glob` + **`fnmatch_impl`** (case's sole user). **Kept:** filename
glob `sash_glob` (separate fn — glob stays for *filenames*, its intent).
Lowering reuses `ir::Case`/`build_dispatch_chain` with the op carried from
the `[[ ]]` operator (`==`/`=~` instead of `=glob`). Converter rewrites
bash `case` → dispatch.

Captured in **DD#66** (`cond-form-dispatch`, candidate) + task
`dd66-cond-form-dispatch` (spec-first). Supersedes the case-as-regex idea,
the fix-(a) glob note, and DD#63's open case-desugar; subsumes the Janet
multi-pattern bug. No compiler code yet.

### Session 2026-06-18 (cont.) — `case` redesign direction + 2 Janet case bugs

Design discussion (DG-driven) on `case`. Confirmed `case` is **literally
sugar for an if-chain**: `build_dispatch_chain` emits, per arm, a
`TestExpr value =glob pattern` → `Branch{body, next}`; `*` = uncond goto;
`a|b` = OR-chain; `;&`/`;;&` add fall-through/resume edges. `=glob` →
`sash_fnmatch` (whole-string, `*`-only) is **case's sole user** of
`fnmatch_impl` (after DD#62 removes `[[ ]]` glob). Filename globbing is a
*separate* fn (`sash_glob`, getdents) used by `arr=(*.sh)` — it survives.

**Direction (DG):**
- `case` becomes **exact-match** dispatch — not glob, not regex. Glob was
  only ever meant for filenames (`*`/`*.ext`). This dissolves the P0
  glob-pattern bug (`bug-case-pattern-glob-not-validated`) and supersedes
  the fix-(a) glob-rejection note + the case-as-regex idea.
- **Abandon `;&` and `;;&`** (fall-through/resume): untested, and the
  Janet/C backends never honoured them. Removing them simplifies
  `lower_case` (drops `resume` check-blocks, fall-through wiring) and the
  `SemiAmp`/`DblSemiAmp` tokens + `CaseItem.fallthrough/resume`.
- Envisioned core: a clean **(key → command-group) pairs** primitive
  (exact, first-match, no fall-through), with bash `case…esac` (literal +
  `|` + `*` + `;;` subset) as **sugar** over it. Glob/regex patterns and
  `;&`/`;;&` are dropped, not sugar.

**Open (DG to settle):** the clean-core surface syntax (new keyword
`match`/`switch`/`when` + brace groups vs. regularized `case…esac`);
keep bash `case` as sugar vs. clean break (converter rewrites); default
token (`*`/`else`/`default`); multi-key spelling; literal-only vs
interpolated keys. **No DD yet** — write once syntax is locked.

**Bugs found:** (1) Janet miscompiles multi-pattern arms `a|b)` → `_`
catch-all → filed `bug-janet-case-multipattern` (P1; affects arms we're
keeping). (2) Janet/C ignore `;&`/`;;&` — mooted by the abandonment.

### Session 2026-06-18 — Path B (`/`-quote after `:s`) + `${var:-}` is a word

Empirically re-checked the brace-`}` cases. Two findings: (1) the
`${var:OP}` lexer **does** look inside `${}` — `scan_brace_expansion`
brace-counts and skips `'…'`/`"…"`/`\…`, which is why `${foo:-'a}b'}`
finds the right close but `${var:s/a}b/z/}` silently miscompiles to
`a}bb/z/}` (the bare regex `}` ends the count). (2) `:-` defaults are
taken **verbatim** today — quotes leak (`${foo:-'abc'}` → `'abc'`), no
interpolation (`${foo:-$bar}` → `$bar`).

**DG decisions captured:**
- **Path B** (DD#63): the lexer treats `/` as a quote-delimiter after
  `:s`, so a bare `{`/`}` inside RE/SUB is literal — `${var:s/a}b/z/}`
  becomes a *valid* substitution. A malformed `:s` payload is a **compile
  error**; literal text after a valid `}` still concatenates. This +
  DD#65 dropping the `/`-glob op makes **both** in-operand backslash
  handlers removable (`\}` lexer.rs:660, `\/` find_unescaped_slash) —
  recorded in DD#64 (deferred → resolved).
- **`${var:-default}` is a word** (DD#65): interpolation + concat +
  quoting; unquoted `}` closes (single-quote to include), command-subs
  rejected, arith allowed.

Spec tests specified (TDD, spec-first): `regex-interpolation-delimiters`
§G (G1–G7: brace handling + malformed→error) and `vet-var-modifiers`
(V1–V11: `:-`-as-word incl. the `${foo:-'a}b'}` regression, + drop
diagnostics). Open for DG: malformed-`:s` diagnostic code; whether the
trailing `/` in `s/RE/SUB/` is mandatory. Notes-only; no code yet.

### Session 2026-06-17 — [[ ]] design discussion → DD#62 + refactor task

Design discussion (no code) auditing `[[ ]]` operator semantics and the
duplicate test-path lowering. Captured as **DD#62**
(`test-expr-simplification`, candidate) + task `dd62-test-expr-ssavalue`.

**Decisions (DD#62):** (1) `==`/`!=`/`=` are exact-only — drop the
unquoted-RHS glob holdover; pattern-match via `=~`. (2) `=~` quoting is
grouping, not literalization (the bash quoted-regex-is-literal branch is
already dead/broken — quoted regex doesn't even parse, SB4014). (3)
Remove `-v` — sash has no unset state (undeclared scalar = zero value,
`unset` scalar rejected), so it's a misnamed alias for `-n` and broken
for arrays.

**Architectural payoff:** removing those eliminates `right_is_quoted`,
the only syntactic property of a *value* operand `SsaValue` can't carry.
Then `[[ ]]` value operands lower to `SsaValue`, subscripts use the SSA
path, and the duplicate `lower_subscript` is deleted — closing
`dynamic-map-subscript-in-test` (now superseded) by construction.

**Misses logged for the task's spec-first plan:** quoted `=~` is SB4014;
no way to put a space in a regex (quoted/unquoted/`[[:space:]]`/`\ ` all
broken); `a[[:space:]]b` mis-terminates at the internal `]]`;
`${var:s/re/repl/}` corrupts on a `/` in the regex/replacement (naive
`find`/`rfind`, no `\/`); `[[ 1 = 2 ]] && X || Y` drops `|| Y`.

**Open (DD#62):** the `\` meaning (engine-escape vs literalizer), `/`
delimiter escaping in `:s/`, and whether undeclared scalar reads should
be a compile error.

### Session 2026-06-17 (cont.) — DD#63 regex quoting/interpolation

Walking the regex-quoting design surfaced that **field interpolation is
unimplemented on every regex surface** — `$var` in a pattern/replacement
is taken literally (`[[ $x =~ $pat ]]` doesn't interpolate; `${pat}` →
SB4014; `${v:s/$x/Y/}` no-ops; mutation `v =~ s/$x/Y/` → `sash: bug:`).
DD#39's "dynamic patterns compiled at runtime" was aspirational; DD#39 is
also stale on `{n,m}` (counted repetition now works).

**DD#63 reframed to a builtin family** (`match`/`sub`/`gsub`) with the
three syntaxes (`[[ =~ ]]`, `s/re/sub/`, `${var:s/…}`) as **thin sugar**
that desugars to them. Because args are ordinary strings, quoting/
interpolation = normal argument evaluation — the bespoke regex-field
parser (and its open `\`/`/` questions) disappears. Decisions:
- `match SUBJ PAT` (boolean exit); `$(match …)` = whole match (`resub[0]`);
  `match ARR SUBJ PAT` fills `ARR=[whole,g1..g9]` (mapfile-style out-array).
- `sub`/`gsub` (awk) — `g` = the function choice, no flag arg. Engine has
  **no flags at all** (regcomp9 takes only a pattern; no ICASE) — case-
  insensitive unsupported, punted.
- Replacement backrefs `\0` (whole) / `\1..\9` (groups) — template subst
  from `resub`, *not* engine pattern-backrefs (Plan 9 still lacks those).
- `\xnn` (2 hex) in regcomp; `\x`+non-hex = error.
- Thin `s///` sugar: `/`-delimited, always interpolating; anything else →
  `$(sub|gsub …)`. `=~` RHS is just an argument (no special parsing).
- Literal containment → `fgrep`. The engine already captures submatches
  (10-slot `Resub`); `sash_regex_match` discards them today.

**Synergy:** `=~`-as-`match`-command means `=~` leaves the `[[ ]]`
TestExpr, simplifying the DD#62 value-operand→SsaValue refactor. Captured
in DD#63 + task `regex-interpolation-delimiters` (rewritten).

**DD#60 #1 (argv snapshot → out-param):** surfacing the array-last
out-param pattern (mapfile/read -a/`match S P arr`) prompted reworking the
argv snapshot from `array a=$(argv N)` (the lone command-sub→array case)
to `argv [N] a` (array last). Deletes the `ArgvSnapshot` command-sub→array
machinery; splat `$(argv)` for-in and scalars `$(arg N)`/`$(argc)`
unchanged. DD#60 revision note + task `argv-snapshot-to-outparam`.

**DD#64 (no shell backslash):** the backslash thread resolved to "shell
reads `\` only in `$'…'` and at EOL; verbatim everywhere else; escape via
quoting + concatenation (`'"'`, `'$'`); `\xNN` for literal regex bytes."
Removes the `\\`→`\` collapse / `"\\\\"` wart, `a\ b`≠`'a b'`, `\/`-broken,
dropped-standalone-`\`. Backslash *inside* `${var:OP}` operands deferred to
the var-modifier vet.

**DD#65 (prune `${var:OP}`):** vetted the family. Keep `:-`, `:off:len`,
`:s/`. Drop `:=` `:+` `:?` (broken/redundant), `#`/`##`/`%`/`%%` strip
(use `basename`/`dirname` — the community idiom), `/`-family glob-replace
(redundant with `sub`/`gsub`, glob holdover, only var-op using `\`), and
`^`/`,` first-char case. Move `^^`/`,,` → **`toupper`/`tolower` builtins**
(awk names, joining `match`/`sub`/`gsub`; gives case-insensitive matching
the engine's missing `i` flag can't). Open: `basename`/`dirname` builtin
vs external. Task `vet-var-modifiers` → DD#65.

### Session 2026-06-17 — map iteration: dynamic subscript + flat-literal + keys taint

DG: "fix the map iteration bugs." The filed `map-iteration-builtins-broken`
diagnosis (keys garbage / values ICE / keys escape) was **wrong**;
discussion with DG corrected it before coding. See
[journal 113](journal/113-map-iteration.md).

**Three real bugs, all fixed:**
- **(A) Flat map literal `map m=(a 1 b 2)`** was silently stored as a
  4-element indexed array. DG chose **reject** → new **SB4031** (DD#33),
  `validate.rs::reject_flat_map_literal`. (`[k]=v` and empty `()` OK.)
- **(B) Dynamic subscript was unimplemented.** The parser literalised the
  bracket index, so `${m[$k]}` was empty, `${a[$i]}` SB4030-false-rejected,
  and `a[$i]=v`/`m[$k]=v` writes mis-parsed as a command (`sash: bug:`).
  Read fix: `expr.rs::subscript_index_expr` expands `$var`/`${var}` at the
  three subscript sites (downstream already handled `SimpleExpansion`).
  Write fix: `parser.rs` recognises the split-token `name[$var]=` LHS.
- **(C) `keys` of a tainted unblessed map escaped taint** (bare-name arg).
  `validate.rs::check_collection_reader_taint` + `taint_for` cmdsub check
  → SB5020; `len` stays exempt.

The map-iteration idiom now works:
`for k in $(keys m); do echo "$k=${m[$k]}"; done`.

**Spec:** native **414 passing / 2 failing** of 416 (only pre-existing cf.c7/c8).
8 new tests (types.sh 33.5–33.10, taint.sh 60.35/60.36). Rust suite:
identical 167-failing set vs baseline — zero net regression.

**Filed (pre-existing, found here):** `subscript-append-broken` (P2,
`a[i]+=v` discards the element), `nonbrace-dynamic-subscript` (P3, `$a[$i]`
non-brace read — brace form works). `map-iteration-builtins-broken` marked
done with corrected diagnosis.

### Session 2026-06-16 — argv/arg/argc trio (DD#60) + two latent bugs

Implemented the full argv-trio chain (argv-trio-*, all 7 tasks done),
closing `bug-for-in-dollar-at-taint-ice` (top-level `for x in "$@"` ICE).
See [journal 112](journal/112-argv-trio.md).

**DD#60 locked + implemented** (supersedes the argv half of DD#42).
`$(argc)` = C argc (clean); `$(arg N)` = scalar 0-based tainted;
`$(argv)`/`$(argv N)` = tainted array splat (for-in / snapshot only);
`array a=$(argv N)` = tainted snapshot. Native + c backends both pass
`spec/argv.sh`. Validator gained SB5024 (top-level positional, DD#46),
SB5025 (splat misuse), SB5026 (quoted splat, DD#22), SB5027 (mixed
for-in), plus a bare-tainted-cmdsub use-site reject that closed a
**pre-existing getenv leak** (`echo $(getenv HOME)` printed `$HOME`).

**Two latent bugs fixed beyond the documented ICE:** inside-function
`$@` was splicing program argv (not the caller's args) — fixed by
expanding `$@`→`$1..$N` at inline time; and the getenv use-site leak.

**Spec:** native 402 passing / 2 failing (only pre-existing `cf.c7`/`cf.c8`
case-glob). Zero regressions in any mode; c improved 66→62 failing.

**Filed:** `emit-janet-argv-parity` (janet has no argv backend),
`rust-test-suite-rot` (mk test ~158-red at HEAD, pre-existing — argv-trio
added zero net rust regressions), `converter-positional-use-needs-bless`
(every top-level positional read needs a bless the converter can't author).

### Session 2026-06-17 — bless-shape negative-test audit

Per DG's rule (*every blessed spec shape needs a no-bless negative*),
audited all `bless` usages and closed **one more taint hole** plus filed
another. See [journal 112](journal/112-argv-trio.md) follow-up.

**Fixed:** `$(join @a)` / `$(join @arr)` over a tainted-but-unblessed
array escaped the CST taint pass (silent compile for stored arrays, ICE
for argv snapshots). `check_expr_taint` now recurses into a command
substitution's inner command args (`check_cmdsub_use_taint`), so any
tainted read inside `$(… @x …)` rejects SB5020 cleanly. `$(len a)` stays
exempt; blessed/clean joins still compile.

**Negatives added** (5): av.n6 (snapshot join no-bless), av.n7 (snapshot
subscript no-bless), 60.32 (mapfile-array iter no-bless), 60.33
(mapfile-array join no-bless), 60.34 (mapfile-map subscript no-bless).
Every blessed collection shape now paired.

**Filed:** `map-iteration-builtins-broken` (P1) — `keys` emits garbage on
a clean map, `values` ICEs on a clean map, and both leak a tainted map's
data; not spec-covered, needs a key-vs-value taint DD + a codegen fix.
_(Diagnosis later corrected — see 2026-06-17 map-iteration session below.)_

**Spec:** native 409 passing / 2 failing (only pre-existing cf.c7/cf.c8).
Zero regressions.

**Follow-up (same day):** a converter review surfaced a missed ICE —
`for x in $(argv); do echo "$x"; done` (unblessed) and `for x in @a` (a
tainted) ICE'd (the `av.s*`/`av.n*` tests masked it with body blesses).
Fixed in `validate.rs` `taint_for`: splat sources seed the loop var
tainted (body-bless valid, DD#60 Q5), tainted-array iteration is rejected
(DD#61 bless-all). Regression tests av.s5/av.n5.

### Session 2026-06-15 — array taint construction restriction + bless-all (DD#61)

DG reported `array a=( $(getenv HOME) $(getenv PATH) ); bless a [[ … ]];
echo $a[1]` compiling when it shouldn't. Two blind spots: the IR taint
pass didn't recurse into array/map/concat literals, and the SSA backstop
didn't check the array name on subscript reads. Both closed.

**DD#61 locked + implemented.** Tainted values may not enter an array via
literal / `+=` / `a[i]=` (SB5021) — tainted arrays come only from bulk
reads (read -a, mapfile). Whole-array taint is now exact. New bless-all
syntax: `bless x in a [[ $x … ]]` (indexed), `bless k v in m [[ … ]]`
(map, both validated). Scalar bless on a collection is a type error.
Full pipeline: parser → classify_bless → BlessAll/BlessRetag SSA →
Cranelift loop codegen. 31 new spec tests (taint.sh 60.*), all green
native; compile-time checks cross-mode, runtime guarded to native.

**Two coupled pre-existing bugs fixed:** `mapfile`/`read -a < file`
silently dropped the redirect (now routed through __redirect_push/pop);
`map m=()` built an indexed table (now respects the declared kind).

**type_check now runs for all backends** (moved ahead of the janet/c
emit branch) so bless-form errors fire everywhere. Migrated existing
scalar-bless-on-collection tests (54.1, 22.6/8/9/10) to bless-all.

Filed `oracle-blessall-mapfile` (P2): the c/janet oracles don't yet
cover bless-all + mapfile (pre-existing gap). DD#60 stays reserved for
the argv trio; vetting those tasks (join / `@a` spelling issues) next.

### Session 2026-06-07 — task hygiene sweep + cohort wave + DD#53 cruft

Swept 14 non-done tasks; 6 closed (5 done, 1 done-via-supersede), 1
redirected (monitoring-rules → skill-not-paragraph). Filed
cohort-complexity-cog-cc as the standing home for the ongoing
refactor work.

**Cohort wave: 28 commits, 28 violators cleared.** Live cog ceiling
fell from 59 (`emit_stmt`) to 16. Mean cog 3.82 → 2.94, so the
3× threshold ratcheted automatically twice (11.46 → 9.84 → 8.83).
The cohort surfaced 96 functions because the tightening keeps
pulling in moderate violators as the population improves.

**Pre-DD#53 cruft discovered in handle_cd.** While refactoring,
DG flagged that `cd -`, `cd HOME`, and `PWD`/`OLDPWD` autoupdate
violate DD#53 (env isolation). All three were silently active dead
code. handle_cd reduced to chdir + \$? only. See journal 109. A
companion task (tilde-vs-dd53) tracks the same problem in tilde
expansion, which the parser lowers to bare \$HOME and the validator
rejects with a misleading SB5018.

Open task count: 10 (1 active, 4 ready, 3 pending, 1 deferred, 1 todo).

### Recently landed (last ~2 weeks)

**Tree-sitter fully gone.** No `tree-sitter*` in `sash/Cargo.toml`,
no `tree_sitter` anywhere in `sash/src/`. The old `rule.rs`,
`rules/`, `taint.rs`, `validator.rs` files have been removed from
the tree, not just left dormant. Single-parser architecture via
logos (DD#59) + hand-written recursive descent in `sash/src/parse/`.
Validator and taint reimplemented as IR walkers in `validate.rs`
(~650 lines, formerly ~2575 lines across rules + taint).

**C oracle complete (6 phases, 67 tests).** `sash/src/emit_c.rs` +
`sash/tests/c_tests.rs` give a differential oracle against the
Cranelift backend. Both consume SSA. gcc-compiled C output is diffed
against bash. 25/30 janet_tests cases also reach parity via the C
path; the 5 remainders are legitimate divergence (3 sash-rejected
bash syntax, 2 sash extensions bash misinterprets). 38 sash-rt
runtime functions wired. Mirror-codegen architecture (emit_c
mimics codegen call-by-call) caught 6 bugs before runtime.

**Janet oracle stays on IR.** It's the IR-stage oracle; C is the
SSA-stage oracle. emit_janet joined the complexity-gate cohort
(the exemption was removed in the same change set that filed
add-c-oracle).

**Complexity gate enforced.** Commit `c51e296` introduced the
3× mean cyclomatic gate (currently cc > 16.8) with
grandfather-on-modify mechanic. The cog-vs-cc rubric in
[journal 108](journal/108-cog-cc-rubric.md) replaced the cyclo-allow
exemption table with a delta-aware framework. Phase 11 + 12
landed 16 cohort refactors using the rubric; live ceiling moved
from 49 → 26.

### Architecture (current)

- `sash/src/parse/` — logos lexer + hand-written recursive descent,
  produces IR directly
- `sash/src/lower/` — retired (was CST→IR)
- `sash/src/validate.rs` — IR walker, ~650 lines
- `sash/src/taint.rs` — removed (rolled into validate.rs)
- `sash/src/transform/` — IR → SSA
- `sash/src/codegen/` — SSA → Cranelift → static ELF
- `sash/src/emit_janet.rs` — IR oracle (1331 LOC, in cohort)
- `sash/src/emit_c.rs` — SSA oracle (mirrors codegen)
- `sash-rt/` — pure Rust `#![no_std]` runtime, rustix, no libc

### Remaining open tasks (9)

Active: `cohort-complexity-cog-cc` (standing — 15 grandfathered violators).
Ready: `brush-rerun`, `fuzz-validator`, `r47-validate-rs-per-rule-tests`.
Pending: `audit-string-keyed-builtin-dispatch` (36 sites),
`extract-instctx-setup` (15 sites in dispatch.rs),
`r47-promote-monitoring-rules` (redirected to skill design).
Deferred: `audit-run3` (schedule after next big feature wave).
Todo: `converter-compound-postfix` (SB402x in sash-convert),
`bang-precedence-and-suppress` (`! X && Y` should parse `(!X)&&Y`;
`- ! X` → SB4019; c/janet `!` parity — parser-precedence tail of DD#70's
`!`-must-consume work, whose native core shipped 2026-07-29),
`bug-emit-c-splat-vla-jump` (c-backend: external `@{a:?}` splat after a
preceding statement emits non-compiling C — goto into VLA scope; native ok).

