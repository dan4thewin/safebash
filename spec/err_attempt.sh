#!/bin/bash
# Provisional DD136: ERR owns whole-call recovery, not helper-local returns.
. common.sh
plan=67
setup err_attempt

set +e

run_sash 'fn leaf { echo entered; fail 7; echo lost-leaf }
fn outer { leaf; echo lost-outer }
ERR { : outer; echo resumed }
fail 9'
[[ $rc = 9 && $out = $'entered\nresumed' && -z $err && -z $rerr ]]; chk errtry.1

run_sash 'fn zero { if [[ $1 == fail ]] { fail 0 }; return 0 }
fn full { return 256 }
ERR {
    if zero fail { echo wrong } else { echo "abnormal:$?" }
    if full { echo wrong } else { echo "normal:$?" }
}
fail 9'
[[ $rc = 9 && $out = $'abnormal:0\nnormal:256' && -z $err && -z $rerr ]]; chk errtry.2

run_sash 'fn leaf { fail 7; return 0 }
fn outer { if leaf { echo lost-true } else { echo lost-false }; echo lost-tail }
ERR { : outer; echo resumed }
fail 9'
[[ $rc = 9 && $out = resumed && -z $err && -z $rerr ]]; chk errtry.3

run_sash 'fn predicate { if [[ $1 == fail ]] { fail 7 }; return 0 }
ERR { - predicate fail; echo lost }
fail 9'
[[ $rc = 7 && -z $out && -z $err && -z $rerr ]]; chk errtry.4

run_sash 'fn action { fail 7 }
ERR { action; echo lost }
fail 9'
[[ $rc = 7 && -z $out && -z $err && -z $rerr ]]; chk errtry.5

run_sash 'fn predicate { fail 7; return 0 }
if predicate { echo lost-true } else { echo lost-false }
ERR { echo terminal }'
[[ $rc = 7 && $out = terminal && -z $err && -z $rerr ]]; chk errtry.6

run_sash 'fn action { fail 7 }
ERR { : action; echo resumed; fail 8; echo lost }
fail 9'
[[ $rc = 8 && $out = resumed && -z $err && -z $rerr ]]; chk errtry.7

run_sash 'fn number -> int { changed=yes; fail 0; return 2 }
fn action { echo "lost:$1" }
ERR { str changed=no; : action "$(number)"; echo "$changed" }
fail 9'
[[ $rc = 9 && $out = yes && -z $err && -z $rerr ]]; chk errtry.8

run_sash 'fn action { echo body }
ERR { : action >missing/output; : action; echo resumed }
fail 9'
[[ $rc = 9 && $out = $'body\nresumed' && -z $err && -z $rerr ]]; chk errtry.9

run_sash 'fn action { echo written; fail 7 }
ERR { : action >saved; echo resumed; /bin/cat saved }
fail 9'
[[ $rc = 9 && $out = $'resumed\nwritten' && -z $err && -z $rerr ]]; chk errtry.10

run_sash 'fn action { exec 8>saved; echo committed >&8; fail 7 }
ERR { : action; echo retained >&8; close 8; /bin/cat saved }
fail 9'
[[ $rc = 9 && $out = $'committed\nretained' && -z $err && -z $rerr ]]; chk errtry.11

run_sash 'fn action { fail 7 }
ERR {
    exec 8>saved
    close 8
    : action 8>saved
    if close 8 { echo wrong } else { echo closed }
}
fail 9'
[[ $rc = 9 && $out = closed && -z $err && -z $rerr ]]; chk errtry.12

run_sash 'fn action {
    /usr/bin/yes inner | while readline word { _=${word:!}; fail 7 }
}
ERR {
    /usr/bin/printf "one\ntwo\n" | while readline row {
        : action
        echo "${row:!}"
    }
}
fail 9'
[[ $rc = 9 && $out = $'one\ntwo' && -z $err && -z $rerr ]]; chk errtry.13

run_sash 'fn leaf { echo hidden; fail 7 }
fn predicate { echo partial; str value=$(leaf); echo "lost:$value"; return 0 }
ERR {
    if text=$(predicate) { echo wrong } else { echo "[${text:!}]:$?" }
}
fail 9'
[[ $rc = 9 && $out = '[partial]:7' && -z $err && -z $rerr ]]; chk errtry.14

