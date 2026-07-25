# 132 — autopilot: prioritization + abort-diagnostics-remaining

Running commentary for the unattended/autopilot session started
2026-07-07. Working the DG-ratified tier order (notes/status.md):
finish partials → compiler bugs → compiler features → c/janet parity →
convert. Skipping the 4 decision-gated (⚠) tasks.

## Prep

- Recorded the tier ordering (commit 4fc9221).
- Correction on entering execution: `dd55-abort-channel-remaining` is all
  c/janet (native landed 707832f) → moved to Tier 4; `dd67-bash-sugar-sweep`
  compiler part done, §3 is a convert canonicalize mode → Tier 6. So the
  real first native task is `abort-diagnostics-remaining`.

## Task: abort-diagnostics-remaining (Tier 1 #1, native)

Goal: every (A)-channel abort prints a `<argv0>: …` diagnostic (DG-ratified
scheme `<argv0>: <context>: <reason>`). W3 already shipped the syscall
aborts (cd/redirect/echo-write, 55.20-29) + overflow (via the wrap-bug
fix). Remaining classes: external nonzero, pipeline aggregate, cmdsub,
background-exec, regerror9 reprefix. Native-only specs until c/janet
parity (that is the separate Tier-4 work).

Key finding: external/pipeline/cmdsub messages are STATUS-based
(`<name>: command failed (exit N)`, no errno), so `sash_emsg` (errno→
strerror) doesn't fit — needs a status-formatting runtime helper.
bg-exec is errno-based (fits sash_emsg); regerror9 is a prefix change.

(Progress appended below as each class lands.)

## Class 1: external command message — DONE

`<argv0>: <name>: command failed (exit N)` on a bare external nonzero.
- Runtime: new `sash_emsg_status(name_ptr, name_len, status)` (sash-rt) —
  status-based sibling of `sash_emsg` (which is errno-based).
- Codegen: `AbortMsg` struct → enum `{ Errno{4 segs}, Status{name} }`;
  `emit_status_abort_check` matches → `sash_emsg` vs `sash_emsg_status`
  (passing the live `status`). `handle_general_command` now passes
  `Status{name}` (name is already interned by `intern_command_parts`).
  cd/echo updated to `Errno{…}`.
- Spec: 55.1 asserts the message suffix in native (`! in_modes native ||
  [[ $rerr == *"...command failed (exit 1)"* ]]`), one chk, cross-mode-safe.
- Verified: bare aborts w/ msg; `-`/`||` stay silent. 0 regressions
  native/c/janet (authoritative latest-run-per-test diff).

## Class 2+3: cmdsub + pipeline messages — DONE

- cmdsub (`x=$(/bin/false)`, 55.8): in the value_handlers abort block, emit
  `sash_emsg_status(name, status)` (name = `argv_values.first()` literal).
- pipeline (`/bin/false | /bin/true`, 55.10): `wait_for_children` now also
  tracks the failing stage's name (select alongside the worst-status
  select) and returns `(worst, name_ptr, name_len)`;
  `emit_pipeline_abort_check` emits the status message naming that stage.
  `handle_pipeline` materializes each stage name via `diag_static_seg`.
- Verified: both name the failing command + exit; `-`-suppressed stay
  silent; last-stage failure named correctly. 0 native regressions.

## Class 4: regerror9 prefix — DONE

