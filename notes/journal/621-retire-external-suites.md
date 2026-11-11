# 621 - Retiring the legacy external suites

**Date:** 2026-11-11

DG requested a tag before removing `suites/`, so the old material could
be found if useful for future converter tests. Inspection found four
tracked files, but 1,015 ignored corpus files. DG clarified that those
belong upstream: preserve how to retrieve them, not another vendored copy.

## What the tag preserves

Annotated tag `archive/legacy-suites` points to commit `628ba828`, with
`suites/README.md`, `brush_runner.py`, `oils_runner.py` and
`oils-harness.sh`. The tag was pushed and its four file contents verified
before deleting the directory. No downloaded corpus files entered Git.

The README already identified the three upstream sources, but its commands
changed into `/tmp` and then used relative repository destinations. They
also depended on existing destination directories to retain the upstream
layout. The tagged version keeps the repository working directory, uses
a fresh temporary download directory and explicitly creates the destinations:

- Oils: `https://github.com/oilshell/oil.git`, copying `spec/`.
- GNU Bash: the `bash-5.2.37.tar.gz` release from `ftp.gnu.org`, copying `tests/`.
- ShellCheck: `https://github.com/koalaman/shellcheck.git`, copying `src/`.

The separate Brush runner's input source is now identified too:
`https://github.com/reubeno/brush`, under `brush-shell/tests/cases/`.
Its inputs were outside this directory.

The Git sources and their relevant subdirectories were reachable, as was
the Bash release tarball. This was an availability check, not a reconstruction
of the old copies. Oils and ShellCheck follow their default branches; the
guide explicitly does not promise the exact historical downloaded bytes.
The obsolete runners are reference material, not current compiler gates.

Read the retained guide with:

```sh
git show archive/legacy-suites:suites/README.md
```

## Removal and boundaries

Removed the four tracked files and all 1,015 ignored downloads: 668 Bash
files, 320 Oils files and 27 ShellCheck files. The full directory membership
and file hashes were rechecked immediately before removal. There were no
symlinks, special files or nested Git repositories.

No current compiler, build, spec or hook reference to the directory was
found. Removed its active tree listings from AGENTS, GUIDE and the committed
README; GUIDE now points to the archive tag. The README change removes only
one line from the old version, without touching or publishing its draft
rewrite. Historical journal/task references and the existing ignore rules
remain.

No legacy runner was executed, no compiler/runtime/spec source changed,
and no full spec sweep or fresh coverage measurement was needed for this
retirement. Converter-test migration is only a possible future use, not
newly authorized implementation work. The root README/licensing drafts
and both local lockfiles remain untouched.
