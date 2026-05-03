setup() {
    local id=$1
    trace=.trace/$(date +%a)/"$id"
    find ${trace%/*} -maxdepth 0 -mtime +1 -exec rm -r {} + 2>&- || :
    mkdir -p ${trace%/*}
    exec 3>&1 >> $trace 2>&1 4>>${SPEC_TSV:-spec.tsv}
    PS4='+$LINENO: '
    set -x
    date

    local dir
    dir=$(mktemp -d)
    trap "rm -r '$dir'" EXIT
    cd $dir
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

chk() {
    local -i last_rc=$? expect=${2:-0}
    local id=$1 msg
    (( index++ ))
    if (( last_rc == expect )); then
        msg="ok"
    else
        msg="not ok"
        (( fail++ ))
    fi
    echo "$msg $index" >&3
    printf '%s\t%s\t' "$id" "$msg" >&4
    date -u -Iseconds | sed 's/+.*/Z/' >&4
}
