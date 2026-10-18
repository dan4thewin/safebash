#!/bin/bash
# Shared compiler-coverage scope. codegen/tests.rs is production, not a unit file.
# cargo-llvm-cov's default filter also excludes tests.rs, so replace it.
COV_REPORT_SCOPE=(
    --no-default-ignore-filename-regex
    --ignore-filename-regex
    '(sash-rt|sash-convert|sash-expand|/tests/|/parse/tests\.rs$|main\.rs)'
)

cov_check_scope() {
    if ! grep -q '^SF:.*sash/src/codegen/tests\.rs$' "$1"; then
        echo "coverage: missing production module sash/src/codegen/tests.rs in $1" >&2
        return 1
    fi
}
