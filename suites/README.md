# External Test Suites

Three external bash test suites, gitignored (not committed).
Populate locally:

```bash
# Oils spec tests (~2MB, 222 files, 3964 cases)
cd /tmp && git clone --depth 1 https://github.com/oilshell/oil.git
cp -r /tmp/oil/spec suites/oils/

# Bash test suite (~3MB, 80 .tests files + 391 .sub + 84 .right)
cd /tmp && wget -q https://ftp.gnu.org/gnu/bash/bash-5.2.37.tar.gz && tar xzf bash-5.2.37.tar.gz
cp -r /tmp/bash-5.2.37/tests suites/bash/

# ShellCheck source (~856KB, rules embedded in Haskell)
cd /tmp && git clone --depth 1 https://github.com/koalaman/shellcheck.git
cp -r /tmp/shellcheck/src suites/shellcheck/
```

## Suite comparison

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
