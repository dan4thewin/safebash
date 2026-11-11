# External Test Suites

The downloaded corpora are upstream material, gitignored and not committed.
The `archive/legacy-suites` tag preserves this retrieval guide and the
historical runners for possible reuse in converter tests, not the corpora.
The runners contain obsolete compiler paths and are not current test gates.

Run these commands from the safebash repository root. They preserve the
upstream `spec/`, `tests/` and `src/` directory layout expected below:

```bash
fetch_dir=$(mktemp -d)
mkdir -p suites/oils suites/bash suites/shellcheck

# Oils spec tests (~2MB, 222 files, 3964 cases)
git clone --depth 1 https://github.com/oilshell/oil.git "$fetch_dir/oil" &&
    cp -R "$fetch_dir/oil/spec" suites/oils/

# Bash test suite (~3MB, 80 .tests files + 391 .sub + 84 .right)
curl --fail --location https://ftp.gnu.org/gnu/bash/bash-5.2.37.tar.gz \
    --output "$fetch_dir/bash-5.2.37.tar.gz" &&
    tar xzf "$fetch_dir/bash-5.2.37.tar.gz" -C "$fetch_dir" &&
    cp -R "$fetch_dir/bash-5.2.37/tests" suites/bash/

# ShellCheck source (~856KB, rules embedded in Haskell)
git clone --depth 1 https://github.com/koalaman/shellcheck.git "$fetch_dir/shellcheck" &&
    cp -R "$fetch_dir/shellcheck/src" suites/shellcheck/

printf 'Temporary upstream sources: %s\n' "$fetch_dir"
```

The copies land in `suites/oils/spec/`, `suites/bash/tests/` and
`suites/shellcheck/src/`. Remove the printed temporary directory when it is
no longer needed. Oils and ShellCheck follow their current default branches;
these commands do not reproduce the exact historical downloaded copies.

`brush_runner.py` instead reads a separate
[Brush checkout](https://github.com/reubeno/brush), historically at
`~/src/brush/brush-shell/tests/cases/`. Its inputs were never stored here.

## Historical suite comparison

| Suite | Cases | Format | Use |
|-------|-------|--------|-----|
| Oils | 3,964 | `#### Name` + `## stdout:` blocks | Primary — self-contained, machine-parseable |
| Bash | ~6,500 | `.tests` scripts + `.right` golden output | Gold standard, but needs `source` support |
| ShellCheck | 2,208 | Haskell `verify`/`verifyNot` inline | Validator/coercer rule checklist |

## Relevant Oils files for safebash

Arithmetic: arith.test.sh, arith-context.test.sh, for-expr.test.sh
Variables: assign.test.sh, assign-extended.test.sh, var-op-test.test.sh, var-op-strip.test.sh
Arrays: array-literal.test.sh, array-assign.test.sh, array-basic.test.sh, array-assoc.test.sh
Control: loop.test.sh, case_.test.sh, if_.test.sh
Conditionals: dbracket.test.sh
Functions: func-parsing.test.sh, sh-func.test.sh
I/O: redirect.test.sh, command-sub.test.sh, pipeline.test.sh
Builtins: builtin-printf.test.sh, builtin-echo.test.sh
