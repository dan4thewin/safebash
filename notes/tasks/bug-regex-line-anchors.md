title: "bug: regex `^`/`$` anchor at lines, and a global match re-anchors after every match"
status: open
priority: P1
kind: bug
depends: []
gates:
  - "`x=$(arg 1); attest x [[ $x =~ ^[a-z]{1,32}$ ]]` fails the gate for `$'abc\\n;rm -rf x'` and for `$'x;y\\nabc'` (today both are blessed whole), native and C"
  - "`[[ $'ab\\ncd' =~ ab$ ]]` and `[[ $'ab\\ncd' =~ ^cd ]]` are false"
  - "`${x:s/^a/X/g}` on `aaa` is `Xaa` (today `XXX`); `${w:s/^a-/X/g}` on `a-a-a` is `Xa-a` (today `XXa`)"
  - "`matchall` of `^a` on `aaa` finds one match (today 3)"
  - "on `$'ab\\ncd'`, `${z:s/^/>/g}` is `>ab` / `cd` (today `>a>b>` / `>c>d`) and `${z:s/^c/X/g}` is unchanged (today `ab` / `Xd`)"
  - "a non-global `${z:s/^c/X/}` and a `.`/`[^…]` across a newline keep today's answers (neither matches a newline)"
  - "sash.1 says `[^…]` never matches a newline, beside the `.` sentence"
  - "DD39 addendum: the first local change to the vendored matcher"

Two bugs in how the runtime uses the vendored Plan 9 matcher. The first is
P1 because it lets a value past an attest gate. Neither is fixed on the
`taint-walker-redesign` branch. Found during F10 of the batch-531 port
(journal 552); DG 2026-10-31: "strict rule; file P1".

## 1. `^` and `$` are line anchors

`docs/sash.1:839-842` documents "`.` matches any byte except newline; `^`
and `$` anchor at the subject's start and end". The matcher treats them as
*line* anchors instead:

- `sash-rt/csrc/regexec.c:108`: BOL matches at `s == bol || *(s-1) == '\n'`;
- `:112`: EOL matches at `s == j->eol || r == 0 || r == '\n'`;
- `:55-58`: the BOL start fast path skips to just after the next `\n`.

So a pattern anchored at both ends accepts a value if any one of its lines
matches:

```
x=$(arg 1)
attest x [[ $x =~ ^[a-z]{1,32}$ ]]
echo "[$x]"
```

With the argument `$'abc\n;rm -rf x'` this prints `[abc` / `;rm -rf x]`,
rc 0: the whole value is blessed. `$'x;y\nabc'` is blessed too. Measured on
3b96bc40 and on the branch (e98fca8), native and C.

A value with an interior newline reaches a program through argv, the
environment, a capture's output or `$(<file)`; a `readline` value cannot
carry one. The attack is line injection, e.g. `bob\nadmin=true` written
into a config file. F10's strict predicate rule (DD24's Bless list: string
comparisons, regex match, file tests) makes `=~` the main gate, so this
sits under every regex attest.

## 2. A global match re-anchors after every match

`matchall_into` (`sash-rt/src/lib.rs:2585-2630`; it re-runs the engine at
`str_buf.add(off)`, :2599) and `sash_regex_replace` (:2707-2800; it copies
the remainder each iteration, :2746-2752) run the engine on the remainder
as if it were a fresh string. So `^` matches again at every resume point.
Measured on 3b96bc40 under `--no-taint`, native and C identical:

| program | today | subject anchors (sash.1) |
|---|---|---|
| `x=aaa; ${x:s/^a/X/g}` | `XXX` | `Xaa` (as sed and perl) |
| `w=a-a-a; ${w:s/^a-/X/g}` | `XXa` | `Xa-a` (as sed and perl) |
| `- matchall "$x" '^a' m; $(len m)` | `3` | `1` |
| `z=$'ab\ncd'; ${z:s/^/>/g}` | `>a>b>` / `>c>d` | `>ab` / `cd` (as perl without `/m`) |
| `${z:s/^c/X/g}` | `ab` / `Xd` (bug 1) | unchanged |

## Fix

- In `regexec.c`, match BOL only at the subject's start and EOL only at its
  end, at all three places.
- Make the resume loops keep the subject's real start: `regexec.c:201-206`
  already takes `resub[0].s.sp` as the search start while `bol` stays the
  string start, so pass the whole subject plus an offset instead of the
  remainder. Enumerate every caller of the two loops when fixing.
- `.` and `[^…]` keep excluding newline (`regcomp.c` bldcclass ~:551-559
  removes `\n` from every negated class), so `^[a-z]+$` and `^[^;]+$` reject
  any value with a newline. Document the `[^…]` half in sash.1, beside the
  `.` sentence.
- DD39: `regexec.c` and `regaux.c` are byte-identical to plan9port today, so
  this is the first local change to the matcher and needs an addendum. DD39's
  2026-09-11 ruling keeps a wrong answer a bug. The runtime calls
  `regcomp9`, not `regcompnl9` (lib.rs:2431/2467/2513/2585/2724).
- Only `sash-rt` needs a rebuild. Native links
  `$CARGO_MANIFEST_DIR/../sash-rt/target/release/libsash_rt.a`
  (`sash/src/lib.rs:1001-1004`), and the compiler validates patterns with
  `regcomp.c` alone (`sash/build.rs`).

Converted bash moves closer to bash: bash's `=~` compiles without
`REG_NEWLINE`, so its anchors are string anchors too. Its `.` matches a
newline and sash's does not; that difference is unchanged.

## Specs

Write them in `spec/regex.sh`, at fresh ids above the spec.db high-water
mark for `39.` (take it from `runs`, not `grep`). Cover each gate above, in
both modes. The fallout on existing specs is unmeasured, so predict FLIPS
from a standalone run first.

Probes: `disc/p8.sh` and `anc/g1.sh` in `files/port-f10-probes.tar.gz` (session
folder).
