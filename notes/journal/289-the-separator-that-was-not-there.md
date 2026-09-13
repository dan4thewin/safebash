# 289 — The separator that was not there

`converter-positional-use-needs-bless` was filed as a feature: DG resolved it
on 2026-07-08 (hoist the positional, attach a placeholder attest, mark it
MANUAL), and the work was to implement that. Implementing it turned up three
bugs that had nothing to do with the feature and everything to do with looking
at the output.

## What the task actually asked for

`sash-convert` turned `echo "$1"` into `echo "$(arg 1)"`. `$(arg 1)` is
tainted (DD#60), so that output could never compile — every use raised SB5020,
and the reader got a diagnostic about a construct the converter had just put
there. DG's resolution: emit a scaffold that builds.

    arg1=$(arg 1); attest arg1 [[ -n $arg1 ]]  # MANUAL: tighten this predicate …
    echo "$arg1"

Same for the for-in form, which now also gets a body-leading `attest x`. That
part was mechanical.

## The three bugs

**`$#` printed −1.** The converter emitted `$(($(argc) - 1))`. A command
substitution is not an arithmetic operand — but until journal 286 the arith
parser silently recovered an unparseable operand as literal `0`, so this
folded to `0 - 1`. Every converted script that asked how many arguments it had
got a wrong answer, silently, since the argv trio landed in `cc4c8d3`. Journal
286 turned it into a hard SB4020 four commits ago, which is why it surfaced
now. The DD#58 sexpr spelling `(argc)` was available the whole time.

**`for x in "$@"` iterated one time too many.** The converter emitted
`$(argv)`. DD#60 is explicit that `$(argv)` includes argv[0] and that
`$(argv N)` — "the natural 'iterate user args' idiom" — is the slice. Bash's
`"$@"` is argv[1:]. So the converted loop ran an extra iteration with the
program's own path as the first value. `spec/convert.sh` conv.q1 pinned it,
asserting `for x in $(argv)` — the spec had learned the bug.

**And the one worth the entry.** `echo "$1 $1"` converted to `echo "$a$a"`.
The space was gone.

Stock tree-sitter-bash 0.25 gives the *second and later* `simple_expansion`
nodes inside one double-quoted string a span that includes the preceding
whitespace. In `"$1 $1"` the first node spans `$1` and the second spans ` $1`.
The converter replaced `node.start_byte()..node.end_byte()`, so it consumed
the separator. This was not about positionals: it hit `$HOME`, every known env
var, every family the converter rewrites. `echo "$HOME $HOME"` became
`echo "$home$home"`.

The fix does not trust the node's start. A `simple_expansion` is a `$`
immediately followed by its name, and the *name child's* start is exact, so
step back one byte and check for `$`:

    fn expansion_start(node: Node, child: Node, source: &[u8]) -> usize

Three bugs, three different mechanisms, one property in common: each produced
output that *looked* right. `$(argc)` reads like a call. `$(argv)` reads like
"the arguments". `echo "$a$a"` is a plausible line of shell. Journal 286 ended
on the same note about a pid of `0` and `$(( 1 + ))` folding to `1`; this is
the third session in a row where the bug was hiding behind plausibility.

## What found them

Not reading the code — running it. The new `conv.x1`–`conv.x9` specs convert a
script, **compile the conversion, run it with `a b c`, and compare against
what bash printed for the same script.** Every substring assertion in
`convert.sh` passed throughout: `57.13b` asserted `$(($(argc) - 1))` appeared
in the output, and it did, and the program printed −1. A text assertion on a
translator's output can only check that it said what you expected; it cannot
check that what it said is true. The round trip can.

Three of the ten new checks failed on first write. That is the argument for
them.

## Left for DG

`cmd "$@"` still gets a MANUAL marker rather than a scaffold. Bash runs the
command with zero arguments when `"$@"` is empty; SB5032 requires the sash
author to state that case, and both spellings that do (`@{a:-WORD}`,
`@{a:?WORD}`) mean something else — substitute an argument, or abort. DG's
2026-07-08 resolution predates SB5032 and assumed a bare `@a` would compile.
There is no faithful mechanical answer, so the converter says so instead of
picking one. Filed as `converter-splat-empty-case.md`.

Sweep 4352 passing, 9 skipped, 0 failing, 0 flips.