run_sash 'fn predicate { echo err >&2; echo out; if [[ $1 == fail ]] { fail 0 }; return 0 }
ERR {
    if text=$(predicate fail 2>&1) { echo wrong } else { echo "[${text:!}]:$?" }
}
fail 9'
[[ $rc = 9 && $out = $'[err\nout]:0' && -z $err && -z $rerr ]]; chk errtry.15

run_sash 'fn zero { return 0 }
fn full { return 256 }
ERR {
    if ! full { echo "yes:$?" } else { echo wrong }
    if ! zero { echo wrong } else { echo "no:$?" }
}
fail 9'
[[ $rc = 9 && $out = $'yes:0\nno:1' && -z $err && -z $rerr ]]; chk errtry.16

run_sash 'fn predicate { echo partial; if [[ $1 == fail ]] { fail 0 }; return 0 }
ERR {
    if ! text=$(predicate fail) { echo "[${text:!}]:$?" } else { echo wrong }
}
fail 9'
[[ $rc = 9 && $out = '[partial]:0' && -z $err && -z $rerr ]]; chk errtry.17

run_sash 'fn change_dir { cd /sash-err-attempt-missing; echo lost }
fn close_fd { close 9; echo lost }
ERR { : change_dir; : close_fd; echo resumed }
fail 9'
[[ $rc = 9 && $out = resumed && -z $err && -z $rerr ]]; chk errtry.18

run_sash 'fn action { echo failed >/dev/full; echo lost }
ERR { : action; echo resumed }
fail 9'
[[ $rc = 9 && $out = resumed && -z $err && -z $rerr ]]; chk errtry.19

run_sash 'fn action { glob files "missing/*"; echo "lost:$(len files)" }
ERR { : action; echo resumed }
fail 9'
[[ $rc = 9 && $out = resumed && -z $err && -z $rerr ]]; chk errtry.20

PROBE_ENV=bad run_sash 'fn action {
    value=$(- getenv PROBE_ENV)
    attest value [[ $value == good ]]
    echo "lost:$value"
}
ERR { : action; echo resumed }
fail 9'
[[ $rc = 9 && $out = resumed && -z $err && -z $rerr ]]; chk errtry.21

run_sash 'fn action {
    array values=(bad good)
    attest value in values [[ $value == good ]]
    echo "lost:$(len values)"
}
ERR { : action; echo resumed }
fail 9'
[[ $rc = 9 && $out = resumed && -z $err && -z $rerr ]]; chk errtry.22

run_sash 'fn action { /bin/true | /bin/false; echo lost }
ERR { : action; echo resumed }
fail 9'
[[ $rc = 9 && $out = resumed && -z $err && -z $rerr ]]; chk errtry.23

run_sash 'fn child { echo child; fail 4; echo lost }
fn action { child | /bin/cat; echo lost }
ERR { : action; echo resumed }
fail 9'
[[ $rc = 9 && $out = $'child\nresumed' && -z $err && -z $rerr ]]; chk errtry.24

run_sash 'fn child { echo child; fail 4; echo lost }
fn action { child &; - wait; echo "waited:$(waitstatus)" }
ERR { : action; echo resumed }
fail 9'
[[ $rc = 9 && $out = $'child\nwaited:4\nresumed' && -z $err && -z $rerr ]]; chk errtry.25

run_sash 'fn action { exit 0; echo lost }
EXIT { echo wrong-handler }
ERR { : action; echo lost }
fail 9'
[[ $rc = 0 && -z $out && -z $err && -z $rerr ]]; chk errtry.26

run_sash 'fn action { str value=""; _=${value:?required}; echo lost }
ERR { : action; echo lost }
fail 9'
[[ $rc = 1 && -z $out && -z $err && $rerr == *": required"* ]]; chk errtry.27

run_sash 'fn action { - /bin/echo $'\''a\x00b'\''; echo lost }
ERR { : action; echo lost }
fail 9'
[[ $rc = 1 && -z $out && -z $err && $rerr == *"argument contains a NUL byte"* ]]; chk errtry.28

run_sash 'fn action { int index=-1; _=$(- arg $index); echo lost }
ERR { : action; echo lost }
fail 9'
[[ $rc = 1 && -z $out && -z $err && $rerr == *"arg"* ]]; chk errtry.29

