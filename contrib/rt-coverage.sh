#!/bin/bash
# contrib/rt-coverage.sh — measure spec-suite coverage of the RUNTIME.
#
# contrib/coverage.sh measures the compiler and explicitly ignores sash-rt,
# because for a long time sash-rt was not measurable at all: it is no_std
# with no libc, so compiler-rt's atexit writer — which wants `FILE*`,
# `getenv` and `snprintf` — could never be linked, and every instrumented
# binary exited with live counters and wrote nothing.
#
# sash-rt/src/coverage.rs supplies the missing write over rustix. This script
# supplies the rest of the loop. See that module for why only seven objects
# out of libprofiler_builtins are needed.
#
# Three things here are load-bearing and each looks fine when wrong:
#
#  1. The profiler objects come from the *rustc sysroot*, not from clang.
#     Both work, but the .profraw header version is written by
#     InstrProfilingVersionVar.o while the counter layout is decided by the
#     rustc that instrumented the code. Taking both from one toolchain means
#     they cannot disagree; mixing brew clang with rustup rustc produces a
#     file `llvm-profdata` rejects, or worse, misreads.
#
#  2. The report runs against a --whole-archive reference binary, not
#     against a spec binary. `sash` links with `--gc-sections`, so each
#     compiled program keeps only the runtime objects it actually calls and
#     its covmap describes a *subset* of sash-rt. Reporting against one of
#     those silently scopes the result to whatever that one program touched.
#
#  3. The compiler is built UNinstrumented. sash runs thousands of times
#     during a sweep; if it were instrumented too, its profraws would land in
#     the same pool and be merged into the runtime's numbers.
#
# The canary below makes the whole chain loud: one compiled program must drop
# a profraw before we spend a full sweep.
#
# Usage: contrib/rt-coverage.sh [--branch] [--lcov FILE] [--keep]
#   --branch    also measure branch coverage. Requires nightly
#               (-Zcoverage-options=branch is unstable).
#   --lcov FILE write per-line/-branch detail for contrib/covdb.py.
#   --keep      keep the raw .profraw files (default: delete after merge;
#               a sweep drops several GB of them).
set -eu
cd "$(dirname "$0")/.."
root=$(pwd)

toolchain=""
branch=""
lcov=""
keep=""
while [ $# -gt 0 ]; do
    case "$1" in
        --branch) toolchain="+nightly"; branch="--branch" ;;
        --lcov) shift; lcov="${1:-}"
                [ -n "$lcov" ] || { echo "--lcov needs a path" >&2; exit 1; } ;;
        --keep) keep=1 ;;
        *) echo "usage: contrib/rt-coverage.sh [--branch] [--lcov FILE] [--keep]" >&2
           exit 1 ;;
    esac
    shift
done

profdir="$root/target/rt-coverage"
rm -rf "$profdir"
mkdir -p "$profdir"

sysroot=$(rustc $toolchain --print sysroot)
host=$(rustc $toolchain -vV | sed -n 's/^host: //p')
rlib=$(echo "$sysroot/lib/rustlib/$host/lib/libprofiler_builtins-"*.rlib)
if [ ! -f "$rlib" ]; then
    echo "rt-coverage: no libprofiler_builtins in $sysroot" >&2
    echo "  install it with: rustup component add llvm-tools-preview" >&2
    exit 1
fi

profdata=$(find "$sysroot" -name llvm-profdata -type f | head -1)
llvmcov=$(find "$sysroot" -name llvm-cov -type f | head -1)
if [ -z "$profdata" ] || [ -z "$llvmcov" ]; then
    echo "rt-coverage: llvm-profdata/llvm-cov not in $sysroot" >&2
    echo "  install them with: rustup component add llvm-tools-preview" >&2
    exit 1
fi

# ── 1. instrument the runtime ─────────────────────────────────────────────
rtflags="-Cinstrument-coverage"
[ -n "$branch" ] && rtflags="$rtflags -Zcoverage-options=branch"
(cd sash-rt && cargo clean -p sash-rt >/dev/null 2>&1
 RUSTFLAGS="$rtflags" cargo $toolchain build --release --features coverage) \
    >/dev/null 2>&1
archive="$root/sash-rt/target/release/libsash_rt.a"
[ -f "$archive" ] || { echo "rt-coverage: sash-rt build failed" >&2; exit 1; }

# ── 2. fold in compiler-rt's buffer writer ────────────────────────────────
# Only these seven. The full archive drags in the FILE*/getenv/printf layer
# that no_std cannot satisfy; ld -r over this set leaves just memcpy, memset,
# memcmp (all defined in sash-rt/src/lib.rs) and the linker's __ehdr_start.
objs="InstrProfilingBuffer InstrProfilingWriter InstrProfilingPlatformLinux
      InstrProfilingInternal InstrProfilingVersionVar InstrProfilingNameVar
      InstrProfiling"
