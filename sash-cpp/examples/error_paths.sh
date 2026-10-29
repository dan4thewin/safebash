#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

basic_only=0
list_only=0
without_injection=0
selected=
build=build
active=
ran=0

die() {
    printf 'error showcase: %s\n' "$*" >&2
    if [[ -n $active ]]; then
        printf 'evidence: %s.*\n' "$active" >&2
        for file in "$active.out" "$active.err"; do
            [[ ! -f $file ]] || cat "$file" >&2
        done
    fi
    exit 1
}

while (( $# )); do
    case $1 in
        --basic) basic_only=1 ;;
        --list) list_only=1 ;;
        --without-injection) without_injection=1 ;;
        --case)
            (( $# >= 2 )) || die '--case requires a name'
            selected=$2
            shift
            ;;
        --*) die "unknown option: $1" ;;
        *) build=$1 ;;
    esac
    shift
done

program=$build/error_paths
helper=$build/command-helper
supervisor=$build/error-supervisor
directory=$build/error-demo
trace_set=write,read,close,fcntl,dup2,lseek,pipe2,memfd_create,mmap,munmap,clone,wait4,execve
if (( ! list_only )); then
    [[ -x $program && -x $helper ]] || die 'build the examples and command helper first'
    [[ ! -e $helper.missing ]] || die "unexpected missing-command fixture: $helper.missing"
    mkdir -p "$directory"
    if (( ! basic_only )); then
        command -v strace >/dev/null || die 'the full showcase requires strace; --basic does not'
        [[ -x $supervisor ]] || die 'build the error supervisor with mk -p 1 error-demo'
    fi
fi

wanted() {
    [[ -z $selected || $selected == "$1" ]]
}

invoke() {
    local prefix=$1 scenario=$2 handling=$3 context=$4 traced=$5
    shift 5
    local command=("$program" "$scenario" "$handling" "$helper")
    if [[ $context == no-child-status ]]; then
        command=("$helper" ignore-sigchld "${command[@]}")
    fi
    if (( traced )); then
        : >"$prefix.trace"
        command=(strace -qq -s 4096 -e "trace=$trace_set" -o "$prefix.trace" "$@" "${command[@]}")
        command=("$supervisor" "${command[@]}")
    fi
    if (
        case $context in
            stdin-closed) exec 0<&- ;;
            stdout-closed) exec 1>&- ;;
            stdin-stdout-closed) exec 0<&- 1>&- ;;
        esac
        export SASH_CPP_ERROR_TRACE=1
        exec timeout 10 "${command[@]}"
    ) >"$prefix.out" 2>"$prefix.err.raw"; then
        observed=0
    else
        observed=$?
    fi
    sed '/^__SASH_CPP_DEMO_BEGIN__$/d' "$prefix.err.raw" >"$prefix.err"
}

show() {
    local name=$1 channel line
    printf '\n== %s ==\n' "$name"
    for channel in stdout stderr; do
        while IFS= read -r line || [[ -n $line ]]; do
            printf '  %s: %s\n' "$channel" "$line"
        done <"$active.${channel#std}"
    done
    printf '  exit: %s\n' "$observed"
    ran=$((ran + 1))
}

basic() {
    local name=$1 scenario=$2 handling=$3 status=$4 pattern=$5 context=${6:-normal}
    wanted "$name" || return 0
    if (( list_only )); then
        printf '%s\n' "$name"
        ran=$((ran + 1))
        return
    fi
    active=$directory/$name
    invoke "$active" "$scenario" "$handling" "$context" 0
    [[ $observed == "$status" ]] || die "$name exited $observed, expected $status"
    if (( status )); then
        [[ ! -s $active.out ]] || die "$name leaked output before aborting"
        grep -qE -- "$pattern" "$active.err" || die "$name did not report the expected failure"
        ! grep -q '^continued$' "$active.err" || die "$name continued after an uncaught error"
    else
        local report=$active.out other=$active.err
        if [[ $context == stdout-closed ]]; then
            report=$active.err
            other=$active.out
        fi
        [[ ! -s $other ]] || die "$name emitted an unexpected diagnostic or leaked capture"
        local expected='^'
        if [[ -n $pattern ]]; then
            expected+=$pattern$'\n'
        fi
        if [[ $scenario == capture-command* ]]; then
            expected+=$'target: unchanged\n'
        fi
        expected+='continued$'
        local output
        output=$(<"$report")
        [[ $output =~ $expected ]] || die "$name did not preserve the expected continuation/state"
    fi
    show "$name"
}

