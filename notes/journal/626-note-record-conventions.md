# 626 - Repair decision and task record conventions

DG requested repairs to filenames and frontmatter in `notes/decisions/`
and `notes/tasks/`. The AI normalized the records against the existing
filename patterns and `.iwe/config.toml`: bare frontmatter, with `title`
as the document title. This is formatting and reference maintenance, not
a review of the decisions or a change to task dispositions.

## Inventory and repairs

The collections contain 121 decision documents and 576 task documents.
The separate `batch-531-hunks.tsv` is a historical ledger, not a malformed
Markdown note.

| Repair | Count |
|---|---:|
| Convert fenced headers to bare frontmatter | 456 |
| Remove leftover closing fences from already-bare headers | 32 |
| Add missing titles from existing H1 text | 48 |
| Combine mixed inline/block gate lists into block lists | 15 |
| Quote invalid plain scalar values | 27 |
| Preserve multiline resolution values without physical blank lines | 2 |
| Normalize filenames | 8 |

These categories overlap. There were 42 syntactically malformed metadata
blocks, not merely two competing fence styles. Unquoted colons and leading
backticks invalidated whole headers; fifteen gate lists put indented items
after a completed inline list.

The seven missing decision titles and 41 missing task titles come from
their existing headings. Two task headings wrap across consecutive H1
lines; their new titles include both lines. The original body headings
remain in place.

DD110-DD115 now carry the normal `DD` filename prefix. DD70's filename
uses the lowercase `a-b` suffix, and the task filename
`bug-file-test-n-diverges-from-bash.md` uses lowercase `n`. The actual
error categories `(A)`/`(B)` and file-test operator `-N` are unchanged.
The early aggregate `DD01-09-early.md` remains intact.

Seven existing documents needed ten reference replacements, including
inline filename citations as well as Markdown links. The ledger's H002
and H003 file paths remain unchanged: they identify hunks in the frozen
reference patch, not current note locations.

## Header boundaries matter

`dd75-retire-bracket-subscripts.md` had its closing frontmatter fence below
several paragraphs of real scoping prose. Its bare header now ends after
the actual gate list. All scoping text remains in the body; the later
separator remains an ordinary thematic break.

Two other tasks have `resolution: |` values containing blank paragraphs.
Blindly stripping their fences is wrong: iwe ends bare frontmatter at
the first blank line, even inside a YAML literal block. An isolated control
put a marker after each resolution and gate list. iwe found it with fenced
metadata and with an escaped quoted value, but not with the unmodified
literal block in bare metadata. Marker-free controls did not match.

Those two values now use quoted strings with escaped newlines. Their
decoded text, indentation and paragraph breaks are unchanged. The indented
`resolution-spec-tests:` text in one resolution remains part of that
value, not a newly invented metadata field.

The full-library `iwe normalize` command was deliberately not used:
it also rewrites document bodies, links and layout. A guarded session-only
formatter made the bounded transformations and checked their idempotence.
AGENTS now makes the filename, title and blank-line conventions explicit.

Even `iwe find` is not filesystem-read-only with the current virtual-directory
configuration: it regenerated `_decisions.md`, `_journal.md` and `_tasks.md`
at the notes root. An isolated control reproduced exactly those three
outputs without changing its input documents. The repository copies matched
the captured iwe inventory byte-for-byte; only those verified generated
outputs were archived and removed. No configuration or ignore rule changed,
and the previous top-level pruning was not undone.

## Preservation and verification

All 697 records now have valid bare YAML, a nonempty title and status,
and conventional filenames without case-fold collisions. iwe recognizes
every expected key and title. Its metadata filters reproduce all 21
authored status groups exactly. Before repair, its `accepted` filter found
52 of the 64 accepted records, and `done` found 345 of 378; these are
visibility repairs, not new acceptances or completed work.

Existing metadata fields and values are preserved apart from the necessary
syntax and reference repairs; no priority, kind, dependency or gate was
invented. Body text and headings are preserved apart from the intended
filename-reference replacements. Historical assertions were not refreshed
or reclassified.

All renamed targets and repaired local links exist. The ledger, generated
status block, iwe configuration, local lockfiles, compiler/helper binaries
and runtime archive retain their baseline identities. Spec history remains
at row 6902131. No source, spec, build rule or repository dependency changed,
and no spec sweep or coverage run was performed.
