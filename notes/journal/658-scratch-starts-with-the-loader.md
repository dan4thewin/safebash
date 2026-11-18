title: Scratch starts with the loader
date: 2026-11-18

# Scratch starts with the loader

DG rejected the tour's rollout example as contrived. Scratch was devised
alongside accumulating bulk array loaders: the useful contrast is gathering
several batches versus working with only the result of the current load,
not an invented need to freeze a rollout plan. DD131 already states that
destination model; this revision changes the explanation, not the language.

The AI removed the two rollout slides and made the existing per-directory
log example the scratch introduction, beside the glob loaders. Its program
is unchanged: directories with two, one and zero log files report
**2, 1, 0** through `glob _logs`. An ordinary target would accumulate
**2, 3, 3**. The underscore makes a fresh result explicit, so there is no
manual clear to forget and no previous batch to mistake for the current one.

A new companion slide attempts two separate `glob` fills into `_logs`.
The second construction site is rejected. This distinguishes a second
source site from revisiting the same site in a loop, which constructs a
new instance. The ordinary alternatives are explicit: one `glob logs`
with both patterns, or two calls accumulating into `logs`.

The explanation retains immutable instances, scalar scratch, unchanged
scope, initializer self-read rejection and bare `_` as discard. The later
NUL-safe filename copy, caught numeric input and fresh regex-capture
examples are unchanged.

## Evidence and scope

Removing two programs and adding one leaves **70 programs**, including
**15 intended failures**. The existing `mk -p 1 tour-doc` runs all of them
through the real compiler and native executables and regenerates the GitHub
edition with zero outcome mismatches. GFM parsing confirms that all
**69 retained programs and their recorded outputs are byte-identical**.

The new refusal is exactly one SB5103 at `snippet.sh:5:1`, naming the
original construction at `3:1`. Three additional actual-CLI controls pin
the prose: an ordinary loop target reports 2, 3, 3; both ordinary gathering
spellings collect the two files. All use normal taint checking, and their
temporary files are removed.

The baseline is `ca6dcedd` on `glob-scratch-trial`. Only the authored deck,
generated README, current status and this journal change. Journal 657
remains the historical account of the previous revision. The product gate
remains journal 655's sweep ending at row 7271250; there is no new product
sweep, C-backend measurement, coverage or interactive-layout claim.
DD131/DD132 remain provisional and main remains unchanged.

Private evidence is in the session's `tour-scratch-loaders-20261118`
directory.
