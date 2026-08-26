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

# in_modes "native,janet,c" — returns 0 if SASH_MODE is in the CSV list,
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
    if (( fail > 0 || plan != index )); then
        echo "FAILED ($fail failed, $skip skipped, $((index - fail - skip)) passed of plan=$plan)" >&3
        exit 1
    fi
    if (( skip > 0 )); then
        echo "PASSED ($skip skipped of $plan)" >&3
    else
        echo PASSED >&3
    fi
}

# Compile sash source with the current backend, run, capture rc/out/err.
# Modes:
#   native — sash -c "$src" -o ./test_bin; ./test_bin
#   janet  — sash --janet writes Janet to stdout; pipe to janet
#   c      — sash --emit-c writes C to stdout; gcc -static + libsash_rt.a
run_sash() {
    rc=-1 out="" err="" rerr=""
    local src=$1; shift
    case $SASH_MODE in
        native) _run_sash_native "$src" "$@" ;;
        janet)  _run_sash_janet "$src" "$@" ;;
        c)      _run_sash_c "$src" "$@" ;;
        *) echo "run_sash: unknown SASH_MODE=$SASH_MODE" >&2; exit 1 ;;
    esac
}

_run_sash_native() {
    local src=$1; shift
    sash ${SASH_CFLAGS:-} -c "$src" -o ./test_bin 2> err.raw
    rc=$?
    if [[ $rc = 0 ]]; then
        # Capture runtime stderr separately (rerr) so abort diagnostics
        # (DD#55) can be asserted; err stays for compile diagnostics.
        out=$(./test_bin "$@" 2>rt.err)
        rc=$?
        rerr=$(cat rt.err 2>/dev/null)
        rm -f ./test_bin rt.err
        err=""
    else
        out=""
        err=$(grep -E '^\S+:[0-9]+:[0-9]+:|^sash:|: (ICE|tainted)' err.raw | grep -v 'static binary' || true)
    fi
    rm -f err.raw
}

_run_sash_janet() {
    local src=$1; shift
    printf '%s\n' "$src" > .input.sash
    sash ${SASH_CFLAGS:-} --janet .input.sash > .out.janet 2> err.raw
    rc=$?
    if [[ $rc = 0 ]]; then
        out=$(janet .out.janet "$@" 2>/dev/null)
        rc=$?
        err=""
    else
        out=""
        err=$(grep -E '^\S+:[0-9]+:[0-9]+:|^sash:|: (ICE|tainted)' err.raw | grep -v 'static binary' || true)
    fi
    rm -f .input.sash .out.janet err.raw
}

_run_sash_c() {
    local src=$1; shift
    if [[ -z $SASH_RT_LIB || ! -f $SASH_RT_LIB ]]; then
        rc=125 out="" err="libsash_rt.a not built — run cargo build --release in sash-rt/"
        return
    fi
    printf '%s\n' "$src" > .input.sash
    sash ${SASH_CFLAGS:-} --emit-c .input.sash > .out.c 2> err.raw
    rc=$?
    if [[ $rc = 0 ]]; then
        gcc -nostartfiles -static -fno-stack-protector -o ./test_bin .out.c "$SASH_RT_LIB" -lpthread 2> gcc.err
        local gcc_rc=$?
        if (( gcc_rc != 0 )); then
            # gcc rejecting our emitted C is a compiler bug. Surface it.
            rc=126 out="" err="gcc failed: $(<gcc.err)"
        else
            out=$(./test_bin "$@" 2>rerr.raw)
            rc=$?
            err=""
            rerr=$(<rerr.raw)
            rm -f rerr.raw
        fi
        rm -f ./test_bin gcc.err
    else
        out=""
        err=$(grep -E '^\S+:[0-9]+:[0-9]+:|^sash:|: (ICE|tainted)' err.raw | grep -v 'static binary' || true)
    fi
    rm -f .input.sash .out.c err.raw
}

run_convert() {
    rc=-1 out="" err=""
    printf '%s\n' "$1" > .input.sh
    sash-convert .input.sh > out 2> err
    rc=$?
    out=$(<out)
    err=$(<err)
}

run_convert_annotate() {
    rc=-1 out="" err=""
    printf '%s\n' "$1" > .input.sh
    sash-convert --annotate .input.sh > out 2> err
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