run_sash 'fn action { _=$(<missing-input); echo lost }
ERR { : action; echo lost }
fail 9'
[[ $rc = 1 && -z $out && -z $err && $rerr == *"missing-input"* ]]; chk errtry.30

run_sash 'fn action { read _ <.; echo lost }
ERR { : action; echo lost }
fail 9'
[[ $rc = 1 && -z $out && -z $err && $rerr == *"read"* ]]; chk errtry.31

run_sash 'fn action { int zero=0; _=$(( 1 / zero )); echo lost }
ERR { : action; echo lost }
fail 9'
[[ $rc = 2 && -z $out && -z $err && $rerr == *"division by zero"* ]]; chk errtry.32

source='fn action { /bin/true; echo body }
ERR { echo entered; : action; echo resumed }
fail 9'
run_sash_under "$source" strace -qq -o fork-ok.trace -e trace=clone
control=0
[[ $rc = 9 && $out = $'entered\nbody\nresumed' && -z $rerr ]] &&
    grep -q 'clone(.* = [0-9]' fork-ok.trace && control=1
run_sash_under "$source" strace -qq -o fork-fail.trace -e trace=clone \
    -e inject=clone:error=EAGAIN:when=1
[[ $control = 1 && $rc = 1 && $out = entered && $rerr == *"fork"* ]] &&
    grep -q 'clone(.*EAGAIN.*INJECTED' fork-fail.trace; chk errtry.33

source='fn action { str dir=$(mkdtemp); /bin/rmdir "$dir"; echo made }
ERR { : action; echo resumed }
fail 9'
run_sash_under "$source" strace -qq -o mkdir-ok.trace -e trace=mkdirat
control=0
[[ $rc = 9 && $out = $'made\nresumed' && -z $rerr ]] &&
    grep -q 'mkdirat(.* = 0' mkdir-ok.trace && control=1
run_sash_under "$source" strace -qq -o mkdir-fail.trace -e trace=mkdirat \
    -e inject=mkdirat:error=ENOSPC:when=1
[[ $control = 1 && $rc = 9 && $out = resumed && -z $err && -z $rerr ]] &&
    grep -q 'mkdirat(.*ENOSPC.*INJECTED' mkdir-fail.trace; chk errtry.34

source='fn action { fail 7 }
ERR { : action >saved; echo resumed }
fail 9'
run_sash_under "$source" strace -qq -o restore-ok.trace -e trace=dup2
control=0
[[ $rc = 9 && $out = resumed && -z $rerr &&
   $(grep -c '^dup2(' restore-ok.trace) = 5 &&
   $(grep -c '^dup2(.* = 1$' restore-ok.trace) = 3 ]] && control=1
run_sash_under "$source" strace -qq -o restore-fail.trace -e trace=dup2 \
    -e inject=dup2:error=EIO:when=5
[[ $control = 1 && $rc = 1 && -z $out && $rerr == *"restore fd"* ]] &&
    grep -q '^dup2(.*EIO.*INJECTED' restore-fail.trace; chk errtry.35

dd if=/dev/zero of=large-input bs=65536 count=1 status=none
source='fn action { _=$(<large-input) }
ERR { echo entered; : action; echo resumed }
fail 9'
SASH_CFLAGS=--heap-size=262144 run_sash "$source"
control=0
[[ $rc = 9 && $out = $'entered\nresumed' && -z $rerr ]] && control=1
SASH_CFLAGS=--heap-size=4096 run_sash "$source"
[[ $control = 1 && $rc = 1 && $out = entered &&
   $rerr == *"out of memory (requested 8192 bytes)"* ]]; chk errtry.36

run_sash_under 'fn action {
    /usr/bin/yes outer | while readline x {
        _=${x:!}
        /usr/bin/yes inner | while readline y { _=${y:!}; fail 7 }
    }
}
ERR { : action; int pid=$(wait); if (( pid < 0 )) { echo reaped } }
fail 9' strace -qq -o reader.trace -e trace=pipe2,close,wait4
[[ $rc = 9 && $out = reaped && -z $err && -z $rerr ]] &&
python3 - <<'PY'
import re
from pathlib import Path
lines = Path("reader.trace").read_text().splitlines()
readers = [int(m[1]) for line in lines
           if (m := re.search(r"pipe2\(\[(\d+), \d+\]", line))]
