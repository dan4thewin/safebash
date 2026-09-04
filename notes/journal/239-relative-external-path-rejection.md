---
title: "Relative external paths rejected (DD#23; SB5051) + CST taint hardening"
date: 2026-09-03
---

# Relative external paths are rejected (DD#23)

DG: "look at _tmp/17, I'm getting 2 SSA taint backstop warnings." The file is a
`sash-getopt`-based arg parser; DG ran it as `PATH=.:$PATH ./sash 17`.

## The symptom vs. the root cause

Two SSA-taint-backstop rejects fired on `echo -n $ckd`, where `ckd` is the
capture of `if ! ckd=$(sash-getopt … )`. The backstop reject (rather than a
clean **SB5020**) is the "airbag" firing — its own doc says a reject there means
"the CST analyzer should have caught it first." So the backstop working is a
*symptom*; the real bug is upstream.

(The "2" is incidental: a called function's body appears twice in the SSA — a
dead `fn_` template that `entry` jumps past, plus the inlined copy — so any
backstop hit inside a called function is reported once per copy. Pre-existing,
generic, not the issue here.)

Root cause: with `.` first in PATH, `sash-getopt` resolved to the **relative**
path `./sash-getopt`, and two checks disagreed on it:

- **CST taint** (`command_name_is_taint_source`) tested `name.starts_with('/')`
  — absolute only — so the `./sash-getopt` capture was **not** tainted; `ckd`
  flowed to `echo` unflagged.
- **SSA backstop** (`cmd_subst_taint`) is fail-closed: `basename` → tainted
  unless a known clean builtin / user fn. It caught it.

Confirmed the gap is relative paths generally — a **source-written**
`$(./sash-getopt …)` is also backstop-only, while an **absolute** capture is a
clean SB5020. My first repro used `--no-taint-cst` (wrong — that bypass makes
backstop-only *expected*); DG corrected me, and the real invocation
(`PATH=.:$PATH`) exposed the relative-path handling.

## DG decision

Presented two coupled DD#23 violations (resolution emits a relative path; CST
taint misses relative externals) with fix options. DG chose **A — reject
relative external paths + tighten CST taint** (over absolutizing). This is the
cleanest reading of DD#23: *"resolved to absolute paths"*, *"sash_exec takes an
absolute path only … no slash is a compiler bug"*, security = *"no PATH
injection, no symlink tricks, no search ambiguity."* A relative path binds to
the runtime cwd — exactly that ambiguity.

## Fix (three changes)

- `resolve_command_path` (resolve.rs): **skip non-absolute PATH dirs.** `.`,
  `..`, and empty entries (= cwd) are ignored; a command found only via a
  relative dir is unresolved → SB5011.
- `resolve_command` (resolve.rs): a literal head that `contains('/')` but not
  `starts_with('/')` → **SB5051** ("relative external path … requires an
  absolute path"). Runs for every position resolution reaches (top-level,
  `$( )`, pipeline stages).
- `is_valid_command` (validate.rs): the pre-resolution structural check now
  **defers** relative heads (returns "valid") so it doesn't preempt with a
  generic SB5011 — `resolve` owns the uniform SB5051 diagnostic.
- `command_name_is_taint_source` (validate.rs): `starts_with('/')` →
  **`contains('/')`** (defense-in-depth). Post-rejection no relative path
  reaches it, but a slip would still be tainted; the backstop was already
  fail-closed.

SB5051 is a new code.

## Behavior

| input                                   | before            | after           |
|-----------------------------------------|-------------------|-----------------|
| `PATH=.:$PATH` bare `sash-getopt`       | resolves `./…`, backstop reject | **SB5011** (rel dir skipped) |
| `PATH=$PWD:$PATH` (absolute) capture    | backstop reject   | **SB5020** (clean taint) |
| `foo/bar baz` / `./tool -a` (top-level) | SB5011 "not found"| **SB5051**      |
| `x=$(./tool -a)` / `./tool a \| /bin/cat` | backstop reject | **SB5051**      |
| `/bin/echo ok`                          | ok                | ok (unchanged)  |
| bare unknown `nosuchcmd x`              | SB5011            | SB5011 (distinct)|

## Verification

- Full `SPEC_MODES="native c janet"` sweep, latest-run-per-test diff vs HEAD
  (`7170517`): **0 regressions**, 0 gone, **18 new passing** (relpath.1–6 ×3).
  errors.sh `plan` 98→104.
- Rust suites are benched/rot (linker GNU-stack warnings, removed
  heredoc/herestring features, missing `brush` binary). Stash-compared
  with/without my changes: taint_spec 25/25, e2e 47/47 failing — **0 new
  failures**.
- `cargo fmt` / `clippy` clean.

## Notes

- The dead-`fn_`-template double-report of the backstop is untouched (it only
  surfaces under `--no-taint-cst`, and both reports are true). Left as-is.
- DD#23 doc gained an addendum (2026-09-03) recording the rule + SB5051.
