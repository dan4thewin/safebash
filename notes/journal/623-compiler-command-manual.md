# 623 - Making sash(1) a compiler-command reference

**Date:** 2026-11-11

DG asked to assess the man page after the architecture-guide refresh,
then approved the recommendation to rebuild it as a concise compiler
reference and preserve the useful language material for the future
safebash reference. The AI rewrote the manual and recorded that material's
location without starting the deferred specification effort.

## What changed

The former 1312-line page mixed an obsolete builtin-switch survey with
newer language documentation. Its `read` flags, `mapfile`, `echo -n`,
retired diagnostics and historical reader-bug catalogue were not a
dependable description of the shipped compiler. The preceding assessment
confirmed the obsolete spellings' refusals through the real CLI.

The replacement is 285 lines and 1378 words. It covers invocation,
options, output, command resolution, environment, runtime-archive location,
compiler exit statuses, examples and current limitations. It uses
safebash for the language and `.sb` for source examples, without pretending
the compiler enforces that suffix.

Current source at `7e8150b5` settles several details that the old page
missed or obscured:

- The default file output strips only `.sh`/`.bash`; `program.sb` produces
  `program.sb.out`. Inline input defaults to `a.out`. Examples use explicit
  `-o`, and the default behavior is documented rather than changed.
- `--extract` selects source, IR or manifest. The IR is inert Janet data
  representing the pre-binding AST, not SSA or executable Janet.
- IR/manifest inspection follows frontend checks and lowering but omits
  the executable backends' final SSA taint check. Extraction is a separate
  early artifact-reading path.
- `--sysroot` changes command-resolution checks, not the output ISA.
  Native linking uses the runtime archive in the compiler's build checkout;
  `SASH_RT_LIB` is not a native-driver override.
- `--trace` instruments branch/loop outcomes, not shell command execution.
  The legacy `--no-taint-cst` name now refers to frontend/AST analysis.

The command manual no longer advertises unsupported converter flags or
companion man pages this repository does not supply. It retains short,
honest warnings about the already-filed object-file collision, driver I/O
panics and regex anchor bugs. The deliberate regex interior-NUL limitation
is kept distinct from those bugs; no repair is claimed.

## Preserving the language material

The existing [specification task](../tasks/specification-document.md) now
maps the former redirection, test, escape, handler, special-value, function
and regex sections to exact line ranges at the pinned full Git revision.
Its retrieval command and permalink recover the complete original page.
This avoids another copied, partially normative document.

The map calls out the retired reader material and two regex corrections:
Bash does accept counted repetition, and the whole-subject anchor promise
contradicts the open anchor bug. Recent EXIT/ERR material remains useful.
All retained prose still needs the eventual specification's DD/spec checks;
its ordering, IDs, authoring model and authority remain unselected.

## Verification and scope

The new page renders with `groff -Tutf8 -t -man -ww` without warnings.
The first render caught leading-dot execution commands being read as roff
requests; zero-width escapes now preserve them in the displayed examples.
The source map's ranges were checked against the frozen original headings.

Real-CLI controls exercised file and inline compilation/execution, C/IR/
manifest output, all three extraction selectors, default extraction,
native default filenames, and trace-enabled/disabled execution. Extracted
source matches the input bytes. The first trace-visibility fixture had only
an `echo`, so correctly produced no branch records; the final example uses
an explicit branch and checks its trace on stderr. No taint bypass was used.
Generated executables were removed; text evidence remains in the session's
`files/manpage-refresh-20261111.5EZluo/`.

Whitespace and documentation references are checked, and normal commit/push
hooks remain in force. Only the manual, existing specification task, this
journal and status change. Compiler/runtime/converter code, specs, build
rules, README, licensing and architecture guides are untouched. No new
implementation sweep or coverage measurement is claimed; journal 619
remains the implementation-gate baseline and status's generated gate block
is preserved.