wait = next(i for i, line in enumerate(lines) if line.startswith("wait4("))
closed = {int(m[1]) for line in lines[:wait]
          if (m := re.search(r"close\((\d+)\)\s+= 0$", line))}
assert len(readers) == 2 and all(fd in closed for fd in readers)
PY
chk errtry.37

run_sash 'fn predicate { /bin/false || echo locally-handled; return 0 }
ERR { if predicate { echo normal } else { echo wrong } }
fail 9'
[[ $rc = 9 && $out = $'locally-handled\nnormal' && -z $err && -z $rerr ]]; chk errtry.38

run_sash 'fn predicate { if [[ $1 == fail ]] { fail 7 }; return 0 }
ERR { predicate fail || :; echo resumed }
fail 9'
[[ $rc = 9 && $out = resumed && -z $err && -z $rerr ]]; chk errtry.39

run_sash 'fn predicate { if [[ $1 == fail ]] { fail 0 }; return 0 }
ERR { predicate fail && echo lost; echo resumed }
fail 9'
[[ $rc = 9 && $out = resumed && -z $err && -z $rerr ]]; chk errtry.40

bad=0
for source in \
    'fn action { : }; : action' \
    'fn action { : }; EXIT { : action }' \
    'fn action { : }; fn helper { : action }; ERR { helper }' \
    'fn action { : }; ERR { if : action { : } }' \
    'fn action { : }; ERR { : action | /bin/cat }' \
    'fn action { : }; ERR { : action & }'; do
    run_sash "$source"
    [[ $err == *SB5108* ]] || ((bad++))
done
[[ $bad = 0 ]]; chk errtry.41

bad=0
for source in \
    'ERR { : unknown }' \
    'ERR { : /bin/true }' \
    'fn predicate { return 0 }; ERR { : predicate }' \
    'fn number -> int { return 2 }; ERR { : number }' \
    'fn action { : }; ERR { name=action; : $name }'; do
    run_sash "$source"
    [[ $err == *SB5108* ]] || ((bad++))
done
[[ $bad = 0 ]]; chk errtry.42

run_sash 'fn action { echo $? }
ERR { : action }
fail 9'
[[ $err == *SB5012* ]]; chk errtry.43

run_sash 'fn action { state=changed }
str state=initial
ERR { : action; echo "$state" }
fail 9'
[[ $err == *SB5102* && $err == *"state"* ]]; chk errtry.44

run_sash 'ERR { if : { echo noop }; : >empty; echo kept }
fail 9'
[[ $rc = 9 && $out = $'noop\nkept' && -f empty && ! -s empty &&
   -z $err && -z $rerr ]]; chk errtry.45

# The first stage cannot exit while the parent retains the pipe, so the
# second clone cannot be a SIGCHLD-restarted first clone.
pipeline_fork_cleanup() {
    python3 - "$1" <<'PY'
import re
import sys
from pathlib import Path
lines = Path(sys.argv[1]).read_text().splitlines()
failed = next(i for i, line in enumerate(lines)
              if line.startswith("clone(") and "EAGAIN" in line and "INJECTED" in line)
pipes = [int(fd) for line in lines[:failed]
         if (m := re.search(r"pipe2\(\[(\d+), (\d+)\].* = 0$", line))
         for fd in m.groups()]
children = [int(m[1]) for line in lines[:failed]
            if (m := re.search(r"^clone\(.*\)\s+= (\d+)$", line))]
waits = [i for i, line in enumerate(lines) if i > failed and line.startswith("wait4(")]
assert len(pipes) == 2 and len(children) == 1, lines
assert waits, ("started stage was not reaped", lines)
closed = {int(m[1]) for line in lines[failed + 1:waits[0]]
          if (m := re.search(r"^close\((\d+)\)\s+= 0$", line))}
assert set(pipes) <= closed, ("pipeline fds must close before waiting", lines)
pid = children[0]
reaped = [i for i in waits
          if re.search(rf"^wait4\({pid},.*\)\s+= {pid}$", lines[i])]
output = next(i for i, line in enumerate(lines) if i > failed and line.startswith("write("))
assert len(reaped) == 1 and reaped[0] < output, ("cleanup must precede fatal dispatch", lines)
PY
}