ordinal() {
    awk -v call="$2(" -v pattern="$3" -v occurrence="$4" '
        index($0, call) == 1 { count++ }
        index($0, "write(2, \"__SASH_CPP_DEMO_BEGIN__") == 1 { begun=1; next }
        begun && index($0, call) == 1 && $0 ~ pattern {
            if (++seen == occurrence) { print count; found=1; exit }
        }
        END { if (!found) exit 1 }
    ' "$1"
}

fault() {
    local name=$1 scenario=$2 syscall=$3 selector=$4 occurrence=$5 effect=$6 diagnostic=$7
    local context=${8:-normal}
    (( ! basic_only )) || return 0
    wanted "$name" || return 0
    if (( list_only )); then
        printf '%s\n' "$name"
        ran=$((ran + 1))
        return
    fi
    active=$directory/$name
    local baseline=$active.baseline baseline_status=0 number
    [[ $scenario != capture-command* && $scenario != exit7 ]] || baseline_status=7
    invoke "$baseline" "$scenario" checked "$context" 1
    [[ $observed == "$baseline_status" ]] || die "$name's uninjected baseline failed"
    if ! number=$(ordinal "$baseline.trace" "$syscall" "$selector" "$occurrence"); then
        die "$name's intended syscall was not reached after the trace marker"
    fi
    local options=()
    if (( ! without_injection )); then
        options=(-e "inject=$syscall:$effect:when=$number")
        if [[ $name == cleanup-wait ]]; then
            local wait_number
            wait_number=$(ordinal "$baseline.trace" wait4 '^wait4[(]' 1) ||
                die 'cleanup-wait has no baseline wait'
            options+=(-e "inject=wait4:error=EIO:when=$wait_number")
        fi
    fi
    invoke "$active" "$scenario" checked "$context" 1 "${options[@]}"
    grep -E "^$syscall[(].*[(]INJECTED[)]" "$active.trace" >"$active.injected" ||
        die "$name did not inject its intended syscall"
    grep -qE -- "$selector" "$active.injected" ||
        die "$name injected a different syscall site than its baseline selector"
    if [[ $effect == error=EINTR ]]; then
        [[ $observed == "$baseline_status" ]] || die "$name failed to recover from interruption"
        cmp -s "$baseline.out" "$active.out" && cmp -s "$baseline.err" "$active.err" ||
            die "$name changed output while recovering from interruption"
    else
        [[ $observed == 2 ]] || die "$name exited $observed instead of 2"
        [[ ! -s $active.out ]] || die "$name leaked captured output after failure"
        if [[ $diagnostic == none ]]; then
            [[ ! -s $active.err ]] || die "$name unexpectedly managed to print its diagnostic"
        else
            grep -qE -- "$diagnostic" "$active.err" || die "$name reported a different fault site"
        fi
        if [[ $name == cleanup-wait ]]; then
            grep -E '^wait4[(].*EIO.*INJECTED' "$active.trace" >>"$active.injected" ||
                die 'cleanup-wait did not fail the destructor wait'
        fi
    fi
    show "$name"
    sed 's/^/  injected: /' "$active.injected"
}

