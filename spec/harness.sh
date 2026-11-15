#!/bin/bash
# spec/harness.sh — the spec hang guard and tour publication.
#
# The first three checks test common.sh. They earn their place because a
# hang is the one failure mode the suite cannot report: `bash run.sh` stops
# producing output entirely and every later spec goes unrun. That is not
# hypothetical — the DD#14 pipeline-stage redirect bug (spec 14.32) made
# `cat < f | cat` read the runner's own stdin, and the suite sat there
# forever with no indication of which spec was responsible.
#
# So every child the harness spawns gets `</dev/null` and a `timeout`. If
# either guard is ever dropped these three checks fail instead of hanging.
set -eu
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
. common.sh
setup harness
plan=9
echo "1..$plan" >&3
set +e

# ht.1 a program that reads stdin sees EOF, not the runner's stdin. Without
# the guard this blocks whenever run.sh is invoked from a terminal.
run_sash 'readline x
attest x [[ $x == "" ]]
echo "got=[$x]"'
[[ $rc = 0 && $out = "got=[]" ]]; chk ht.1

# ht.2 a program that never exits is killed at SPEC_TIMEOUT and reported as
# such. `timeout` exits 124; the message names the guard so the failure is
# not mistaken for an ordinary bad exit status.
SPEC_TIMEOUT=2 run_sash '/bin/sleep 30'
[[ $rc = 124 && $rerr == *"hang guard"* ]]; chk ht.2

# ht.3 …and the suite carries on afterwards, which is the whole point: one
# runaway spec must cost its timeout, not the rest of the run.
run_sash 'echo alive'
[[ $rc = 0 && $out = "alive" ]]; chk ht.3

# Compile the shipped helpers through common.sh in each backend, preserving
# their executable-relative target/tour layout rather than interpreting .sb.
fixture=$PWD/tour-fixture
tour_scratch="$PWD/scratch [tour]+"
mkdir -p "$fixture"/{target/tour,target/release,docs/tour} "$tour_scratch"
ln -s "$(command -v "${SASH:-sash}")" "$fixture/target/release/sash"
cat > stage-helper.sh <<'SH'
cp -- "$2" "$1"
SH
for helper in check sash-snippet; do
    run_sash_under "$(<"$repo_root/docs/tour/$helper.sb")" \
        bash "$PWD/stage-helper.sh" "$fixture/target/tour/$helper"
    if [[ $rc != 0 || ! -x $fixture/target/tour/$helper ]]; then
        echo "could not build tour $helper: $err $rerr" >&3
        exit 1
    fi
done

tour_deck() {
    printf '%s\n' '---' 'title: Fixture tour' 'sub_title: "Recorded examples"' \
        '---' 'Fixture' '===' "$1" > "$fixture/docs/tour/tour.md"
}

tour_run() {
    tour_rc=0
    timeout "$SPEC_TIMEOUT" env TMPDIR="$tour_scratch" \
        "$fixture/target/tour/check" "$@" </dev/null > tour.out 2> tour.err ||
        tour_rc=$?
}

