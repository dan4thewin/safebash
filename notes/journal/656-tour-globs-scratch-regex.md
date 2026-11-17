title: Expanding the tour for globs, scratch and regexes
date: 2026-11-17

# Expanding the tour for globs, scratch and regexes

DG requested: "update and expand the tour for the glob slide, and demonstrate
the other features just added". The AI expanded the authored deck and
regenerated its GitHub edition, starting from `b8a8ec06` on
`glob-scratch-trial`. Main remains `6c7b899f`; DD131/DD132 remain provisional.

## What the examples teach

The original glob example now creates visible and hidden files in deliberately
unsorted creation order. Its output demonstrates bytewise sorting within each
pattern, concatenation in pattern order, explicit dotfile selection and
absolute results. Three companion slides cover explicit `glob` with directory
prefixes and pattern-list splats, accumulation and retained duplicates,
quoted literal stars versus array/append/loop sugar, and successful no-match
versus a failed directory operation that preserves the ordinary destination.

Four collection examples distinguish accumulating fills from assignment,
immutable snapshots from mutable builders, fresh construction at each visit
to one source site, and rejected scratch mutation. After the existing taint
and trust introductions, two examples show caught scratch validation: the
successful arm permits ordinary reads, a failed value can remain unresolved,
and an outside read does not inherit the successful arm's permission.

The regex section adds six examples: whole-subject anchors over newline data,
quiet `r?` rejection of malformed syntax, an attested runtime const pattern,
fresh capture groups across matching and nonmatching attempts, `rglob` over
an attested immutable pattern list, and substitution text on no-match and
identity replacement. The anchor slide also states the retained newline and
first-NUL limitations; it does not advertise the deferred multiline mode.

That is **15 added programs**, plus the revised original glob program:
**71 programs total**, with **15 intentional failures**. The two new refusals
have terse comments on their responsible lines. All 55 other original
programs and their recorded output are unchanged.

The introductory notice labels the branch-only package as provisional. The
closing converter example now warns that accumulating fills, mutable
underscore names and runtime regexes still require review. Neither the tour
nor this update resolves converter migration or DG's adoption decision.

## One authored source, unchanged helpers

`docs/tour/tour.md` remains the only authored deck. The existing native
`check.sb` renderer produces `docs/tour/README.md`; no second set of manually
maintained examples or outputs was introduced.

The revised `Globs` example retains its title and prints raw paths directly
under its temporary working directory, preserving the existing narrow path
normalization. The other selector examples print basenames instead. There
is no broader substitution hiding output differences.

The three `.sb` helpers, presenterm configuration and mk rules retain their
baseline bytes. The helpers were rebuilt before use so they link the current
runtime table ABI from journal 655.

## Verification

The initial `mk -p 1 tour-doc` correctly refused publication for two invalid
draft examples: an unquoted bare `read` source received SB5057, and attaching
a redirect to `glob` received SB4046. The existing README stayed byte-identical
and no staging file remained. The examples were corrected to pass a string
value to `read` and handle `glob` directly; no compiler or helper change was
needed.

The final deck ran through the real compiler and native executables with
normal taint checking: **71 snippets, zero mismatches**. Independent assertions
checked the actual new outputs, not just the runner's success/failure categories:
ordering and duplicates, builder/snapshot independence, per-attempt freshness,
no-match preservation, quiet invalid-pattern handling, validated selection and
the distinction between matched truth and changed text.

The two new negative examples each produce exactly one intended diagnostic
on source line 3: SB5103 for the second scratch construction/write and SB5020
for the tainted read outside the caught attest. The latter is the ordinary
taint gate, not the initially predicted SB5107.

Two final full renders are byte-identical. GFM parsing finds 71 Bash blocks
and 71 recorded-output blocks, with 15 expected-failure labels. Local links,
whitespace, private-path exclusion and absence of leftover render drafts
also pass.

Presenterm 0.16.1 loads the complete deck with `--validate-snippets` in a
140-column, 48-row pseudo-terminal and exits normally on `q`. Strict
`--validate-overflows` is **not** counted as passed: under the emulated terminal
it reports horizontal overflow on slide 2 in both the original and expanded
decks, including the expanded deck at a wider terminal size. No theme or
layout-policy change was made to accommodate that check. The initial
noninteractive attempt lacked a controlling terminal; the bounded probes
left no presentation process running.

This is documentation-only. The accepted product gate from journal 655
remains the implementation evidence: rows **7258678-7271250**, 12,573 checks.
No new product spec sweep, C-backend measurement or coverage run was needed.
Compiler/runtime artifacts, both local lockfiles and the spec-history boundary
remain unchanged. Private logs, exact output assertions and identity records
are retained in the session's `tour-trial-20261117` evidence directory.

Only the two tour Markdown files, this journal and current status change.
No architecture, language policy, converter repair or unrelated backlog work
is included.
