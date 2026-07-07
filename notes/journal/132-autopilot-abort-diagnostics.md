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
