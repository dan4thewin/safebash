title: A short README and GPLv3 licensing
date: 2026-11-11

## Direction

DG asked for a README proposal, then narrowed it to a short public
introduction rather than a contributor guide or language reference.
Internal DDs are not the public specification; condensing them is a
separate undertaking. GUIDE's future is also a separate discussion.
Detailed spec.db and gate mechanics do not belong in this README.

After discussing permissive and GNU licenses, DG directed:

> let's use GPLv3 and the runtime exception; draft the new readme and license files - don't commit yet

The AI initially prepared working-tree drafts and held them uncommitted
through the separate fresh-clone repair and legacy-artifact cleanups.
DG then authorized publication:

> commit the readme/licensing

The approved README, license texts, runtime notice and vendor-license
correction are unchanged from those reviewed drafts.

## README and license files

The README is roughly 400 words: purpose, the deliberate Bash distinction,
one native example, build/use and reviewed conversion, safety boundaries,
current platform limitations, and licensing. It does not invite external
contributions or link internal design records as public documentation.

DG subsequently settled the terminology: safebash is the language;
`sash` is the compiler. The README now uses that distinction.
DG also preferred `.sb` rather than `.sh` for safebash source files.
The README examples follow that convention, keeping `.bash` for Bash
input. No compiler suffix restriction or repository-wide rename is added.

DG replaced the initial `MODE` whitelist because it used `attest` for
ordinary action dispatch. The chosen example totals numeric input with
`readline`, checked integer conversion and typed accumulation, ending
with the unquoted `echo Total: $total`. The build/run example uses
`total.sb` and supplies two input lines.

The old tree-sitter/Janet pipeline, `local`, automatic trust, obsolete test
counts/commands and signing claims are gone. The working platform is
x86-64 Linux; accepted AArch64/cross work is not advertised as delivered.
The initial draft briefly noted the fresh-clone runtime-dependency failure;
the separate repair in journal 619 removed that warning.

DG also requested a build route without requiring Plan 9 mk. The README
now leads with Cargo: build the separate runtime workspace, then all three
Rust workspace tools, chaining the commands so a failed runtime build
stops the sequence. A C toolchain and `ld` remain prerequisites. The optional
`mk -p 1 build` retains the serialized workaround; it additionally builds
the standalone C helpers `sash-regex` and `sash-getopt`, so the recipes are
not described as producing identical artifact sets.

`LICENSE` contains the official GPLv3 text. The application notice says
version 3 only: the AI interpreted DG's "GPLv3" without adding an
unspecified future-version grant, and reported that interpretation.
`COPYING.RUNTIME` contains the unmodified GCC Runtime Library Exception
3.1; `sash-rt/LICENSE` scopes the grant to project-authored
runtime code and modifications, not the compiler or standalone tools.
The standard exception explicitly accommodates non-GCC compilation;
its GCC references have not been rewritten into a bespoke license.

Third-party notices and terms remain intact. The Plan 9 vendor README
still said Lucent Public License, while its existing license file records
the Foundation's 2021 MIT relicensing. Only that stale README label and
its license link are corrected.

## Review and adoption boundary

The exact current README example was extracted and compiled with the
existing native CLI. The documented input prints `Total: 42`; signed,
zero and empty inputs give their expected totals. Malformed and out-of-range
records abort at `attest` with status 1 and no stdout. The temporary source,
object and executable were removed. Compiler/converter help and current
build/link source supplied the other command evidence.

Both documented Cargo commands also completed with the existing dependency
resolutions; neither local lockfile changed. The resulting compiler linked
the exact README example using default runtime discovery, without
`SASH_RT_LIB`, and it printed `Total: 42`.

The license copies are compared byte-for-byte with GNU's GPLv3 text and
the GCC repository's `COPYING.RUNTIME`. Its upstream trailing blank line
is retained verbatim; the whitespace check permits blank-at-EOF for that
file only, with all other whitespace checks unchanged. Local documentation links and
the changed-file scope are checked. At commit preparation, the README
and licensing files matched their previously verified bytes; the builds
and example checks above were not repeated. No compiler/runtime
implementation, manifest, lockfile, GUIDE or spec change accompanies
these files; there is no new full sweep or coverage measurement.

This commit publishes the README and licensing files, not a completed
source-notice rollout. Applying copyright and GPL/exception notices to
individual source files and aligning Cargo license metadata remain
separate work in
[apply-source-license-notices](../tasks/apply-source-license-notices.md).
Appropriate copyright-holder identification must be confirmed rather
than inferred while adding those notices; third-party terms must remain
intact.
