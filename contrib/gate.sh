#!/bin/bash
# The spec gate in one command (`mk gate`). It runs the checks that AGENTS.md's
# "Spec tests" section describes and exits nonzero on any mismatch.
#
#   static  duplicate chk ids, variable-id collisions, plan vs chk per file,
#           and the SB census in both directions
#   sweep   one full spec/run.sh in both modes, recorded as a block of rowids
#   db      the block against the plan sums (total and per mode); no failing
#           row; flips, first-ever ids and retirements against the predictions;
#           no ghost registry row; and a liveness check proving the flip
#           comparison can report something at all
#
# Predictions come from the environment. Each defaults to 0; predict them
# before the sweep, because a count you did not predict is a finding.
#   NEW=n      ids with no earlier row: your new specs (+2N in a dual-mode file)
#   FLIPS=n    ids whose status differs from their previous row
#   RETIRED=n  registered ids the sweep no longer produces. The purge cap is set
#              to exactly this, so an unpredicted retirement deletes nothing.
#   TOTAL=n    checks in the sweep (default: the plan-derived total)
#
#   bash contrib/gate.sh            static, then sweep, then db
#   bash contrib/gate.sh --static   static only (seconds)
#   bash contrib/gate.sh --recheck  db only, against the last gate sweep's block
#                                   (target/gate/state), e.g. after correcting
#                                   a prediction
#
# `mk gate` builds and runs the unit tests first; this script does neither.
# Exit status: 0 pass, 1 a check failed, 2 usage.
set -u
cd "$(dirname "$0")/.."

mode=full
case ${1:-} in
    '') ;;
    --static) mode=static ;;
    --recheck) mode=recheck ;;
    *) echo "usage: $0 [--static|--recheck]" >&2; exit 2 ;;
esac

count() {
    [[ $2 =~ ^[0-9]+$ ]] || { echo "gate: $1 must be a count, got '$2'" >&2; exit 2; }
}
new=${NEW:-0} flips=${FLIPS:-0} retired=${RETIRED:-0}
count NEW "$new"; count FLIPS "$flips"; count RETIRED "$retired"
[[ -n ${TOTAL:-} ]] && count TOTAL "$TOTAL"

state=target/gate
mkdir -p "$state"
failed=0
pass() { printf 'PASS  %s\n' "$1"; }
# check LABEL ACTUAL EXPECTED [DETAIL]: DETAIL (one item per line) is shown
# only on failure, capped at 40 lines.
check() {
    if [[ $2 == "$3" ]]; then
        pass "$1: $2"
    else
        printf 'FAIL  %s: %s, expected %s\n' "$1" "$2" "$3"
        failed=$((failed + 1))
        [[ -n ${4:-} ]] && printf '%s\n' "$4" | head -40 | sed 's/^/        /'
    fi
}
lines() { printf '%s\n' "$1" | grep -c .; }

files=()
for f in spec/*.sh; do
    case ${f##*/} in common.sh | run.sh) continue ;; esac
    files+=("$f")
done

