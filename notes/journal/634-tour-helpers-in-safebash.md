# 634 - Tour helpers in safebash

DG asked for safebash versions of the tour scripts and helpers, without
deleting the originals. The AI added three side-by-side sources:

- `docs/tour/sash-snippet.sb` compiles `snippet.sh`, runs it with the existing
  `world 42` arguments, and displays compiler diagnostics on failure.
- `docs/tour/tour.sb` locates the checkout from its executable, checks its
  dependencies, and launches presenterm with the original configuration.
- `docs/tour/check.sb` extracts executable fences, strips the hidden `/// `
  prefix, compares actual and expected success, and reports snippet output.

The extensionless runner, both shell scripts, `presenterm.yaml` and `tour.md`
are unchanged. The native programs invoke their external tools directly,
not through a shell interpreter. Normal taint checking remains enabled.

## Build and use

With the checkout's compiler and runtime already built, run from its root:

```sh
mkdir -p target/tour &&
target/release/sash docs/tour/sash-snippet.sb -o target/tour/sash-snippet &&
target/release/sash docs/tour/check.sb -o target/tour/check &&
target/release/sash docs/tour/tour.sb -o target/tour/tour
```

Then `target/tour/check` checks the deck, `target/tour/check -v` includes
successful output, and `target/tour/tour` launches it. The launcher forwards
presenterm options unchanged. Keep these binaries under `target/tour/`:
that placement supplies the checkout root, and their directory leads PATH
so the unchanged presenterm configuration selects the native snippet runner.
The original launch/check routes remain available.

The checker owns one temporary directory and removes it on EXIT or ERR;
HUP, INT and TERM select EXIT. ERR cannot catch the current compiler's
possible-NUL guard on dynamic external argv, so cleanup writes the pathname
as a NUL-delimited record to `xargs -0 -r rm -rf --`. The whole pipeline is
caught, including its construction failures. This retains literal external
arguments without changing handler rules or interpreting the pathname as
shell code. Cleanup leaves tool stderr visible.

Two deliberate checks belong only to the new counterpart: an unfinished
executable fence and a deck with no runnable snippets are errors, rather
than successful checks of less than the intended source.

## Coupled readline repair

The port exposed a runtime defect: one 5,000-byte line arrived as records of
4,096 and 904 bytes. `sash_read_line` stopped at its stack buffer's length,
even without a newline or EOF. That would silently corrupt long snippet
source, contrary to DD99's record contract.

The reader keeps its 4,096-byte stack fast path and grows a temporary heap
buffer for longer records. Growth happens only after another payload byte
arrives, not merely because a boundary-sized record may end. The final result
uses the existing exact-sized, terminated allocation; the growth buffer is
freed. Empty records, interior NUL/UTF-8 bytes and one final unterminated
record retain their meaning. Heap exhaustion aborts rather than publishing
a successful partial record.

This changes only record capacity. Existing treatment of read errors and
interruptions is unchanged; the broader recoverable-file-failure work is not
implemented here.

## Evidence

Three new dual-mode `wordsplit.sh` cases, `rlsize.1`-`.3`, cover exact record
copies across the old boundary, binary/unterminated input, and growth under
a deliberately insufficient heap. All three failed in both modes before
the repair; the 144 existing cases passed. Afterwards, all 147 passed in
each mode. Reinstating the old length cap made exactly the same three cases
fail again in both modes. The restored runtime source and archive matched
the saved positive hashes; compiler and both local lockfiles stayed fixed.

The native checker ran the current deck: **55 snippets, 0 mismatched**.
Twenty-six controlled helper invocations also passed: original/native
runner and checker comparisons, literal extraction of a 12,000-byte line,
hidden lines, expected and unexpected failures, verbose indentation, EOF,
normal/ERR/TERM cleanup, and launcher dependency and argument handling.
The launcher controls include a symlink, another cwd, paths containing
spaces, an empty option argument, and the installed presenterm's `--help`.
Cleanup preserves a sentinel outside the allocated directory.

The predicted ordinary gate passed:
`NEW=6 FLIPS=0 RETIRED=0 TOTAL=11375 timeout 900 mk -p 1 gate`.
Rows **6913501-6924875** contain 11,362 passing checks and 13 existing skips,
with exactly the six new `rlsize` mode IDs, no flips, retirements, ghosts or
plan mismatches. Native/C counts are 5,799/5,576; liveness is 251. This gate
includes `mk test`.

The existing hook's formatting, Clippy and totality checks passed without
bypasses, as did the separate runtime formatting check. Its staged-range
complexity check skipped because the index remains empty; this is not a new
complexity measurement of the inherited compiler repair.

The helper ports and reader repair remain uncommitted alongside the existing
review drafts on `spec-outline`. No originals were replaced, no coverage
work resumed, and no commit or push was made.
