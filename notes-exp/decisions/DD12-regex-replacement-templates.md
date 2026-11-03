---
title: "DD#12 - Standard C++ regex replacement templates"
status: accepted
---

DG selected standard C++ replacement notation for this target:

> use C++ replacement notation; when the IR consumer is written we can translate; go

This settles the remaining template fork after DD#11's matching/extraction
unit. No native template adapter or IR-consumer implementation belongs in
this runtime unit.

## Operations and representation

Use ordinary counted string values/views and compiled `std::regex`:

- `sub(subject, regex, replacement)` returns an owned string with only
  the first match replaced.
- `gsub(subject, regex, replacement)` returns an owned string with all
  standard-iteration matches replaced.

The string `sub` overload is distinct from the existing two-integer
arithmetic helper. No wrapper, output parameter, or public flag mode is
needed. Scalars and owned string-cell reads share these operations.
No-match returns an owned unchanged subject, not false or an error.

The implementation uses standard `regex_iterator` matches and
`match_results::format` with explicit format begin/end pointers. It copies
unmatched input between those matches and stops after the first for `sub`.
There is no subject/format snapshot or bespoke template parser/progression.

A direct probe found that this host's `regex_replace` convenience overload
truncates an embedded NUL even when passed a counted format string; the
standard range formatter preserves it. Using that formatter keeps the
counted-data contract without introducing a second replacement notation.

## Templates and iteration

Use standard C++ default formatting: `$&` for the whole match, `$1` through
`$99` for numbered groups, `$$` for a literal dollar, and the standard
prefix/suffix expansions `$` followed by backtick/apostrophe. Backslash is
ordinary replacement data: native `\1` is not a group reference here.
Do not add native's stricter backslash validation or silently rewrite
templates. Evaluated/computed templates follow the same rules as literals.

Empty and nonparticipating groups, missing references, literal text, and
prefix/suffix context follow the standard formatter. Its context belongs
to each iterator match, not an independently invented JavaScript model.
DD#11's standard iteration applies to replacement too: original anchor
context, nonempty retry after an empty match, and final-boundary termination.
`sub` stops after its first match, even when that match is empty.

Subjects and templates remain fully counted through NUL/high bytes.
DD#11's explicit literal-NUL **pattern** rejection is unchanged; it does
not prohibit NUL in replacement data.

## Failures and future consumer

The result is local and owned. Regex errors use the existing status-2
`regex: ` exception; allocation and length failures escape. No partial,
empty, or unchanged success-shaped result is returned on failure. Caller
assignment occurs only after successful evaluation, including when its
subject or format view borrows from that destination.
No helper adds a checkpoint or automatic END handling.

The future IR consumer must translate native replacement templates into
this target's notation, including evaluated/computed templates and literal
dollars/backslashes. That is separate from C++ source-string escaping.
The hand-written sash companion uses equivalent native `\0`/`\1` templates
and compares against an independent golden; it is not evidence that the
two template syntaxes are interchangeable.

## Scope

This completes first/global replacement, not bulk reads/splitting,
remaining formatting, general redirects, public wait, forked-C++/
background ownership, or source-to-C++ emission. No producer, native
compiler, or native runtime implementation changes are included.