static_checks() {
    local dups vcol bad="" f p n v k src spc
    dups=$(grep -ho '\bchk [A-Za-z0-9._]*' spec/*.sh | sort | uniq -d)
    check "duplicate chk ids" "$(lines "$dups")" 0 "$dups"

    # A variable id (coverage.sh's loop) is invisible to the duplicate check.
    vcol=$(comm -12 <(grep -hoE '_id=[A-Za-z0-9._]+' spec/*.sh | sed 's/_id=//' | sort -u) \
        <(grep -ho '\bchk [A-Za-z0-9._]\+' spec/*.sh | sed 's/chk //' | sort -u))
    check "variable-id collisions" "$(lines "$vcol")" 0 "$vcol"

    # plan= is per mode. A `chk $var` token runs once per distinct _id= value
    # in its file, which is what makes coverage.sh agree without an exemption.
    for f in "${files[@]}"; do
        p=$(grep -m1 '^plan=' "$f" | sed 's/^plan=//')
        n=$(grep -ho '\bchk [A-Za-z0-9._$]\+' "$f" | grep -c .)
        v=$(grep -ho '\bchk \$' "$f" | grep -c .)
        k=$(grep -hoE '_id=[A-Za-z0-9._]+' "$f" | sort -u | grep -c .)
        n=$((n - v + (v > 0 ? k : 0)))
        [[ $p == "$n" ]] || bad+="${f##*/} plan=$p chk=$n"$'\n'
    done
    bad=${bad%$'\n'}
    check "files whose plan= disagrees with their chk count" "$(lines "$bad")" 0 "$bad"

    # SB census: a code is covered only if a spec names it. Comments do not count.
    src=$(grep -rhE 'SB[0-9]{4}' sash/src/ | grep -vE '^[[:space:]]*//' | grep -oE 'SB[0-9]{4}' | sort -u)
    spc=$(grep -hE 'SB[0-9]{4}' spec/*.sh | grep -vE '^[[:space:]]*#' | grep -oE 'SB[0-9]{4}' | sort -u)
    v=$(comm -23 <(printf '%s\n' "$src") <(printf '%s\n' "$spc"))
    check "SB codes raised but asserted by no spec" "$(lines "$v")" 0 "$v"
    v=$(comm -13 <(printf '%s\n' "$src") <(printf '%s\n' "$spc"))
    check "SB codes asserted but raised nowhere" "$(lines "$v")" 0 "$v"
}

# Plan sums. Native runs every file; c skips the native-only ones, which are
# the files carrying the native-only guard.
plan_sums() {
    local f p
    nat=0 cm=0
    for f in "${files[@]}"; do
        p=$(grep -m1 '^plan=' "$f" | sed 's/^plan=//')
        [[ $p =~ ^[0-9]+$ ]] || { echo "gate: ${f##*/} has no numeric plan=" >&2; exit 2; }
        nat=$((nat + p))
        grep -q 'SASH_MODE:-native} != native' "$f" || cm=$((cm + p))
    done
}

sweep() {
    local t0 rc
    sqlite3 spec.db < spec/schema.sql
    b0=$(sqlite3 spec.db 'SELECT COALESCE(MAX(rowid), 0) FROM runs')
    sqlite3 spec.db 'SELECT id FROM tests ORDER BY id' > "$state/tests-before"
    echo "sweep: spec/run.sh, both modes (log: $state/sweep.log)"
    t0=$SECONDS
    # Unset anything that would point the sweep at another binary or a subset.
    env -u SASH -u SASH_RELEASE_DIR -u SPEC_MODES -u SASH_MODE -u SPEC_TSV \
        SPEC_PURGE_MAX="$retired" timeout "${GATE_SWEEP_TIMEOUT:-3600}" \
        bash spec/run.sh > "$state/sweep.log" 2>&1
    rc=$?
    b1=$(sqlite3 spec.db 'SELECT COALESCE(MAX(rowid), 0) FROM runs')
    echo "sweep: done in $((SECONDS - t0))s, rowids $((b0 + 1))-$b1"
    printf 'b0=%s\nb1=%s\nnat=%s\ncm=%s\n' "$b0" "$b1" "$nat" "$cm" > "$state/state"
    check "spec/run.sh exit status" "$rc" 0
}