objdir="$profdir/rtobj"
mkdir -p "$objdir"
(cd "$objdir"
 # Members carry a per-build hash prefix, so ask the archive for the names.
 for o in $objs; do
     m=$(ar t "$rlib" | grep -E "(^|-)$o\.o$" | head -1)
     [ -n "$m" ] || { echo "rt-coverage: $o.o not in $rlib" >&2; exit 1; }
     ar x "$rlib" "$m"
 done)
ar r "$archive" "$objdir"/*.o 2>/dev/null

# ── 3. reference binary carrying the COMPLETE covmap ──────────────────────
cat > "$profdir/refmain.c" <<'EOF'
/* Only here to satisfy _start's call to main; see note 2 in the header. */
long main(void) { return 0; }
EOF
gcc -c -O0 -o "$profdir/refmain.o" "$profdir/refmain.c"
# --unresolved-symbols=ignore-all: --whole-archive also drags in
# compiler_builtins members that a real link drops, and those reference
# rust_eh_personality. This binary is never executed — llvm-cov reads its
# __llvm_covmap and __llvm_prf_names sections and nothing else — so an
# unresolved symbol costs nothing, while dropping the member would cost the
# covmap records we are here for.
gcc -static -nostdlib -nostartfiles -o "$profdir/rt-covmap" \
    "$profdir/refmain.o" \
    -Wl,--whole-archive "$archive" -Wl,--no-whole-archive \
    -Wl,--unresolved-symbols=ignore-all 2>/dev/null
[ -x "$profdir/rt-covmap" ] || { echo "rt-coverage: reference link failed" >&2; exit 1; }

# ── 4. uninstrumented compiler + C helpers ────────────────────────────────
(cd sash && cargo build --release) >/dev/null 2>&1
cc -O2 -o target/release/sash-regex sash-rt/csrc/sash-regex.c \
    sash-rt/csrc/regcomp.c sash-rt/csrc/regexec.c sash-rt/csrc/regaux.c \
    -I sash-rt/csrc
cc -O2 -o target/release/sash-getopt sash-getopt/sash-getopt.c
if nm -C target/release/sash 2>/dev/null | grep -q llvm_profile; then
    echo "rt-coverage: target/release/sash is instrumented — run mk cov-reset first." >&2
    echo "  Its profraws would be merged into the runtime's numbers." >&2
    exit 1
fi

# ── 5. canary ─────────────────────────────────────────────────────────────
# SASH_RT_PROFILE_DIR, not LLVM_PROFILE_FILE: cargo-llvm-cov sets the latter
# for the compiler, and a compiled program that honoured it would overwrite
# the compiler's own profraw on every run.
export SASH_RT_PROFILE_DIR="$profdir"
"$root/target/release/sash" -c 'echo canary' -o "$profdir/canary" >/dev/null 2>&1
"$profdir/canary" >/dev/null 2>&1 || true
if [ "$(ls "$profdir"/*.profraw 2>/dev/null | wc -l)" -eq 0 ]; then
    echo "rt-coverage: a compiled program dropped no profraw — aborting." >&2
    echo "  Nothing downstream can tell this from a runtime nobody calls." >&2
    exit 1
fi
rm -f "$profdir"/*.profraw

# ── 6. sweep ──────────────────────────────────────────────────────────────
bash spec/run.sh >/dev/null 2>&1 || true

n=$(ls "$profdir"/*.profraw 2>/dev/null | wc -l)
echo "profraws from sweep: $n"
[ "$n" -gt 0 ] || { echo "rt-coverage: sweep dropped none — aborting." >&2; exit 1; }

# ── 7. report ─────────────────────────────────────────────────────────────
ls "$profdir"/*.profraw > "$profdir/inputs.txt"
"$profdata" merge -sparse -f "$profdir/inputs.txt" -o "$profdir/rt.profdata"
[ -n "$keep" ] || rm -f "$profdir"/*.profraw

# coverage.rs is the measuring apparatus: it exists only under the feature
# this script turns on, so no shipped binary contains it and counting it
# would be the measurement measuring itself.
IGNORE='(/\.cargo/|/rustc/|library/core|refmain|sash-rt/src/coverage\.rs)'
"$llvmcov" report "$profdir/rt-covmap" \
    -instr-profile="$profdir/rt.profdata" \
    -ignore-filename-regex="$IGNORE" ${branch:+--show-branch-summary}

if [ -n "$lcov" ]; then
    mkdir -p "$(dirname "$lcov")"
    "$llvmcov" export "$profdir/rt-covmap" \
        -instr-profile="$profdir/rt.profdata" \
        -ignore-filename-regex="$IGNORE" -format=lcov > "$lcov"
    echo "lcov written to $lcov"
fi

cat <<'EOF'

NB: sash-rt/target/release/libsash_rt.a is still INSTRUMENTED, and every
    program sash compiles will link it — slower, larger, and dropping a
    profraw whenever SASH_RT_PROFILE_DIR is set.

    Run `mk rt-cov-reset` to get a clean runtime back.
EOF
