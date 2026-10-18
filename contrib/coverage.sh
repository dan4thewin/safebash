#!/bin/bash
# contrib/coverage.sh — measure spec-suite coverage of the compiler.
#
# This script exists because of three failure modes that all report a
# clean-looking result while measuring nothing:
#
#  1. `cargo llvm-cov clean --workspace` does NOT invalidate target/release.
#     A prior `cargo build --release` fingerprint is reused, the specs run
#     against an UNINSTRUMENTED binary, and the sweep drops 0 profraws. The
#     report still prints. Hence the unconditional `rm -rf target/release`.
#
#  2. `spec/run.sh` reads SASH_RELEASE_DIR, not SASH. The old mk target
#     exported `SASH=`, so it measured whatever happened to be in
#     target/release. We build into target/release and let run.sh find it.
#
#  3. `$PWD` inside a mkfile recipe is a *mk* variable, not a shell one, so
#     the old LLVM_PROFILE_FILE path expanded to a bare relative name. Doing
#     this in a shell script sidesteps the whole class.
#
# The canary below makes all three loud: one sash invocation must drop a
# profraw, or we abort before spending a full sweep on a dud.
#
# Usage: contrib/coverage.sh [--branch] [--with-unit] [--lcov FILE]
#   --branch     also measure branch coverage. Requires nightly
#                (-Zcoverage-options=branch is unstable); the shipped build
#                stays stable, nightly is used for measurement only.
#   --with-unit  include the in-crate unit tests. Off by default: the goal is
#                what the SPEC SUITE reaches, and folding unit tests in hides
#                exactly the should-never-run assertions worth finding.
#   --lcov FILE  also write per-line/-branch detail to FILE. The summary table
#                says which FILES are thin; only lcov says which LINES, and it
#                cannot be recovered after `mk cov-reset` drops the covmap.
#                Feed it to contrib/covdb.py to triage by function.
set -eu
cd "$(dirname "$0")/.."

toolchain=""
branch=""
with_unit=""
lcov=""
while [ $# -gt 0 ]; do
    case "$1" in
        --branch) toolchain="+nightly"; branch="--branch" ;;
        --with-unit) with_unit=1 ;;
        --lcov) shift; lcov="${1:-}"
                [ -n "$lcov" ] || { echo "--lcov needs a path" >&2; exit 1; } ;;
        *) echo "usage: contrib/coverage.sh [--branch] [--with-unit] [--lcov FILE]" >&2
           exit 1 ;;
    esac
    shift
done

# sash-rt, sash-convert and sash-expand are separate concerns; tests and the
# 4-line main.rs are not the subject. sash-rt has its own driver —
# contrib/rt-coverage.sh — because it needs a different binary, a different
# profile pool and an uninstrumented compiler. (This comment used to say
# sash-rt was "not measurable this way at all"; instrumentation always worked,
# only the at-exit write was missing. Journal 320.)
. contrib/cov-common.sh

eval "$(cargo $toolchain llvm-cov show-env --sh $branch 2>/dev/null)"

rm -rf target/release
cargo $toolchain build --release >/dev/null 2>&1

# The C helpers live in target/release too and were just removed with it.
cc -O2 -o target/release/sash-regex sash-rt/csrc/sash-regex.c \
    sash-rt/csrc/regcomp.c sash-rt/csrc/regexec.c sash-rt/csrc/regaux.c \
    -I sash-rt/csrc
cc -O2 -o target/release/sash-getopt sash-getopt/sash-getopt.c

rm -f target/*.profraw

# ── canary ────────────────────────────────────────────────────────────────
target/release/sash --help >/dev/null 2>&1 || true
if [ "$(ls target/*.profraw 2>/dev/null | wc -l)" -eq 0 ]; then
    echo "coverage: target/release/sash is NOT instrumented — aborting." >&2
    echo "  A report from this state measures nothing and still looks clean." >&2
    exit 1
fi

if [ -n "$with_unit" ]; then
    # This MUST be a plain `cargo test`, not `cargo llvm-cov`.
    #
    # `show-env` above exported RUSTC_WRAPPER=cargo-llvm-cov. Running the
    # `llvm-cov` *subcommand* inside that environment makes it re-enter itself
    # once per rustc invocation, and cargo-llvm-cov says so itself:
    #
    #   cargo-llvm-cov subcommands other than report and clean may not work
    #   correctly in context where environment variables are set by show-env;
    #   consider using normal cargo commands
    #
    # It does not "not work correctly" — it forks without bound. Measured here:
    # 259,142 cargo-llvm-cov processes and a load average of 13,167, which
    # starves the box until the OOM killer takes the build. That read exactly
    # like an out-of-memory failure on a busy shared host, and was misdiagnosed
    # as one twice; the tell was that the SIGKILL landed just as readily with
    # 115 GB free as with 400 MB.
    #
    # show-env has already exported LLVM_PROFILE_FILE into target/, so a normal
    # cargo command produces exactly the profraws the report step wants. There
    # is no --ignore-run-fail here deliberately: a failing unit test should fail
    # the coverage run, not be measured and discarded.
    #
    # Do NOT suppress this step's output. With it on /dev/null the failure was
    # invisible and `set -e` aborted the whole run having printed nothing at
    # all, which looks exactly like success with an empty report. A step that
    # cannot report its own failure is not a step.
    unit_log=$(mktemp)
    if ! (cd sash && cargo $toolchain test --release --lib) >"$unit_log" 2>&1; then
        echo "coverage: the unit-test pass failed --- last lines:" >&2
        tail -5 "$unit_log" >&2
        rm -f "$unit_log"
        exit 1
    fi
    rm -f "$unit_log"
fi

bash spec/run.sh >/dev/null 2>&1

n=$(ls target/*.profraw 2>/dev/null | wc -l)
echo "profraws from sweep: $n"
cargo $toolchain llvm-cov report --release $branch "${COV_REPORT_SCOPE[@]}"

if [ -n "$lcov" ]; then
    mkdir -p "$(dirname "$lcov")"
    cargo $toolchain llvm-cov report --release $branch \
        "${COV_REPORT_SCOPE[@]}" --lcov --output-path "$lcov"
    cov_check_scope "$lcov"
    echo "lcov written to $lcov"
fi

cat <<'EOF'

NB: target/release is still INSTRUMENTED (slower, and it drops a
    default_*.profraw wherever it is run, since LLVM_PROFILE_FILE is no
    longer set). `mk build` will NOT undo this: cargo-llvm-cov passes its
    flags through a RUSTC_WRAPPER, so they are invisible to cargo's
    fingerprint and the instrumented artifacts look fresh.

    Run `mk cov-reset` to get a clean binary back.
EOF
