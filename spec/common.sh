SASH_MODE=${SASH_MODE:-native}

setup() {
    local id=$1
    trace=.trace/$(date +%a)/"$id.$SASH_MODE"
    find "${trace%/*}" -maxdepth 0 -mtime +1 -exec rm -r {} + 2>&- || :
    mkdir -p "${trace%/*}"
    exec 3>&1 >> "$trace" 2>&1
    [[ ${SPEC_TSV:-} ]] && exec 4>> "$SPEC_TSV"
    PS4='+$LINENO: '
    set -x
    date
    echo "SASH_MODE=$SASH_MODE"

    # Ensure sash/sash-convert are on PATH even after cd to tmpdir.
    # When SASH_MODE-coverage runs use an alternate sash dir, fall through
    # to the regular target/release for sister tools (sash-convert).
    local regular_release
    regular_release=$(cd "$(dirname "${BASH_SOURCE[0]}")" && realpath ../target/release)
    if [[ ":$PATH:" != *":$regular_release:"* ]]; then
        PATH="$PATH:$regular_release"
    fi

    # Locate libsash_rt.a for --emit-c mode (gcc link target).
    SASH_RT_LIB=$(cd "$(dirname "${BASH_SOURCE[0]}")" && realpath ../sash-rt/target/release/libsash_rt.a 2>/dev/null || echo "")

    local dir
    dir=$(mktemp -d)
    # shellcheck disable=SC2064 # single quotes - false positive
    trap "rm -r '$dir'" EXIT
    cd "$dir"
    declare -gi index=0 fail=0 skip=0
    declare -g rc out err
}

# in_modes "native" — returns 0 if SASH_MODE is in the CSV list,
# 1 otherwise. Use to guard test bodies for mode-specific behavior.
in_modes() {
    case ",$1," in
        *",$SASH_MODE,"*) return 0 ;;
        *) return 1 ;;
    esac
}

# skip_chk <id> — bump index and record SKIP for current mode. Use
# inside the else-branch of an in_modes guard so plan counts stay
# stable across modes.
skip_chk() {
    local id=$1
    (( index++ ))
    (( skip++ ))
    echo "ok $index # SKIP not applicable in $SASH_MODE mode" >&3
    [[ ${SPEC_TSV:-} ]] || return
    printf '%s\t%s\t' "$SASH_MODE:$id" "skipped" >&4
    date -u -Iseconds | sed 's/+.*/Z/' >&4
}

tally() {
    local -i passed=$(( index - fail - skip ))
    if (( fail > 0 )); then
        echo "FAILED ($fail failed, $skip skipped, $passed passed; ran $index)" >&3
        exit 1
    fi
    # Success is fail-based: only an actual `not ok` fails a run. A `plan`
    # mismatch is a visible NOTE, not a failure — `plan` can legitimately
    # diverge from the tap count per mode (`in_modes` guards) or drift when
    # checks are added, and the per-test spec.db records (test_status) are the
    # real source of truth. Keeping it advisory stops stale plans from
    # masquerading as failures (the old `plan != index` FAILED).
    local note=""
    (( plan != index )) && note="  [plan=$plan, ran=$index]"
    if (( skip > 0 )); then
        echo "PASSED ($passed passed, $skip skipped)$note" >&3
    else
        echo "PASSED ($passed passed)$note" >&3
    fi
}

# ── hang guard ────────────────────────────────────────────────────────────
# Every child the harness spawns gets </dev/null and a wall-clock limit.
#
# Without the stdin guard a binary that reads stdin — a compiler bug, or a
# spec that legitimately pins one — inherits the runner's own stdin and
# blocks the whole suite. That is exactly how the DD#14 pipeline-stage
# redirect bug presented: `cat < f | cat` lost the redirect, the stage read
# the terminal, and `bash run.sh` sat there forever with no output.
#
# Without the time limit an infinite loop in the compiler, in gcc, or in
# generated code does the same. Either way the spec must FAIL loudly and let
# the rest of the suite finish, never hang it.
#
# Raise SPEC_TIMEOUT for a deliberately slow spec; it is seconds.
: "${SPEC_TIMEOUT:=10}"

# `timeout` exits 124 when the limit fires. On its own that reads as an
# ordinary bad exit status, so name it.
_hang_msg() {
    echo "spec: $1 exceeded the ${SPEC_TIMEOUT}s hang guard (raise SPEC_TIMEOUT)"
}

# Compile sash source with the current backend, run, capture rc/out/err.
# Modes:
#   native — sash -c "$src" -o ./test_bin; ./test_bin
#   c      — sash --emit-c writes C to stdout; gcc -static + libsash_rt.a
run_sash() {
    rc=-1 out="" err="" rerr=""
    local src=$1; shift
    case $SASH_MODE in
        native) _run_sash_native "$src" "$@" ;;
        c)      _run_sash_c "$src" "$@" ;;
        *) echo "run_sash: unknown SASH_MODE=$SASH_MODE" >&2; exit 1 ;;
    esac
}

# Restrict or fault-inject only the compiled program, never the compiler/linker.
run_sash_under() {
    local src=$1; shift
    local -a _spec_runtime=("$@")
    run_sash "$src"
}