Runtime `regerror9` reprefixed `sash: regexp error: …` → `<argv0>: regexp
error: …` (mirrors sash_emsg's argv0 logic). Verified behaviorally with a
runtime-invalid dynamic pattern (`[[ x =~ $p ]]`, p=`[` → `<argv0>: regexp
error: malformed '[]'`). No dedicated spec (a runtime regex-error test
needs env/argv plumbing; behaviorally verified).

## Class 5: bg-exec — status message already emitted; errno filed

Surprise: `/nonexistent &; wait` ALREADY prints `<argv0>: <name>: command
failed (exit 127)` — the bg child runs through `handle_general_command` →
`sash_run` (fork/exec/wait), so the external status message (§1) already
covers it. 55.16 now asserts that status message (native).

The gate wanted the *errno* form (`<name>: No such file or directory`).
Producing that cleanly is entangled: exec-FAILURE (missing binary, 127)
is indistinguishable from a process that ran and exited 127, at the
parent. Emitting the errno in the child (`sash_exec`/`sash_run`) would
double with the parent status message (fg/pipeline too). This needs a
design decision (how exec-failure should read across fg/bg/pipeline) →
filed `bg-exec-errno-message`. My initial `sash_exec` attempt was reverted
(wrong function + double-message).

## Status

Native abort diagnostics DONE for external / cmdsub / pipeline / regerror9
(+ overflow already). bg-exec has a status diagnostic; errno-precision
filed. c/janet parity is the separate `c-janet-dd55-abort-channel` /
`dd55-abort-channel-remaining` work. 0 regressions native/c/janet.

## Task: bug-echo-builtin-array-splat-ice (Tier 2 #1) — DONE

`echo @a` ICE'd at codegen (`values.rs:61`, ArraySplat lowered as scalar).
DG resolved (DD#69 §3, B): `echo` join-and-prints, so `@a` there is a
compile error → use `echo $(join @a)`.

Fix: new front-end **SB5030** in `validate::check_echo_splat` — a
`StmtKind::Command` arm in `check_argv_splat` flags a named-array splat
(`arrvals`/`arrkeys` ParameterExpansion) in an `echo` arg. `$(argv)` stays
SB5025 (used `is_named_array_splat`, excluding cmdsub, to avoid a double
diag). External splats untouched (`/bin/echo @a` still expands).
spec/types.sh 33.14-16; 0 regressions native/c/janet. Backstop ICE now
unreachable for echo.

## Task: tighten-cmdsub-taint-execve (Tier 2 #6) — DONE (via printf/pwd bug)

Started as a taint reclassification; DG diagnosed the root cause: `printf`
and `pwd` were **builtins** (`BuiltinCmd::Printf`/`Pwd`), so
`is_resolvable_external` skipped them → the front-end never resolved them
to abs-paths → CST taint saw bare `printf` (clean) while SSA basename-
classified it (tainted) → SSA ICE backstop. DG: "printf and pwd are
external; any builtin of printf/pwd is a bug."

Fix (native + janet; c already externals them):
- Removed `BuiltinCmd::Printf`/`Pwd` (enum + `from_name`), native
  `emit_pwd`, janet `emit_janet_printf` (+ its now-dead `interpret_printf_
  escapes`/`printf_format_literal`), and the `builtin_command_set` entries.
  `from_name`→`Other` → resolved to `/usr/bin/printf` / `/usr/bin/pwd`,
  exec'd like any external.
- `cmd_subst_taint`: `echo`→clean list; deleted the `echo|printf|pwd`
  pass-through (printf/pwd → tainted via the external default); dropped the
  now-unused `taint` param. CST twin (`cmdsub_is_taint_source`) unchanged —
  it already keys on the (now-resolved) abs-path.

`$(printf hi)` unblessed → SB5020 (front-end), no ICE. Also fixes the ICE
`bug-echo`-style mismatch for external cmdsubs. spec/commands.sh 14.2b
flipped clean→SB5020 (all modes) + 14.2c blessed positive (native). 0
regressions native/c/janet.

## Task: bug-test-operand-concat-splice (Tier 2 #7) — NOT A BUG

The repro `suf=ing; x=running; [[ $x == run$suf ]]` expected Y, but
`run`+`ing` = `runing` ≠ `running`, so N is correct — **bash returns N
too** (oracle-verified). The splice concat works: `suf=ing; x=runing;
[[ $x == run$suf ]]` → Y (and `=~` matches). `expr_to_value(Concat)` →
`concat_to_value` builds it fine. Added spec/operand_words.sh ow.11 (`==`
splice), ow.12 (`=~` splice), ow.13 (negative), passing all modes. Closed
as not-a-bug.

## Task: tilde-vs-dd53 (Tier 2 #8) — compiler half DONE

`echo ~` / `echo ~/x` gave the misleading `$HOME` SB5018 (parse_word_arm
lowered `~`→SimpleExpansion("HOME")). Now `parse_word_arm` returns
`Result` and rejects unquoted leading `~`/`~/path` with a focused
**SB4036** directing to `$(getenv HOME)` (option (b) — parse-time
diagnostic). Quoted (`'~'`/`"~"`) and mid-word (`a~b`) tildes untouched.
spec/taint.sh tilde.1-3, all modes; 0 regressions. Converter rewrite half
(bash `~` → `$(getenv HOME)`+bless) deferred to Tier 6 (convert); task now
`partial`.

## Tier 2 (native compiler bugs) — COMPLETE
echo-splat ICE (SB5030), tighten-cmdsub-taint (printf/pwd external),
concat-splice (not-a-bug), tilde (SB4036 compiler half). Tier 3 next.

## Task: implement-getpid-builtins (Tier 1 #2) — DONE (native+c)

DG decision: `$(random)` = non-negative 63-bit i64 (0..i64::MAX) from
getrandom(2) (rustix `rand` feature — same CSPRNG as /dev/urandom, cleaner
than an explicit device open). Added `sash_random` (sign bit cleared).
Wired the two missing arms: `getpid` (native value_handlers — c already had
it) and `random` (native + c). All getpid-class intrinsics now work as
clean in-process builtins in the oracles.

spec/functions.sh 42.7 (getpid numeric + clean capture, no bless), 42.8
(random non-negative), 42.9 (two draws differ) — guarded native,c. 0
regressions.

Discovered janet execs the WHOLE getpid-class as fake commands
(`(os/spawn ["getppid"])` etc. — getenv/argc too), a pre-existing gap →
filed `janet-getpid-class-intrinsics` (Tier 4). **Tier 1 COMPLETE.**

## Decision #4: bg-exec-errno-message — RESOLVED + DONE

DG's model: split the exec/fork diagnostic by who owns the syscall.
- child (in execve) always reports `<argv0>: <cmd>: <errno>` at the point
  of failure, then exit 127 (the *what*).
- parent reports `<argv0>: <cmd>: <errno>` on a fork(2) failure.
- parent's DD#55 `command failed (exit N)` is unchanged — already gated on
  UNCAUGHT (emit_status_abort_check's abort flag), so it's the
  *why-aborting* signal, not a duplicate. DG: "why should the parent report
  the 127 unless it is an uncaught situation?"

No CLOEXEC pipe needed. Behavior: caught exec-failure → child errno only;
uncaught → child errno + uncaught-abort line; ran-and-exited-nonzero →
command-failed only.

Impl (sash-rt): sash_exec reports the exec errno (rustix execve returns
Errno directly on failure — the earlier E0308 was treating it as Result);
sash_fork preserves errno via set_last_errno; sash_run's fork-failure arm
reports it. spec/errors.sh 55.16 (bg errno), 55.30 (fg uncaught both
lines), 55.31 (caught → errno only). 0 regressions.

## Tier 3 #9: reject-undeclared-scalar-read — DONE (DD#62 Model B)

DG pivoted the design mid-implementation. The original task was a
definite-assignment (must-analysis, intersection-at-joins) escalation so a
read on a path where the var might be unset was rejected. DG: "the footgun
is the misspelled variable... in Go the zero value is preferably meaningful
— we've boxed ourselves into a corner where the zero value is useless."

**Model B** (amends DD#62): scalar zero values ARE meaningful (untyped→"",
int→0 — the runtime already produces these). Reject ONLY a scalar read of a
name introduced NOWHERE in scope — a pure typo/scope-membership check. This
is a single-pass may/union analysis, NOT dataflow: `if c; then x=1; fi;
echo $x` COMPILES. `${x:-WORD}` is the explicit opt-out.

Impl: `check_undeclared_reads` pass in validate.rs (SB5031), wired into
run_semantic_checks. Known-set = per-scope introductions (assignment LHS,
int/array/map/const decl, for-var, read/mapfile/readarray targets) ∪
globals. Functions inherit enclosing globals; a name introduced only in a
sibling function is unknown. Reads = SimpleExpansion + bare
ParameterExpansion (operator present ⇒ opt-out). Specials/positionals
auto-excluded (is_plain_identifier); ENV/REJECTED keep their own codes;
$LINENO parse-lowers to a Literal so it's invisible anyway.

Two subtleties handled:
- **bless binder**: `bless ARR in VAR [[…$VAR…]]` introduces predicate-local
  binder(s) (Indexed.binder, Map.key+val via classify_bless) — added to
  known when checking the test, and the binder/array literals aren't treated
  as reads. (Over-fired on 15 native specs before this fix.)
- **command substitution**: recurse into `$(…)` bodies with SUBSHELL scope
  (outer known ∪ the sub's own introductions, which don't leak out). Catches
  `echo $(echo $typo)`; allows `echo $(x=1; echo $x)`.

Specs: spec/undeclared.sh ud.1-16, all 3 modes, 0 regressions.

**Two bugs surfaced by the new specs (filed, not worked around):**
- `janet-scalar-zero-value`: janet emits `(var n )` for `int n` (no
  zero-init) and `(or (os/getenv "x") "")` for a bare scalar read
  (env-fallback — violates DD#53). Guarded ud.5/6/11 to native,c.
- `bug-multistatement-cmdsub-placeholder`: a `$(…)` body of >1 statement is
  never executed — `single_command_subst` bails at len!=1 and the value
  falls through to `expr_to_string`'s debug `"$(...)"` placeholder, which
  leaks to runtime output. Single-statement works. Decision (DG): implement
  vs reject. Guarded ud.16 to native + assert-no-SB5031-only.

## Tier 3 #10: regex-interpolation-delimiters — DONE (DD#63 D/E/G)

Prior DD#63 work (builtins match/sub/gsub, captures, backrefs — checkpoint
007) was done. Remaining delta this task: interpolation (D7/D8), `\xnn`
(E1/E2), and `${var:s}` Path B brace/delimiter handling (G group).

**Root causes (mapped via explore agent):**
- Both sugars flattened fields to flat `Literal` strings at parse time
  (`\0`-packed for `${var:s}`; a single `s/…` word for `=~`), so `$x` in a
  field never interpolated. `=~ s/$x/Y/` even miscompiled (the lexer split
  `$x` into its own token; only `s/` was captured → "bare command passed to
  sash_exec").
- The vendored engine had no `\xnn`.
- The lexer's `scan_braced_content` counted braces only, so `${v:s/a}b/z/}`
  truncated at the first `}`.

**Fixes:**
1. `\xnn` in `sash-rt/csrc/regcomp.c::nextc` (+ `hexdigit`): two hex digits
   → one byte; short/non-hex → `rcerror`. (E1/E2, G6.)
2. Lexer Path B: `scan_braced_content` delegates to
   `try_scan_sed_braced_content` when the body opens `NAME:s/` — scans the
   three `/`-delimited fields by slash count (braces literal until the 3rd
   `/`), closing `}` only after. Unterminated (a `"`/newline/EOF before the
   close) stops at the boundary → parser flags SB4032. (G1/G2/G3/G5/G8.)
3. Re-route at parse: `try_sed_op` and `parse_regex_mutation` both build
   `$(sub|gsub "$var" RE SUB)` with RE/SUB via `parse_default_word` (the
   DD#65 interpolating-word parser). Unifies on the working RegexReplace
   path; interpolation falls out (D7/D8). `=~` reconstructs the raw `s/…`
   from adjacent tokens (incl. `$x`/`${x}`).
4. Malformed → SB4032: the `=~` path (Result-returning) errors directly; the
   `${var:s}` value path (can't raise a parse diagnostic) emits an
   `s_malformed` ParameterExpansion sentinel that a new fatal validate pass
   (`check_malformed_sed`, wired into run_structural_checks) rejects.
5. Compile-time regex validation (DD#39, for 39.5) kept on the `=~` static
   pattern only (matches old asymmetry: `${var:s}` never validated).

The old `${var:s}` (`ParameterExpansion "/"`/`"//"`) and `=~`
(`regex_replace` ParamExpansion) transform+codegen are now runtime-
unreachable but still compiled (no dead-code warning). Filed
retire-legacy-sed-regex-replace-machinery to remove them.

**Decision surfaced + filed (decide-regex-bare-brace):** the engine
deliberately requires `\{` for a literal brace (bare `{` starts an
interval; errors otherwise: "use \{ for literal"). `}` is already literal
(so G1 works). The task gate G3 assumed bare `{` literal (bash-compat) —
adjusted 63.g3 to the engine-correct `\{` form (guarded native,c) and filed
the bash-compat-vs-`\{` decision for DG. Path B itself is verified for the
`{` byte.

Specs: spec/regex.sh 63.d7/d8/e1/e2/g1-g8 (native 40/40; c/janet skip only
mode-guarded cases). 0 regressions all modes. Refactored the new lexer/
parser/validate fns under the complexity gate.

## Tier 3 #11: dd70-error-model-contract-spec — DONE

Most of the DD#70 (A)/(B) contract was already implemented+tested (55.x
abort/catch/suppress+diagnostics; 63.1c-2f match must-consume; abort
diagnostics from Tier 1). This task filled the gaps and consolidated the
contract into spec/errors.sh.

**Feature gap closed — bare `[[ … ]]` is a (B) must-consume boolean.**
`check_match_must_consume` → `check_boolean_must_consume`, generalized to
also fire for `BuiltinCmd::Test` (a bare `[[ … ]]` is `Command{name:"[[",
args:[Test]}`). Bare → SB4018; `- [[ … ]]` → SB4019; consumed positions
(if/while/&&/||, `|| :`) are fine via the existing `state.in_condition`
gate. Verified safe against every existing `[[ ]]` spec:
- `bless x [[ … ]]` predicates are `BuiltinCmd::Bless`, not Test.
- `[[ … { } ]]` dispatch (DD#66) is a separate control-flow statement, not
  a Test-arg Command (dp.a4/a5 still compile).
- for-in synthetic `[[ __ai -lt … ]]` bound checks are lowered directly,
  not via handle_command.
- `! [[ … ]]` is `NegateTestResult`, not Test.
- ow.7 (`[[ $x =~ foo bar ]]`) already errors at parse (SB4014).
0 regressions across all modes.

**Contract tests (spec/errors.sh):** e.b70.tbare (SB4018), e.b70.tdash
(SB4019), e.b70.tdisc (`|| :`), e.b70.tand, e.b70.tor, e.b70.fndisc
(`-> int` via `|| :`), e.a70.silent (caught → no rerr diagnostic),
e.a70.regcomp (dynamic bad regex → runtime `regexp error`).

**Native-only guards + gap filed:** the c/janet spec harnesses run the
binary with `2>/dev/null` and never populate `rerr`, so any rerr-asserting
test (silent, regcomp) is native-only. e.b70.fndisc is native-only too:
`fn f -> int { return 3 }; f || :` fails to compile in c (gcc rejects the
emitted C) and errors in janet — a genuine backend gap, filed
c-janet-fn-int-discard.

## Tier 3 #12/#13: glob-splat disposition — Part 1 done, Part 2 scoped

Investigated glob-as-array-splat-disposition (#12) and found **Part 1 is
already implemented**: `for f in *` iterates per-element and inline `*` in a
command arg is SB5029. Set #12 → partial.

Part 2 (external-splat empty-disposition `@{a:-WORD}`/`@{a:?WORD}` + bare-@a-
external → SB5032) is the same work as the focused task
glob-splat-disposition-word (#13). It's a full-stack, new-syntax feature:
`@{…}` needs new lexer tokenization (today `@{a:-foo}` mis-parses as `@` + a
`{ }` command group), then parser/validate/transform/codegen/runtime. The
design is DG-resolved (2026-06-21) — no blocker. Mapped the full
implementation surface (via an explore agent) and appended a concrete,
anchor-by-anchor implementation-ready plan to #13 so it can be executed
directly next. SB5032 reserved for the bare-@a-external error.

## Tier 3 #14: argv-snapshot-to-outparam — DONE (DD#60 revision)

Moved the argv snapshot from `array a=$(argv N)` (the only cmdsub→array
special case) to the out-param builtin `argv [N] a` (trailing array, like
mapfile / read -a / match S P arr).

- **ssa.rs**: `BuiltinCmd::Argv` + `SsaInst::ArgvFill { array, start }`
  (mirrors Mapfile/MatchInto); removed `SsaValue::ArgvSnapshot`.
- **transform** (stmt_handlers `try_handle_argv`): `argv a` (start 0) /
  `argv N a` (start N); trailing positional is the array. **Auto-declare**
  (DG resolved): if `a` isn't already a collection, emit an empty
  `IndexedArray` Assign first — this reserves the *versioned* slot `a_0`
  that `find_array_slot` resolves. (Discovered the ICE root cause: the
  setup slot-scan inserts the plain name `a`, but reads use `a_0`; without
  a decl/assign there's no `a_0`. mapfile has the same latent ICE on
  `len` of an undeclared target — filed nothing yet, argv now does it
  right.) Removed the old `argv_snapshot_start` cmdsub detection.
- **type_check** (`check_command`): register `argv`'s trailing array as a
  collection so bless / len / iteration see an array, not a scalar.
- **codegen**: `handle_argv_fill` (call `sash_argv_snapshot(start)`, store
  the table into the array slot); dispatch arm.
- **validate**: `taint_read_target` extended to `argv` (fills a tainted
  array, DD#61 → bless-all before read); `check_argv_splat_decl` now
  REJECTS `array a=$(argv …)` (SB5025, directing to `argv [N] a`) instead
  of exempting it.
- **c/janet parity**: `emit_argv_fill_c` (SSA-based inst) and
  `emit_janet_argv` (IR-based command; `(array/slice (dyn :args) …)`).

spec/argv.sh migrated (av.n1-7, av.x5 → `argv [N] a`) + av.rej1/2 (old form
rejected) + av.rej3 (slice past end → empty). native/c/janet all 37 pass;
0 regressions. Tier 3 (all native compiler features) COMPLETE.

## Tier 3 #13: glob-splat-disposition-word — DONE (DD#69 Part 2)

External-splat empty-disposition. A bare `@a` into an external command is
now SB5032; it must carry `@{a:-WORD}` (empty → the single arg WORD; bare
`@{a:-}` → zero args) or `@{a:?WORD}` (empty → abort, exit 1,
`<progname>: WORD`). Only `:-`/`:?` (else SB5033). WORD is a `${x:-WORD}`-
style interpolating word, substituted as ONE element. Builtins, user
functions, intrinsics (join/len/…), and for-in are exempt.

Full stack:
- **Lexer** (`scan_at_brace`): `@{…}` scans into one Word (brace-depth /
  DD#65, like `${…}`) — `@` alone stays a normal word.
- **Parser** (`parse_at_brace_disposition`): decode `@{name:OP WORD}`;
  `:-`→`arrvals_default`, `:?`→`arrvals_abort`; WORD via `parse_default_word`
  (empty WORD → operand None → zero args); malformed → SB5033.
- **SsaValue::ArraySplat** gained a `disposition {word, has_word, abort}`;
  `arg_to_value`/`splat_disposition` build it.
- **Runtime** `sash_splat_disposition(tbl, word, wl, has_word, abort)`:
  non-empty → tbl; empty → abort (`<argv0>: WORD`, exit 1) / 1-elem [WORD] /
  empty. Applied by all three argv builders (native `resolve_splat_tables` +
  `emit_splat_disposition`, c `cache_splat_tables_c` +
  `emit_splat_disposition_c`, janet `janet_splat_splice` inline).
- **Validate** (`check_external_splat`): a bare `@a` (operator `arrvals`, no
  disposition) in a `/`-prefixed (resolved-external) command arg → SB5032.
  Scoped by the resolved `/`-prefix (post-DD#23), which precisely excludes
  builtins/functions/intrinsics — `is_external_source` over-classifies bare
  non-builtins (e.g. `join`), so it is NOT used here.

**Key finding — `echo @a` (SB5030) vs the disposition:** `echo` is a builtin
so it's exempt from SB5032, but still SB5030 (DD#22 flatten). gs.7 pins the
external-only scoping. **Migrated** wordsplit 22.14-19 (bare `@a` to
`/usr/bin/printf`/`cat`) to dispositions — DD#69 has no static-non-empty
exemption. spec/glob_splat.sh gs.1-13 native/c/janet (gs.6 abort native-only,
rerr); 0 regressions all modes. Tier 3 (all native compiler features) is now
fully complete.

## 2026-07-14 — Tier 4 unattended: c/janet lastpipe read/mapfile (dc10423)

First Tier 4 (c/janet parity) task under the unattended SOP. Fresh
multi-mode sweep to map failures: native **0** (gate green), c **47**,
janet **70**. Biggest *shared* cluster was 22.4-10 (wordsplit read/mapfile
as a pipeline last stage) — task `echo-read-inline-pipe-cjanet`.

**Root causes**
- **c**: `emit_pipeline` ignored `capture_read_fd`. For `echo|read` the
  producer Pipeline has 1 stage → the `n==1` degenerate path emitted a plain
  command (prints, then) `ReadLine{fd:-1}` read literal fd -1 → rustix
  `fd != u32::MAX` assertion. `ReadLineMulti`/`Mapfile`/`ReadArray` were TODO
  no-ops entirely.
- **janet**: `emit_pipeline` os/spawns every stage → `read`/`mapfile` became
  `(os/spawn ["read" …])` "not found". *Coupled* pre-existing bug: bare
  `array a;`/`map m;` → `declare_decl_var` inferred map-ness from the
  *initializer* (`AssocArray`), so a bare (empty-literal) decl fell through
  to a plain string binding → `(var m "")` and `${m[key]}` used
  `(scan-number key)` (integer path). Fixed to read the `-a`/`-A` decl flag.

**Fix** (native is the oracle; mirror its structure)
- c `emit_pipeline`: capture path creates `n` pipes (extra last→parent),
  parent keeps the final read end in `__pipe_read_fd`; ReadLine/Multi/Mapfile/
  ReadArray at fd=-1 consume it. Reused sash_split_first_word/field/
  into_array + sash_read_all runtime primitives.
- janet `emit_pipeline`: `detect_lastpipe` → run producers as os/spawn,
  `(:read (p_last :out) :all)`, bind vars in-process (single read, `-f`
  multi, mapfile `-t` array, `-m` map). Bare decl emits `@[]`/`@{}` + map
  registration.

**Result**: c 47→**38**, janet 70→**61**, native **0** unchanged, **0
regressions**. Fixed 22.4-10, 22.12 (un-guarded), + collateral 20.3 (another
lastpipe read) and 54.1 (mapfile-m into map). Complexity gate: refactored 6
flagged functions (cog 8-15) by extracting helpers / flattening to else-if —
all green, no bypass.

**Process notes**
- spec.db is native-only; c/janet results are computed at run time. Map them
  with a `SPEC_TSV` sweep + awk on `mode:id` — do not rely on run.sh's tally.
- Next biggest shared clusters: 58.x (int-func-in-`$((f))`: c dup-`__ptr`,
  janet assigns the fn instead of calling), 36.x (dtoi), 55.x (DD#55 abort),
  56.x (signals, mostly janet).

## 2026-07-14 (cont.) — c/janet hardening + DD#58 int-func-in-arith

Two more commits after the lastpipe work.

**e7c530f — distinct special-var C names + enforce transform diags.**
Investigating the 58.x cluster surfaced a c codegen bug: `c_var_name`
collapsed every non-alphanumeric char to `_`, so `$#`/`$?`/`$@`/`$*`/`$!`/
`$$` all became the same C identifier (`__ptr`) → duplicate declaration
(compile error) + variable aliasing. Fixed: each special char → a distinct
`_c<code>` token. That in turn unmasked a latent gap: `--emit-c`/`--janet`/
`--manifest` called `ir_to_ssa_typed` and *discarded* its diagnostics, so
they emitted (and ran) programs native rejects (SB4016 etc.). native
enforces these via codegen::compile_to_object; added `enforce_transform_diags`
so the alt-output backends reject the same programs. (55.13 had been passing
in c only because the dup-decl made the bad C fail to compile.)
→ c 38→31, janet 61→53; fixed compile-error specs 0.6/46.4/48.1/55.12/13/14
  + 58.6/58.7. 0 regressions.

**73de6d2 — DD#58 int-func-in-arith c + janet.**
- c: `return N` in a lifted int-func lowers to `Assign(?, N)`; emit_assign
  wrote `?` to the generic string slot but `$?` reads come from
  __exit_status → capture saw 0. Now Assign to `?` sets __exit_status (int),
  mirroring native's handle_exit_status_assign. → c 58.1-8 all pass.
- janet: (1) arith `$((f))` for a known int-func emits `(f)` not bare `f`
  (threaded ctx through arith_expr_to_janet, split into two dispatchers +
  helpers for the complexity gate); (2) int-func bodies wrap in `(label _ret
  …)` and `return N` → `(return _ret <numeric>)` (bare janet return needs a
  label; numeric for arith). → janet 58.1/4/5/8 pass.
→ c 31→22, janet 53→50; 0 regressions.

**Session tally: c 47→22, janet 70→50, native 0 throughout.**

Remaining janet 58.2/58.3 need arith string→int coercion of function args
(`return $(($1+$2))` with `(add 3 4)` numeric args vs the body's
scan-number) — folded into `janet-arith-string-coercion`. Next shared
clusters: 36.x (dtoi), 55.x (DD#55 abort), 56.x (signals, mostly janet).

## 2026-07-14 (cont.) — dtoi/itod (36.x) + janet arith coercion

**28948e7 — dtoi/itod fixed-point intrinsics (c+janet).** c added
sash_dtoi/sash_itod calls in try_native_intrinsic_c (were TODO); janet added
cmdsub_num_intrinsic (float-based, illustration). → c 22→18, janet 50→46.
Fixes 36.1-36.4 both modes.

**28db76d — janet arith string→int coercion (`_num`).** sash vars hold
strings; arith needs ints. `_num` coerces operands (var/`$x`/`$1`/compound-
assign read/inc-dec), DD#72-aware (0x via scan-number, 0o/0b via janet
radix, leading-zero decimals base-10). → janet 46→41. Fixes 58.2/58.3,
cf.f5, cf.f8, 16.2; un-guards nl.s1-s10. Closed janet-arith-string-coercion.

Remaining janet cf gaps (cf.f9 multi-init scoping, cf.f10 loop-var
persistence, cf.f12 `$@` iteration) are distinct control-flow issues (task
cf-backend-gaps-brace-surfaced), NOT arith.

**Session totals: c 47→18, janet 70→41, native 0 (gate green throughout),
6 code commits, 0 regressions.**

## 2026-07-14 (cont.) — unset collections + read-herestring; session wrap

**a355d38 — unset array/map (DD#30) c+janet.** c: emit_assign now handles
the `__new_table__` sentinel (→ sash_table_new); janet: `${#a[@]}` nil-safe
via `(length (or a ""))`. → 30.11 both. (Also: the earlier "c array-length
segfault" was a red herring — my manual gcc dropped `-fno-stack-protector`;
the harness flag is correct.)

**3511fe4 — read from herestring + multi-part DoubleQuoted.** c: emit_readline
now applies its redirects (was ignoring them → read from inherited stdin);
ir_expr_as_str concatenates multi-part DoubleQuoted (a herestring body
`"hello"` parses as multi-part, so it had emitted empty via the TODO — broke
`cat <<< "word"` in c generally). janet: expr_to_literal same multi-part
generalization. → 24.6 both.

## SESSION SUMMARY (Tier 4 c/janet parity, 2026-07-14, unattended)

8 code commits, **native 0 throughout (gate green), 0 regressions**:
| commit  | area                                   | c        | janet    |
|---------|----------------------------------------|----------|----------|
| dc10423 | DD#20 lastpipe read/mapfile + bare decl| 47→38    | 70→61    |
| e7c530f | c_var_name distinct + enforce diags    | 38→31    | 61→53    |
| 73de6d2 | DD#58 int-func-in-arith                 | 31→22    | 53→50    |
| 28948e7 | dtoi/itod intrinsics                    | 22→18    | 50→46    |
| 28db76d | janet arith string→int coercion (_num) | 18       | 46→41    |
| a355d38 | unset collection (DD#30)               | 18→17    | 41→40    |
| 3511fe4 | read-herestring + DoubleQuoted         | 17→16    | 40→39    |

**Net: c 47→16 (−31), janet 70→39 (−31).**

Tasks closed: echo-read-inline-pipe-cjanet, janet-arith-string-coercion.

**Remaining (all feature-sized, next session):**
- 56.x signals/END (c 7, janet 15) — biggest; needs the `signal` builtin
  (IGN/DFL/FLAG/EXIT dispositions), `$(signum)`, END-block integration.
  janet lacks END entirely.
- 55.x DD#55 abort remainder — needs: (a) c external `$(cmd)` cmdsub
  (emit_cmdsub is TODO for externals) → 55.7/8; (b) pipefail with
  per-segment `-` suppress (55.10/11, 20.6, 21.3) — the pipeline stage
  tuples carry no suppress flag, and a `-`-suppressed pipeline sets
  checked=false but keeps a nonzero status, so naive pipefail regresses
  55.11; DG-design-sensitive; (c) `$!`/StoreBgPid background pid (38.5,
  55.19).
- janet singletons: cf.f9 (C-style multi-init scoping — vars declared in a
  nested `(do)` don't escape to the loop), cf.f10 (loop var persistence),
  cf.f12 (`for x in $@`), 14.3 (wait), 16.1, 17.1, 18.1-3, 33.10 (`$(keys)`
  + `${m[$k]}`), 42.1, 48.2, 53.2 — heterogeneous.
- c singletons: 24.12 (bless runtime-fail exit).

## 2026-07-24 — DD#55 pipeline semantics pass (DG-directed, commit 845f494)

DG chose to fix the SEMANTICS (not just the c/janet representation) after the
investigation surfaced that `-` per-segment suppression was never represented
as data — it was faked two wrong ways (leading `-` → whole-pipeline
checked=false; non-first `-` → drop the stage), and an undocumented
`i == 0 && !checked` skip in native's wait_for_children conflated leading-`-`
with condition context. One root cause, four bugs.

**Ruling (DG):** honor DD#55 as written — a pipeline's truth value is ONE
definition everywhere (worst over non-suppressed segments = pipefail);
position decides only the consequence (abort vs branch). And `pipeline &&/||
C` is ambiguous postfix → reject (SB4023, same class as &&/|| on a compound;
DG: reuse the code). All four fixed together.

**Key discovery during impl:** sash parses `a | b && c` as `Pipeline[a,
List(b && c)]` — the recursive stage parse ABSORBS the `&&` into the last
stage as a subshell (non-standard precedence). So the SB4023 reject fires on
a `List(And|Or)` last stage, not on a trailing `&&` at the pipeline level.
And leading `- a | b` parses as `Suppressed(Pipeline)`, non-leading as
`Pipeline[.., Suppressed(..)]` — the asymmetry behind bugs #2/#3. Fixed by
normalizing `Suppressed(Pipeline[s0,..])` → `Pipeline[Suppressed(s0),..]` at
parse time, so `-` is uniformly per-segment.

**Representation:** per-segment `suppressed: Vec<bool>` on SSA `Pipeline`
(native codegen + emit_c read it; emit_janet derives it from the AST).
Deleted native's `i == 0 && !checked` skip. emit_janet got a real pipeline
rewrite: `emit_pipeline_do` runs every stage (unwrapping Suppressed),
aggregates over non-suppressed, passes output through verbatim (`prin`, no
trim — fixes a spurious blank line on empty output), and pipeline conditions
emit `(zero? …)`.

native 0/792→801 (9 new specs, gate green); c 16→13, janet 39→37; 0
regressions. Rust integration suites unchanged vs HEAD (their failures are
the pre-existing stale-`declare`/`$HOME` tests — audit-rust-test-suites).