source='ERR { echo "ERR:$?" }
- /bin/cat /dev/zero | /usr/bin/head -c 1 >/dev/null
echo after'
run_sash_under "$source" strace -qq -o partial-main-ok.trace -e trace=clone
control=0
[[ $rc = 0 && $out = after && -z $err && -z $rerr &&
   $(grep -c '^clone(.* = [0-9]' partial-main-ok.trace) = 2 ]] && control=1
run_sash_under "$source" strace -qq -o partial-main-fail.trace \
    -e trace=pipe2,clone,close,wait4,write -e inject=clone:error=EAGAIN:when=2
[[ $control = 1 && $rc = 1 && $out = ERR:1 && -z $err &&
   $rerr = "./test_bin: fork: error 11" ]] &&
    pipeline_fork_cleanup partial-main-fail.trace; chk errtry.46

source='fn action {
    - /bin/cat /dev/zero | /usr/bin/head -c 1 >/dev/null
    echo body
}
ERR { : action; echo resumed }
fail 9'
run_sash_under "$source" strace -qq -o partial-err-ok.trace -e trace=clone
control=0
[[ $rc = 9 && $out = $'body\nresumed' && -z $err && -z $rerr &&
   $(grep -c '^clone(.* = [0-9]' partial-err-ok.trace) = 2 ]] && control=1
run_sash_under "$source" strace -qq -o partial-err-fail.trace \
    -e trace=pipe2,clone,close,wait4,write -e inject=clone:error=EAGAIN:when=2
[[ $control = 1 && $rc = 1 && -z $out && -z $err &&
   $rerr = "./test_bin: fork: error 11" ]] &&
    pipeline_fork_cleanup partial-err-fail.trace; chk errtry.47

# Isolate the front-end and the independent SSA checker for each refusal.
refuse_taint() {
    local source=$1 code=$2 front
    "${SASH:-sash}" --emit-ir -c "$source" >/dev/null 2>front.err
    front=$?
    [[ $front != 0 && $(<front.err) == *"$code"* ]] || return 1
    SASH_CFLAGS=--no-taint-cst run_sash "$source"
    [[ $rc != 0 && $err == *"SSA taint backstop"* ]]
}

refuse_taint 'fn action { value=$(- getenv PROBE_ENV); attest value [[ $value == good ]] }
ERR { str value=clean; : action; echo "$value" }
fail 9' SB5020; chk errtaint.1

refuse_taint 'fn predicate {
    value=$(- getenv PROBE_ENV); attest value [[ $value == good ]]; return 0
}
ERR { str value=clean; if predicate { : } else { echo "$value" } }
fail 9' SB5020; chk errtaint.2

refuse_taint 'fn predicate {
    value=$(- getenv PROBE_ENV); attest value [[ $value == good ]]; return 0
}
ERR { str value=clean; if ! predicate { echo "$value" } }
fail 9' SB5020; chk errtaint.3

refuse_taint 'fn action { /bin/false; value=$(- getenv PROBE_ENV) }
ERR {
    value=$(- getenv PROBE_ENV)
    : action
    attest value [[ $value == good ]]
    echo "$value"
}
fail 9' SB5088; chk errtaint.4

refuse_taint 'fn action { argv A }
ERR {
    array values
    argv values
    : action =values
    attest item in values p?
    echo "$(join values)"
}
fail 9' SB5088; chk errtaint.5

refuse_taint 'fn action { argv A; attest item in A [[ $item == good ]] }
ERR { array values; : action =values; echo "$(join values)" }
fail 9' SB5020; chk errtaint.6

PROBE_ENV=good run_sash 'fn action { value=$(- getenv PROBE_ENV); fail 7 }
ERR {
    value=$(- getenv PROBE_ENV)
    : action
    attest value [[ $value == good ]]
    echo "$value"
}
fail 9'
[[ $rc = 9 && $out = good && -z $err && -z $rerr ]]; chk errtaint.7

run_sash 'fn action { value=$(- getenv PROBE_ENV); attest value [[ $value == good ]] }
ERR { str value=clean; : action; : action; echo "${value:!}" }
fail 9'
[[ $err == *SB5067* ]]; chk errtaint.8