db_checks() {
    local blk total got v snap
    blk="SELECT rowid AS rid, test_id, status FROM runs WHERE rowid > $b0 AND rowid <= $b1"
    q() { sqlite3 spec.db "WITH blk AS ($blk) $1"; }
    total=${TOTAL:-$((nat + cm))}

    check "rows in the block" "$(q 'SELECT COUNT(*) FROM blk')" "$total"
    check "distinct ids in the block" "$(q 'SELECT COUNT(DISTINCT test_id) FROM blk')" "$total"
    check "native ids" "$(q "SELECT COUNT(DISTINCT test_id) FROM blk WHERE test_id LIKE 'native:%'")" "$nat"
    check "c ids" "$(q "SELECT COUNT(DISTINCT test_id) FROM blk WHERE test_id LIKE 'c:%'")" "$cm"

    v=$(q "SELECT test_id FROM blk WHERE status = 'failing' ORDER BY test_id")
    check "failing" "$(lines "$v")" 0 "$v"
    pass "skipped (reported, not gated): $(q "SELECT COUNT(*) FROM blk WHERE status = 'skipped'")"

    v=$(grep -F '[plan=' "$state/sweep.log")
    check "files whose run disagreed with plan=" "$(lines "$v")" 0 "$v"

    # Previous row = the latest row before the block, never one inside it, so
    # this cannot compare a sweep against itself however dirty the tree is.
    v=$(q "SELECT b.test_id || '  ' || p.status || ' -> ' || b.status FROM blk b
           JOIN runs p ON p.rowid = (SELECT MAX(rowid) FROM runs
                                     WHERE test_id = b.test_id AND rowid <= $b0)
           WHERE p.status <> b.status ORDER BY b.test_id")
    check "flips (FLIPS)" "$(lines "$v")" "$flips" "$v"

    # First-ever also catches a new spec that reused a retired id: that id has
    # an old row, so the count comes up short of the prediction.
    v=$(q "SELECT DISTINCT test_id FROM blk b WHERE NOT EXISTS
           (SELECT 1 FROM runs WHERE test_id = b.test_id AND rowid <= $b0) ORDER BY 1")
    check "first-ever ids (NEW)" "$(lines "$v")" "$new" "$v"

    snap=$state/tests-before
    v=$(comm -23 <(grep -E '^(native|c):' "$snap" | sort) \
        <(q 'SELECT DISTINCT test_id FROM blk' | sort))
    check "retired ids (RETIRED)" "$(lines "$v")" "$retired" "$v"

    v=$(sqlite3 spec.db "SELECT id FROM tests t WHERE NOT EXISTS (SELECT 1 FROM runs
           WHERE test_id = t.id AND rowid > $b0 AND rowid <= $b1) ORDER BY id")
    check "registry ids the sweep did not run (ghosts)" "$(lines "$v")" 0 "$v"

    # Liveness: each id's first-ever status against its block status. Zero
    # would mean the comparison machinery is broken, not that the tree is clean.
    got=$(q "SELECT COUNT(*) FROM blk b WHERE (SELECT status FROM runs
             WHERE test_id = b.test_id ORDER BY rowid LIMIT 1) <> b.status")
    if ((got > 0)); then
        pass "liveness: $got ids differ from their first-ever status"
    else
        check "liveness: ids that differ from their first-ever status" 0 ">0"
    fi
}

echo "gate: NEW=$new FLIPS=$flips RETIRED=$retired TOTAL=${TOTAL:-plan}"
case $mode in
    static | full)
        plan_sums
        static_checks
        echo "plan sums: native $nat + c $cm = $((nat + cm))"
        if [[ $mode == full ]]; then
            if ((failed > 0)); then
                echo "gate: static checks failed; not sweeping"
            else
                sweep
                db_checks
            fi
        fi
        ;;
    recheck)
        [[ -s $state/state && -s $state/tests-before ]] ||
            { echo "gate: no previous gate sweep in $state" >&2; exit 2; }
        b0=$(sed -n 's/^b0=//p' "$state/state")
        b1=$(sed -n 's/^b1=//p' "$state/state")
        nat=$(sed -n 's/^nat=//p' "$state/state")
        cm=$(sed -n 's/^cm=//p' "$state/state")
        echo "recheck: rowids $((b0 + 1))-$b1"
        db_checks
        ;;
esac

if ((failed > 0)); then
    echo "GATE FAIL: $failed check(s)"
    exit 1
fi
echo "GATE PASS"
