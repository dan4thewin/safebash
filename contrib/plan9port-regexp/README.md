# Plan 9 regexp(2)

Source: [plan9port](https://github.com/9fans/plan9port) — Plan 9 from User Space  
Author: Rob Pike  
Files: `regcomp.c`, `regexec.c`, `regaux.c`, `regexp9.h`, `regcomp.h`

## License

Lucent Public License Version 1.02

These files are the original, unmodified sources from plan9port's
`src/libregexp/` directory. The copies in `sash-rt/csrc/` may be
modified for integration with sash-rt (e.g., header includes).

## Usage in safebash

Provides POSIX Extended Regular Expression (ERE) matching for the
`[[ string =~ pattern ]]` operator in compiled bash scripts.
Linked into `libsash_rt.a` via `sash-rt/build.rs`.

## What it does

- `regcomp9()` — compiles a regex pattern into an NFA (Reinst array)
- `regexec9()` — matches a string against a compiled pattern
- Thompson NFA guarantees linear-time matching (no ReDoS)
- Supports: `.`, `*`, `+`, `?`, `|`, `()`, `[]`, `[^]`, `^`, `$`
