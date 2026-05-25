---
title: Builtins
modified: 2026-05-24
---

# Sash Builtins

Commands that run inline (no fork/exec). Pipe data from these is
clean (not tainted). This list may evolve.

## Output
- `echo` — write to stdout

## Control flow
- `true`, `false`, `:` — exit status primitives
- `exit` — terminate program
- `return` — return from function
- `break`, `continue` — loop control

## Input
- `read` — read line into variable(s)
- `mapfile` — read lines into array/map

## Taint
- `bless` — remove taint after validation
- `getenv` — read environment variable (result is tainted)

## Positional/args
- `argv` — read program argument (result is tainted)
- `argc` — argument count
- `shift` — shift positional parameters

## Variables
- `unset` — remove variable or array element

## Test
- `[[` — conditional test expression

## Process/signal
- `trap` — register signal handler
- `signal` — send signal to process group
- `wait` — wait for background process
- `kill` — send signal to process
- `signum` — captured signal number in trap handler

## Filesystem
- `cd` — change directory

## Introspection
- `status` — exit status of last checked command ($? sugar)
- `getpid` — current process ID
- `getppid` — parent process ID
- `getcpid` — last child process ID
- `getcwd` — current working directory
- `epochseconds` — unix timestamp

## Conversion
- `dtoi` — decimal string to integer
- `itod` — integer to decimal string

---

## NOT builtins (require exec, resolve via PATH)

printf, cat, grep, sed, awk, wc, sort, head, tail, tee, date,
hostname, find, xargs, cut, tr, basename, dirname, realpath,
mktemp, rm, cp, mv, mkdir, chmod, chown, ln, test, [, etc.

## Reserved words (parsed as syntax)

if, then, else, elif, fi, while, until, do, done, for, in,
case, esac, fn, function, local, int, const, array, map,
bless, signal, end, return, break, continue
