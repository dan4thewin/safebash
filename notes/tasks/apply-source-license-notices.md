title: "Complete GPLv3 source notices and Cargo licensing metadata"
status: open
priority: P3
kind: work
depends: []
gates:
  - "Copyright-holder identification is confirmed rather than inferred from Git authorship"
  - "Project-authored source notices apply GPL-3.0-only; relevant runtime sources additionally carry the GCC Runtime Library Exception 3.1 notice"
  - "The runtime exception does not spread to the compiler or standalone tools"
  - "Third-party copyright notices and licenses remain intact, with project modifications distinguished where needed"
  - "Cargo license metadata agrees with the project/runtime notices, and distributed packages include their required license texts"
  - "The standard GPLv3 and runtime-exception texts remain unmodified"

## Scope

DG approved the README and licensing files on 2026-11-11; journal 618
records the choice and its draft-to-commit history. The application notices
select GPL-3.0-only, with GCC-exception-3.1 for project-authored runtime
code and modifications. They preserve third-party terms.

That commit deliberately leaves source headers and Cargo manifests
unchanged. Complete those remaining application steps without changing
the chosen license or extending the exception beyond the runtime.
The exception's opening paragraph requires a notice on each covered file;
copying its text into `COPYING.RUNTIME` alone is not that application step.

Confirm the appropriate copyright-holder identification before writing
notices. Retain the original Plan 9, FreeRTOS and other third-party notices;
do not replace them with a blanket project copyright claim.

Use the corresponding SPDX expressions in package metadata:
`GPL-3.0-only` for project packages without the exception, and
`GPL-3.0-only WITH GCC-exception-3.1` for the runtime. Check license-text
availability in actual distribution artifacts rather than relying only
on relative links that work in the repository checkout.
