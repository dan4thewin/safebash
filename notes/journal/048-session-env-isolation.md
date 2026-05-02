# Session 2026-05-01 (late) — DD audit triage + DD#53 env var isolation

**Context:** Working through the 49 audit tasks from the 3-model DD
audit. While resolving export-related findings, DG identified a
fundamental design violation in the implementation.

## Audit tasks resolved

- **auto-bless-visibility** — resolved: auditors read `--emit-source`
  output (sash-native form), which includes explicit `bless` lines for
  all auto-blessed items. The finding misidentified the audit artifact.

- **dd01-09-export-crossref** — resolved: added forward reference from
  DD#01-09 item 4 → DD#45 export amendment.

- **dd45-export-amendment** — resolved: DD#45 note updated to document
  the export rejection amendment (SB5001) alongside the declaration
  axes amendment. The journal confirms both amendments belong to DD#45.

## Export and env var design violation discovered

While verifying the export rejection was correctly implemented, DG
asked whether the implementation actually matches the design. It doesn't.

**Three problems found:**

1. **Implicit env-var fallback in codegen.** When codegen encounters an
   unrecognized variable (e.g. `$HOME`), it falls through to
   `sash_getenv` (~line 1258). This silently mirrors env vars into sash
   vars, bypassing the taint/bless gate entirely. The entire point of
   `$(getenv NAME)` + `bless` is that env var access is explicit and
   auditable.

2. **`SsaInst::Export` fully implemented.** The transform emits Export
   instructions for `export` and `declare -x`, and codegen calls
   `sash_setenv`. The validator rejects `export` (SB5001), so this is
   dead code — but it maintains infrastructure for a feature sash
   explicitly does not have.

3. **Post-source re-read loop.** After `source` commands, codegen
   re-reads all env-backed variables via getenv (~line 1822). Same
   violation as #1.

**The design principle (clarified by DG):**

Sash variables and environment variables are completely separate
namespaces. There is no implicit mirroring in either direction:

- Reading an env var requires `$(getenv NAME)` — returns tainted data,
  must be blessed before use
- There is no mechanism to push sash var changes back to the env
- `FOO=bar cmd` is the only way to set env vars for a child process
- `$HOME` in sash source is a sash variable named HOME, NOT an implicit
  read of the HOME environment variable

This principle was implicit in DD#24 (getenv listed as taint source)
and DD#42 (getenv/argv as explicit-origin builtins) but never stated
as a standalone design decision. The implementation predates the taint
model and was never updated to match.

## DD#53 — Env var isolation

New design decision formalizing the separation principle. See
`notes/decisions/DD53-env-isolation.md`.

Cross-references:
- DD#24 updated: added forward reference to DD#53
- DD#42 updated: added forward reference to DD#53
- DD#45 unchanged (export rejection is about the keyword, not the
  namespace model)

## Implementation tasks created

- `notes/tasks/remove-implicit-env-fallback.md` — severity: security.
  Remove the codegen fallback that reads env vars for unrecognized
  variables. Remove the post-source re-read loop. After this, the only
  way to access env vars is `$(getenv NAME)` + `bless`.

- `notes/tasks/remove-export-ssa-dead-code.md` — severity: cleanup,
  depends on above. Remove `SsaInst::Export`, `sash_setenv` extern,
  transform Export emit, codegen Export arm, and the codegen tests
  (`export_basic`, `export_visible_to_child`) that test the dead path.

The `per-command-env-vars` task already exists for `FOO=bar cmd`.