child_fault() {
    local name=$1 scenario=$2 syscall=$3 diagnostic=$4
    (( ! basic_only )) || return 0
    wanted "$name" || return 0
    if (( list_only )); then
        printf '%s\n' "$name"
        ran=$((ran + 1))
        return
    fi
    active=$directory/$name
    local options=(-f)
    if (( ! without_injection )); then
        # Counters are per tracee: the parent loader's first close is also affected.
        options+=(-e "inject=$syscall:error=EIO:when=1")
    fi
    invoke "$active" "$scenario" checked normal 1 "${options[@]}"
    [[ $observed == 2 ]] || die "$name exited $observed instead of 2"
    grep -qE -- "$diagnostic" "$active.err" ||
        die "$name did not report its pre-exec failure"
    awk -v call="$syscall" '
        /clone[(]|clone resumed/ { if ($NF ~ /^[0-9]+$/) child=$NF }
        index($0, call "(") || index($0, call " resumed") {
            if (/INJECTED/) { failed[$1]=1; print }
        }
        /execve[(]/ { executed[$1]=1 }
        END { exit !(child in failed) || child in executed }
    ' "$active.trace" >"$active.injected" ||
        die "$name did not stop the actual fork child before exec"
    show "$name"
    sed 's/^/  injected: /' "$active.injected"
}

basic success process checked 0 ''
basic stream-success stream checked 0 ''
basic regex-no-match regex-no-match checked 0 'no match is not an error'
basic capture-success capture checked 0 'target: captured'
basic capture-closed-success capture-closed checked 0 'target: captured' stdout-closed
for handling in caught discarded checked; do
    for specification in \
        'exit7|exit7|7|caught exit: 7' \
        'exit127|exit127|127|caught exit: 127' \
        'signal|signal|143|caught signal: 15' \
        'missing|missing|2|caught setup 2: .*' \
        'permission|permission|2|caught setup 13: .*' \
        'format|format|2|caught setup 8: .*' \
        'nul-path|nul-path|2|caught setup 22: .*' \
        'nul-argument|nul-argument|2|caught setup 22: .*' \
        'bad-write|bad-write|2|caught command 2: write: .*' \
        'regex|regex|2|caught command 2: regex: .*' \
        'regex-raw|regex-raw|2|caught regex: .*' \
        'length|length|2|caught length: .*' \
        'bad-alloc-injected|bad-alloc|2|caught allocation: .*' \
        'capture-command|capture-command|7|caught command 7: .*'; do
        IFS='|' read -r label operation checked_status caught <<<"$specification"
        status=0
        pattern=
        if [[ $handling == checked ]]; then
            status=$checked_status
            case $operation in
                regex) pattern='^error_paths: regex: ' ;;
                bad-alloc) pattern='^error_paths: out of memory$' ;;
                length) pattern='^error_paths: value exceeds string capacity$' ;;
                *) pattern='^error_paths: ' ;;
            esac
        elif [[ $handling == caught ]]; then
            pattern=$caught
        fi
        basic "$label-$handling" "$operation" "$handling" "$status" "$pattern"
    done
    status=0
    pattern=
    if [[ $handling == checked ]]; then
        status=2
        pattern='^error_paths: echo: '
    elif [[ $handling == caught ]]; then
        pattern='caught command 2: echo: .*'
    fi
    basic "echo-closed-$handling" echo-closed "$handling" "$status" "$pattern" stdout-closed
    pattern=
    if [[ $handling == checked ]]; then
        pattern='^error_paths: wait for child: '
    elif [[ $handling == caught ]]; then
        pattern='caught setup 10: wait for child: .*'
    fi
    basic "wait-no-status-$handling" process "$handling" "$status" "$pattern" no-child-status
done