# DD#23/SB5095: the compiler rejects a command path that is not an executable
# file, so a spec can no longer reach the runtime's exec-ENOENT branch simply
# by naming a path that was never there — that program does not compile. The
# branch is still live (a program can be removed, replaced or unmounted after
# the binary is built), so the honest way to exercise it is to be that case:
# stage a real program, let the compiler see it, delete it before the binary
# runs.
#
# The source names it by the literal SPEC_VANISH_CMD, which this substitutes.
# Both backends honour `_spec_vanish`; it is cleared on the way out so an
# ordinary `run_sash` can never trip over it.
run_sash_vanish() {
    local src=$1; shift
    local victim=${TMPDIR:-/tmp}/sash_vanish_$$
    if ! cp /bin/true "$victim" 2>/dev/null || ! chmod 755 "$victim" 2>/dev/null; then
        rc=125 out="" err="spec: could not stage a vanishing command at $victim" rerr=""
        return
    fi
    _spec_vanish=$victim
    run_sash "${src//SPEC_VANISH_CMD/$victim}" "$@"
    _spec_vanish=
    rm -f "$victim"
}

# Remove the staged program, if this is a `run_sash_vanish` call. Called from
# each backend between a successful compile and the run.
_vanish_now() {
    if [[ -n ${_spec_vanish:-} ]]; then
        rm -f "$_spec_vanish"
    fi
}

_run_sash_native() {
    local src=$1; shift
    timeout "$SPEC_TIMEOUT" ${SASH:-sash} ${SASH_CFLAGS:-} -c "$src" -o ./test_bin \
        </dev/null 2> err.raw
    rc=$?
    if [[ $rc = 0 ]]; then
        # Capture runtime stderr separately (rerr) so abort diagnostics
        # (DD#55) can be asserted; err stays for compile diagnostics.
        _vanish_now
        out=$(timeout "$SPEC_TIMEOUT" "${_spec_runtime[@]}" ./test_bin "$@" </dev/null 2>rt.err)
        rc=$?
        rerr=$(cat rt.err 2>/dev/null)
        if (( rc == 124 )); then rerr=$(_hang_msg "the test binary"); fi
        rm -f ./test_bin rt.err
        err=""
    elif (( rc == 124 )); then
        out="" err=$(_hang_msg "the compiler")
    else
        out=""
        err=$(grep -E '^\S+:[0-9]+:[0-9]+:|^sash:|: (ICE|tainted)' err.raw | grep -v 'static binary' || true)
    fi
    rm -f err.raw
}

_run_sash_c() {
    local src=$1; shift
    if [[ -z $SASH_RT_LIB || ! -f $SASH_RT_LIB ]]; then
        rc=125 out="" err="libsash_rt.a not built — run cargo build --release in sash-rt/"
        return
    fi
    printf '%s\n' "$src" > .input.sash
    timeout "$SPEC_TIMEOUT" ${SASH:-sash} ${SASH_CFLAGS:-} --emit-c .input.sash \
        </dev/null > .out.c 2> err.raw
    rc=$?
    if [[ $rc = 0 ]]; then
        timeout "$SPEC_TIMEOUT" gcc -nostartfiles -static -fno-stack-protector \
            -o ./test_bin .out.c "$SASH_RT_LIB" -lpthread </dev/null 2> gcc.err
        local gcc_rc=$?
        if (( gcc_rc != 0 )); then
            # gcc rejecting our emitted C is a compiler bug. Surface it.
            if (( gcc_rc == 124 )); then
                rc=126 out="" err=$(_hang_msg "gcc")
            else
                rc=126 out="" err="gcc failed: $(<gcc.err)"
            fi
        else
            _vanish_now
            out=$(timeout "$SPEC_TIMEOUT" "${_spec_runtime[@]}" ./test_bin "$@" </dev/null 2>rerr.raw)
            rc=$?
            err=""
            rerr=$(<rerr.raw)
            if (( rc == 124 )); then rerr=$(_hang_msg "the test binary"); fi
            rm -f rerr.raw
        fi
        rm -f ./test_bin gcc.err
    elif (( rc == 124 )); then
        out="" err=$(_hang_msg "the compiler")
    else
        out=""
        err=$(grep -E '^\S+:[0-9]+:[0-9]+:|^sash:|: (ICE|tainted)' err.raw | grep -v 'static binary' || true)
    fi
    rm -f .input.sash .out.c err.raw
}

run_convert() {
    rc=-1 out="" err=""
    printf '%s\n' "$1" > .input.sh
    timeout "$SPEC_TIMEOUT" sash-convert .input.sh </dev/null > out 2> err
    rc=$?
    out=$(<out)
    err=$(<err)
}

run_convert_annotate() {
    rc=-1 out="" err=""
    printf '%s\n' "$1" > .input.sh
    timeout "$SPEC_TIMEOUT" sash-convert --annotate .input.sh </dev/null > out 2> err
    rc=$?
    out=$(<out)
    err=$(<err)
}

chk() {
    # shellcheck disable=SC2319 # assign $? - false positive
    local -i last_rc=$? expect=${2:-0}
    local id=$1 msg status
    (( index++ ))
    if (( last_rc == expect )); then
        msg="ok" status="passing"
    else
        msg="not ok" status="failing"
        (( fail++ ))
    fi
    echo "$msg $index" >&3
    [[ ${SPEC_TSV:-} ]] || return
    printf '%s\t%s\t' "$SASH_MODE:$id" "$status" >&4
    date -u -Iseconds | sed 's/+.*/Z/' >&4
}
