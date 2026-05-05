setup() {
    local id=$1
    trace=.trace/$(date +%a)/"$id"
    find "${trace%/*}" -maxdepth 0 -mtime +1 -exec rm -r {} + 2>&- || :
    mkdir -p "${trace%/*}"
    exec 3>&1 >> "$trace" 2>&1
    [[ ${SPEC_TSV:-} ]] && exec 4>> "$SPEC_TSV"
    PS4='+$LINENO: '
    set -x
    date

    # Ensure sash/sash-convert are on PATH even after cd to tmpdir
    if [[ ":$PATH:" != *"/target/"* ]]; then
        PATH="$PATH:$(realpath "${BASH_SOURCE[0]%/*}/../target/release")"
    fi

    local dir
    dir=$(mktemp -d)
    # shellcheck disable=SC2064 # single quotes - false positive
    trap "rm -r '$dir'" EXIT
    cd "$dir"
    declare -gi index=0 fail=0
    declare -g rc out err
}

tally() {
    if (( fail > 0 || plan != index )); then
        echo FAILED >&3
        exit 1
    fi
    echo PASSED >&3
}

run_sash() {
    rc=-1 out="" err=""
    sash -c "$1" > out 2> err
    rc=$?
    out=$(<out)
    err=$(<err)
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
    printf '%s\t%s\t' "$id" "$status" >&4
    date -u -Iseconds | sed 's/+.*/Z/' >&4
}