fault write-error write write '^write[(]1, "written' 1 error=EIO '^error_paths: write: '
fault write-zero write write '^write[(]1, "written' 1 retval=0 '^error_paths: write: '
fault stdout-inspect capture fcntl '^fcntl[(]1, F_GETFD' 1 error=EIO '^error_paths: inspect stdout: '
fault memfd capture memfd_create '^memfd_create[(]' 1 error=EMFILE '^error_paths: memfd_create: '
fault capture-backup capture fcntl '^fcntl[(]1, F_DUPFD_CLOEXEC' 1 error=EMFILE '^error_paths: save descriptor: '
fault capture-install capture dup2 '^dup2[(]' 1 error=EIO '^error_paths: capture stdout: '
fault capture-restore capture dup2 '^dup2[(]' 2 error=EIO '^error_paths: restore stdout: '
fault capture-restore-flags capture fcntl '^fcntl[(]1, F_SETFD' 1 error=EIO '^error_paths: restore stdout: '
fault capture-close-saved capture close '^close[(]' 1 error=EIO '^error_paths: close descriptor: '
fault capture-rewind capture lseek '^lseek[(]' 1 error=EIO '^error_paths: rewind capture: '
fault capture-read capture read '^read[(]' 1 error=EIO '^error_paths: read capture: '
fault capture-close-store capture close '^close[(]' 2 error=EIO '^error_paths: close descriptor: '
fault capture-closed-backup capture-closed fcntl '^fcntl[(]1, F_DUPFD_CLOEXEC' 1 error=EMFILE '^error_paths: save descriptor: ' stdout-closed
fault capture-closed-restore capture-closed close '^close[(]1[)]' 2 error=EIO '^error_paths: restore stdout: ' stdout-closed
fault capture-closed-close-private capture-closed close '^close[(]1[)]' 1 error=EIO '^error_paths: close descriptor: ' stdout-closed
fault process-pipe process pipe2 '^pipe2[(]' 1 error=EMFILE '^error_paths: create exec error pipe: '
fault process-pipe-relocate process fcntl '^fcntl[(]0, F_DUPFD_CLOEXEC' 1 error=EMFILE '^error_paths: save descriptor: ' stdin-closed
fault process-pipe-close-private process close '^close[(]0[)]' 1 error=EIO '^error_paths: close descriptor: ' stdin-closed
fault process-pipe-relocate-writer process-closed fcntl '^fcntl[(]1, F_DUPFD_CLOEXEC' 1 error=EMFILE '^error_paths: save descriptor: ' stdin-stdout-closed
fault process-pipe-close-private-writer process-closed close '^close[(]1[)]' 1 error=EIO '^error_paths: close descriptor: ' stdin-stdout-closed
fault process-status-storage process mmap '^mmap[(].*MAP_SHARED[|]MAP_ANONYMOUS' 1 error=ENOMEM '^error_paths: allocate child setup status: '
fault fork process clone '^clone[(]' 1 error=EAGAIN '^error_paths: fork '
fault process-close-writer process close '^close[(]' 1 error=EIO '^error_paths: close descriptor: '
fault process-completion-read process read '^read[(]' 1 error=EIO '^error_paths: read exec completion pipe: '
fault process-close-reader process close '^close[(]' 2 error=EIO '^error_paths: close descriptor: '
fault wait process wait4 '^wait4[(]' 1 error=EIO '^error_paths: wait for child: '
fault completion-protocol-synthetic process read '^read[(]' 1 retval=1 '^error_paths: unexpected exec completion data: '
child_fault child-reader-close process close '^error_paths: close child exec error reader: '
child_fault child-stdout-install stream dup2 '^error_paths: connect child stdout: '
fault stream-output-pipe stream pipe2 '^pipe2[(]' 1 error=EMFILE '^error_paths: create command output pipe: '
fault stream-completion-pipe stream pipe2 '^pipe2[(]' 2 error=EMFILE '^error_paths: create exec error pipe: '
fault stream-close-writer stream close '^close[(]' 1 error=EIO '^error_paths: close descriptor: '
fault stream-completion-close-writer stream close '^close[(]' 2 error=EIO '^error_paths: close descriptor: '
fault stream-read stream read '^read[(].*, 8192[)]' 1 error=EIO '^error_paths: read command output: '
fault stream-close-reader stream close '^close[(]' 4 error=EIO '^error_paths: close descriptor: '
fault cleanup-restore capture-command dup2 '^dup2[(]' 2 error=EIO '^sash-cpp: cleanup failed: restore stdout '
fault cleanup-restore-flags capture-command fcntl '^fcntl[(]1, F_SETFD' 1 error=EIO '^sash-cpp: cleanup failed: restore stdout '
fault cleanup-restore-closed capture-command-closed close '^close[(]1[)]' 2 error=EIO '^sash-cpp: cleanup failed: restore stdout ' stdout-closed
fault cleanup-close capture-command close '^close[(]' 3 error=EIO '^sash-cpp: cleanup failed: close descriptor '
fault cleanup-wait process read '^read[(]' 1 error=EIO '^sash-cpp: cleanup failed: wait for child '
fault cleanup-status-storage process munmap '^munmap[(]' 1 error=EIO '^sash-cpp: cleanup failed: unmap child setup status '
fault diagnostic-output exit7 write '^write[(]2, "error_paths"' 1 error=EIO none
fault diagnostic-separator exit7 write '^write[(]2,' 2 error=EIO '^error_paths$'
fault diagnostic-message exit7 write '^write[(]2,' 3 error=EIO '^error_paths: $'
fault diagnostic-newline exit7 write '^write[(]2,' 4 error=EIO '^error_paths: .*command failed [(]exit 7[)]$'

fault retry-write write write '^write[(]1, "written' 1 error=EINTR ''
fault retry-stdout-inspect capture fcntl '^fcntl[(]1, F_GETFD' 1 error=EINTR ''
fault retry-capture-backup capture fcntl '^fcntl[(]1, F_DUPFD_CLOEXEC' 1 error=EINTR ''
fault retry-capture-install capture dup2 '^dup2[(]' 1 error=EINTR ''
fault retry-capture-restore capture dup2 '^dup2[(]' 2 error=EINTR ''
fault retry-capture-flags capture fcntl '^fcntl[(]1, F_SETFD' 1 error=EINTR ''
fault retry-capture-rewind capture lseek '^lseek[(]' 1 error=EINTR ''
fault retry-capture-read capture read '^read[(]' 1 error=EINTR ''
fault retry-pipe process pipe2 '^pipe2[(]' 1 error=EINTR ''
fault retry-pipe-relocate process fcntl '^fcntl[(]0, F_DUPFD_CLOEXEC' 1 error=EINTR '' stdin-closed
fault retry-completion-read process read '^read[(]' 1 error=EINTR ''
fault retry-wait process wait4 '^wait4[(]' 1 error=EINTR ''
fault retry-diagnostic exit7 write '^write[(]2, "error_paths"' 1 error=EINTR ''
fault retry-stream-read stream read '^read[(].*, 8192[)]' 1 error=EINTR ''

(( ran )) || die "no case selected: $selected"
if (( ! list_only )); then
    printf '\nDemonstrated %d case' "$ran"
    (( ran == 1 )) || printf 's'
    (( ! basic_only )) || printf ' (basic subset; syscall faults not requested)'
    printf '.\n'
fi