shopt -s nullglob
tour_clean() {
    local -a leftovers=("$fixture/docs/tour"/.README.md.* "$tour_scratch"/sash-tour.*)
    [[ ${#leftovers[@]} = 0 ]]
}

tour_preserved() {
    cmp -s saved-readme "$fixture/docs/tour/README.md" && tour_clean
}

# Hidden setup executes; only visible source and literal combined output
# appear in the document. Output backticks must not close their own fence.
tour_deck "$(cat <<'MD'
Visible prose.
<!-- jump_to_middle -->
<!-- pause -->
```bash +exec:sash
/// echo hidden > input.txt
echo shown
cat input.txt
echo diagnostic >&2
echo '```'
echo '``````'
echo '<b>literal output</b>'
```
<!-- end_slide -->
Compile failure
===
```bash +exec:sash +expect:failure
int n=3
n=oops # type mismatch
echo $n
```
<!-- end_slide -->
Runtime failure
===
```bash +exec:sash +expect:failure
echo before failure
fail 7 # explicit failure
```
<!-- end_slide -->
Empty output
===
```bash +exec:sash
:
```
MD
)"
tour_run --render
rendered=$(cat "$fixture/docs/tour/README.md" 2>/dev/null)
literal_output=$(cat <<'TEXT'
```````text
shown
hidden
diagnostic
```
``````
<b>literal output</b>
```````
TEXT
)
[[ $tour_rc = 0 && $rendered == *'# Fixture tour'* &&
   $rendered == *'Visible prose.'* && $rendered == *'```bash'* &&
   $rendered == *"$literal_output"* && $rendered == *'SB4030'* &&
   $rendered == *'**Output (expected failure):**'* &&
   $rendered == *$'```text\nbefore failure\n```'* &&
   $rendered == *'_No output._'* &&
   $rendered != *'/// echo hidden'* && $rendered != *'+exec'* &&
   $rendered != *'<!-- end_slide -->'* && $rendered != *'<!-- pause -->'* &&
   $rendered != *'<!-- jump_to_middle -->'* ]] &&
    tour_clean; chk tourgen.1

# Normalization is scoped to the known demonstration fields, not arbitrary
# numbers/text. A TMPDIR with regex metacharacters must be compared literally.
tour_deck "$(cat <<'MD'
```bash +exec:sash
echo started job 4321
echo kernel literal
echo hostname: literal
```
<!-- end_slide -->
Globs
===
```bash +exec:sash
touch "report [1].txt" other+.txt
array files=(*.txt)
for file in @files { echo $file }
```
<!-- end_slide -->
Background jobs
===
```bash +exec:sash
sleep 0.01 &
int pid=$(getcpid)
echo started job $pid
- wait $pid
echo status $(waitstatus)
echo number 1234
```
<!-- end_slide -->
Capturing output
===
```bash +exec:sash
echo kernel release-$(getpid)
user=$(getenv USER) || user=missing
attest user p?
echo user $user
```
<!-- end_slide -->
Reading lines
===
```bash +exec:sash
echo hostname: host-$(getpid)
```
MD
)"
tour_run --render
first_rc=$tour_rc
cp "$fixture/docs/tour/README.md" first-render
tour_run --render
rendered=$(cat "$fixture/docs/tour/README.md" 2>/dev/null)
[[ $first_rc = 0 && $tour_rc = 0 &&
   $rendered == *'/tmp/<tour>/report [1].txt'* &&
   $rendered == *'/tmp/<tour>/other+.txt'* &&
   $rendered == *'started job <pid>'* && $rendered == *'kernel <kernel-release>'* &&
   $rendered == *'hostname: <hostname>'* && $rendered == *'user tour'* &&
   $rendered == *$'```text\nstarted job 4321\nkernel literal\nhostname: literal\n```'* &&
   $rendered == *$'started job <pid>\nstatus 0\nnumber 1234\n```'* &&
   $rendered != *"$tour_scratch"* ]] &&
    cmp -s first-render "$fixture/docs/tour/README.md" &&
    tour_clean; chk tourgen.2

# Both directions of expectation mismatch must preserve the published copy.
printf 'previous publication\n' > saved-readme
cp saved-readme "$fixture/docs/tour/README.md"
tour_deck '```bash +exec:sash
/bin/false
```'
tour_run --render
mismatches_ok=1
[[ $tour_rc = 1 ]] && grep -q MISMATCH tour.out && tour_preserved ||
    mismatches_ok=0
tour_deck '```bash +exec:sash +expect:failure
echo unexpectedly succeeded
```'
tour_run --render
[[ $mismatches_ok = 1 && $tour_rc = 1 ]] &&
    grep -q MISMATCH tour.out && tour_preserved; chk tourgen.3

cp saved-readme "$fixture/docs/tour/README.md"
tour_deck '```bash +exec:sash
echo unfinished'
tour_run --render
malformed_ok=1
[[ $tour_rc = 1 ]] && grep -q unterminated tour.err && tour_preserved ||
    malformed_ok=0
tour_deck 'No runnable snippets.'
tour_run --render
[[ $malformed_ok = 1 && $tour_rc = 1 ]] &&
    grep -q 'no runnable' tour.err && tour_preserved; chk tourgen.4

# An unsupported executor cannot silently vanish alongside a valid snippet.
cp saved-readme "$fixture/docs/tour/README.md"
tour_deck '```bash +exec
echo unsupported
```
```bash +exec:sash
echo supported
```'
tour_run --render
refusals_ok=1
[[ $tour_rc = 1 ]] && grep -q 'unsupported executable fence' tour.err &&
    tour_preserved || refusals_ok=0
tour_run --render -v
[[ $tour_rc = 2 ]] && grep -q 'usage: check' tour.err && tour_preserved ||
    refusals_ok=0
tour_run --unknown
[[ $refusals_ok = 1 && $tour_rc = 2 ]] &&
    grep -q 'usage: check' tour.err && tour_preserved; chk tourgen.5

# A failed final rename is not successful publication, and mv must not place
# the draft inside a directory that happens to be named README.md.
rm "$fixture/docs/tour/README.md"
mkdir "$fixture/docs/tour/README.md"
cp saved-readme "$fixture/docs/tour/README.md/keep"
tour_deck '```bash +exec:sash
echo complete
```'
tour_run --render
[[ $tour_rc != 0 && -s tour.err ]] &&
    cmp -s saved-readme "$fixture/docs/tour/README.md/keep" &&
    [[ $(find "$fixture/docs/tour/README.md" -mindepth 1 -maxdepth 1 | wc -l) = 1 ]] &&
    tour_clean; chk tourgen.6

set -e
tally
