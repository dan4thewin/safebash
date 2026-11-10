title: "Provide informative default :? abort diagnostics"
status: open
priority: P3
kind: work
depends: []
gates:
  - "Omitted and explicitly empty :? messages identify the source-level subject and applicable failure in native and C, rather than only printing the program prefix"
  - "Scalar/cell word checks, arithmetic string conversions and array-splat checks follow the same default-message policy while retaining their distinct failure triggers"
  - "Nonempty custom literal text, DD65 quoting and SB4070 refusals remain unchanged"
  - "Successful operations, abort status, DD128 handler/process routing, evaluation count, taint and placement restrictions are preserved"
  - "New regressions fail on the pre-change binary or an independent reversion; the normal gate passes with predicted IDs and totals"

## Decision

DG, 2026-11-10:

> yes to better default

The proposal was to generate a diagnostic naming the subject and explaining
the applicable failure when a `:?` message is omitted or explicitly empty.
Nonempty custom literal messages keep their text. Requiring a nonempty
message at every failsafe was the alternative, not the selected policy.

DD77 is authoritative, with DD65's literal-message and DD69's splat
contracts pointing to it. This task owns R3 from `q-f2-residuals`; that
mixed question record stays open for its other residuals.

## Required behavior

Apply the default uniformly to every otherwise-legal spelling and subject:
scalar/cell word checks explain emptiness, array splats explain an empty
collection, and arithmetic string conversions explain failure to supply a
valid representable integer. Non-numeric or out-of-range input need not be
empty. Source-level names must not become compiler-generated binding names.

Message emptiness is after existing literal parsing, so an explicitly
empty quoted literal also selects the default. Do not trim or replace a
nonempty custom literal, expand it, or change the `:-`/`:~` dispositions.
The diagnostic must not reevaluate a subject or subscript. Preserve
successful values, existing abort triggers/status, DD128 handler/process
ownership, and the independent typing, taint and placement rules.

Exact wording, subject rendering and the implementation approach are
deferred until this task is undertaken. No runtime ABI, metadata format,
diagnostic code or extra source-level syntax is selected here.

## Evidence and implementation entry points

F2's historical `str s=""; echo "${s:?}"` probe reported only
`./test_bin: ` on stderr before aborting. Its evidence is retained in
`files/port-f2-probes.tar.gz` and `q-f2-residuals` R3; it was not rerun
for this recording.

Current source inspected at `ba49c22c`:

- `transform/expressions.rs:316-319` carries the supplied message into
  `RequireValue`; `emit_c.rs:3083-3098` passes it to the runtime.
- `sash-rt/src/lib.rs:1231-1248` (`sash_require_value`) writes the program
  prefix and newline without a reason when message length is zero.
- `sash_atoi_or_abort` at `1776-1789` uses that same path for invalid
  integer input; native emission at `codegen/values.rs:1294-1315` passes
  the message through.
- `sash_splat_disposition` at `1153-1176` separately has the same
  empty-message behavior.

Compiler paths above are under `sash/src/`. Reinspect their current forms
when implementing; these are entry points, not a chosen design.

Use the real CLI and existing spec files. Cover omitted/explicitly empty
messages, the distinct failure reasons, source-name restoration, a preserved
nonempty custom message and successful-path controls. Assert runtime stderr
(`rerr`), not compiler stderr. Choose final wording and assertions together;
do not treat a generic nonempty stderr assertion as proof of the new default.
