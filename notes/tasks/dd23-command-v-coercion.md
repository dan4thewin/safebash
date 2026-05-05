title: Coerce bash command -v to [[ -x /path ]]
status: ready
severity: feature
depends:
  - dd23-compile-time-resolve
gates: [23.3, 23.4]
  - tests
  - commit

Bash pattern:
    if command -v docker >/dev/null; then docker build .; fi

Coerced sash:
    if [[ -x /usr/bin/docker ]]; then /usr/bin/docker build .; fi

The path comes from compile-time resolution. The coercion:
1. Recognizes `command -v NAME` in the lowerer
2. Resolves NAME to an absolute path (using the resolution pass)
3. Replaces with `[[ -x /resolved/path ]]`
4. `--emit-source` / `--annotate` shows the coercion

If the command can't be resolved, emit a diagnostic (the script
references a command not present on the build host / sysroot).
