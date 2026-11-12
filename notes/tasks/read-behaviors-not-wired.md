title: "read: behaviors not wired in (survey — DG to decide which matter)"
status: done
priority: P2
kind: decision
depends: []
gates:
  - "DONE (2026-09-02): DG approved both top recommendations — (1) accept `-r` as an ignored no-op, and (2) `-d` terminator semantics (drop the trailing empty from a final delimiter). Both shipped: journal 233, specs wordsplit.sh 22.25-22.27. The remaining gaps (`-n`/`-N`/`-u`/interactive) stay unimplemented unless a use case appears."

# `read` — behaviors not wired in

> **Resolved 2026-09-02.** DG: "we add -r as a no-op and we change -d to be
> terminator not separator." Both shipped (journal 233); this survey is kept
> for the record. `-r` → accepted no-op (`ir::classify_read_flags`); `-d` →
> terminator semantics (`sash_split_into_array` gains a `terminator` flag, set
> when `-d` is explicit). The rest below stays unimplemented (leave unless a
> use case appears).

Surfaced while unifying the read-flag parsing (journal 232). sash `read`
recognizes exactly three flags — `-a ARR`, `-d DELIM`, `-f SEP` — through one
shared classifier (`ir::classify_read_flags`). Everything else is either
rejected (SB5015) or unimplemented. This surveys the gaps so DG can pick which
matter.

## Flags sash rejects that bash has

| flag | bash meaning | sash | notes / recommendation |
|------|--------------|------|------------------------|
| `-r` | raw (no `\` processing) | **accepted no-op** (2026-09-02) | **sash is ALWAYS raw**, so `-r` requests the default. Was SB5015; now accepted-and-ignored in `ir::classify_read_flags` so a hand-written `read -r x` (ubiquitous in bash) is not an error. Bundled forms (`-rd ''`) are NOT supported — sash bundles no flags. |
| `-n N` / `-N N` | read exactly N chars | unimplemented | A real (non-interactive) behavior — fixed-width field reads. Not wired. Add only if a use case appears. |
| `-u FD` | read from fd N | unimplemented | sash uses redirects (`read x < file`); an arbitrary fd number isn't expressible. Probably fine to leave. |
| `-p` / `-s` / `-t` / `-e` / `-i` | prompt / silent / timeout / readline / initial-text | unimplemented | All **interactive** — irrelevant to sash's batch/compiled model. Fine to reject. |
| `-z` | (not a bash `read` flag) | rejected | NUL reading is `-d ''` in sash (bash convention), so no `-z` alias is needed — see below. |

## Semantics not wired

- **NUL-terminator trailing-empty — FIXED (2026-09-02).** An explicit
  `read -d` now uses **terminator** semantics: a trailing delimiter does not
  create a final empty element (bash `mapfile -d ''` / `find -print0` /
  `xargs -0`). Interior empty records (`a::b`) are still kept; only the single
  trailing empty from a final delimiter is dropped. The sash-getopt idiom is
  now clean:

  ```
  sash-getopt -o ab: -- -a -b val 'has space' | read -a opts -d ''
  # opts = [-a] [-b] [val] [--] [has space]      ← trailing [] gone; bless [[ -n $x ]] passes
  ```

  Implemented via a `terminator` argument to `sash_split_into_array`, set iff
  `-d` is explicit (`delim.is_some()`). The **default** whitespace/space split
  (no `-d`) keeps **separator** semantics — DG scoped the change to `-d` only.

- **`-f SEP` is a sash invention**, not a bash `read` flag (bash uses `IFS`,
  which sash doesn't have). Noted for completeness; no action.

## What IS wired (for reference)

- `read x` / `read a b` (scalar, whitespace-split remainder).
- `read -a arr` (array; whitespace split by default, SEPARATOR semantics).
- `read -d DELIM` / `read -d ''` (record delimiter; empty = NUL; TERMINATOR
  semantics — a trailing delimiter drops the final empty element).
- `read -f SEP` (sash field separator; requires ≥2 targets, SB5016).
- `read -r` accepted as an ignored no-op (sash reads are always raw).
- Unknown flags → SB5015, now **consistently** in both the standalone and
  lastpipe paths (the two parsers were unified — journal 232).
- The end-to-end getopt idiom works: `sash-getopt … | read -a arr -d ''`
  (and the `< file` form), splitting NUL-separated fields, space-bearing
  fields preserved, no trailing empty.

## Recommendation summary

1. **`-r` → accepted no-op** — SHIPPED 2026-09-02 (journal 233).
2. **NUL-terminator trailing-empty** → dropped for `-d` — SHIPPED 2026-09-02
   (journal 233). Default whitespace split unchanged (DG scoped to `-d`).
3. The rest (`-n`/`-N`/`-u`/interactive) → leave unless a use case appears.