refuse_taint 'fn action { value=$(- getenv PROBE_ENV); fail 7 }
ERR {
    str value=clean; int n=0
    while (( n < 2 )) { echo "$value"; (( n+=1 )); : action; continue }
}
fail 9' SB5020; chk errtaint.9

refuse_taint 'fn action { value=$(- getenv PROBE_ENV); fail 7 }
ERR {
    str value=clean; array rows=(one two)
    for row in @rows { _=$row; echo "$value"; : action; continue }
}
fail 9' SB5020; chk errtaint.10

refuse_taint 'fn action { value=$(- getenv PROBE_ENV); fail 7 }
ERR {
    str value=clean
    for row in one two { _=$row; echo "$value"; : action; continue }
}
fail 9' SB5020; chk errtaint.11

refuse_taint 'fn action { value=$(- getenv PROBE_ENV); fail 7 }
ERR {
    str value=clean; int n=0
    while (( n < 2 )) { (( n+=1 )); : action; break }
    echo "$value"
}
fail 9' SB5020; chk errtaint.12

PROBE_ENV=good run_sash 'fn predicate {
    value=$(- getenv PROBE_ENV); attest value [[ $value == good ]]; return 0
}
ERR {
    str value=clean; array rows=(one two); array columns=(three four)
    for row in @rows {
        for column in @columns {
            if predicate { echo "$row:$column:$value"; continue 2 } else { echo "${value:!}" }
        }
    }
}
fail 9'
[[ $rc = 9 && $out = $'one:three:good\ntwo:three:good' &&
   -z $err && -z $rerr ]]; chk errtaint.13

PROBE_ENV=good run_sash 'fn predicate {
    value=$(- getenv PROBE_ENV); attest value [[ $value == good ]]; return 0
}
ERR {
    str value=clean; int n=0
    while (( n < 2 )) {
        (( n+=1 ))
        /usr/bin/printf "row\n" | while readline row {
            _=${row:!}
            if predicate { echo "$value"; continue 2 } else { echo "${value:!}" }
        }
    }
}
fail 9'
[[ $rc = 9 && $out = $'good\ngood' && -z $err && -z $rerr ]]; chk errtaint.14

RE='^x$' run_sash 'fn action { fail 7 }
str input=$(- getenv RE)
ERR {
    str _pattern=${input:!}
    attest _pattern r?
    : action
    if match x "$_pattern" { echo valid }
}
fail 9'
[[ $rc = 9 && $out = valid && -z $err && -z $rerr ]]; chk errtaint.15

SASH_CFLAGS=--no-taint run_sash 'fn action { /bin/false; attest _pattern r? }
str input=$(- getenv RE)
ERR { str _pattern=${input:!}; : action; - match x "$_pattern" }
fail 9'
[[ $err == *SB5106* ]]; chk errtaint.16

RE='^x$' run_sash 'fn predicate { attest _pattern r?; return 0 }
str input=$(- getenv RE)
ERR { str _pattern=${input:!}; if predicate { if match x "$_pattern" { echo valid } } }
fail 9'
[[ $rc = 9 && $out = valid && -z $err && -z $rerr ]]; chk errtaint.17

SASH_CFLAGS=--no-taint run_sash 'fn predicate { attest _pattern r?; return 0 }
str input=$(- getenv RE)
ERR { str _pattern=${input:!}; if predicate { : } else { - match x "$_pattern" } }
fail 9'
[[ $err == *SB5106* ]]; chk errtaint.18

PROBE_ENV=good PROBE_OLD=old run_sash 'fn predicate {
    value=$(- getenv PROBE_ENV)
    if [[ $1 == fail ]] { fail 7 }
    return 0
}
ERR {
    value=$(- getenv PROBE_OLD)
    if predicate fail { echo wrong } else {
        attest value [[ $value == good ]]
        echo "$value"
    }
}
fail 9'
[[ $rc = 9 && $out = good && -z $err && -z $rerr ]]; chk errtaint.19

refuse_taint 'fn predicate {
    /bin/false; value=$(- getenv PROBE_ENV); return 0
}
ERR {
    value=$(- getenv PROBE_ENV)
    if predicate { : } else {
        attest value [[ $value == good ]]
        echo "$value"
    }
}
fail 9' SB5088; chk errtaint.20

set -e
tally
