title: Review taint clean list and magic variable policy
status: done
depends: []
severity: design
gates: [24.1, 24.2, 24.3, 24.4, 24.5, 24.6, 24.7, 24.8, 24.14, 24.15, 42.2]

## Completed 2026-05-11

The `is_special_var` "clean list" in taint.rs was dead code — never
called. Removed entirely. Taint is definition-based, not name-based.

Implemented:
- SB5017: rejects BASH_*, RANDOM, SECONDS, PIPESTATUS, OLDPWD, SHLVL,
  _, EPOCHREALTIME, $0, HOSTNAME, UID, EUID, GROUPS, HOSTTYPE, OSTYPE,
  MACHTYPE
- SB5018: rejects bare $HOME, $PATH, etc. — directs to $(getenv NAME)
- $LINENO: compile-time rewrite to literal line number
- $(getppid), $(getcwd): new keywords with runtime + codegen
- sash_random: removed from runtime, codegen, transform
- Converter: $HOME → $(getenv HOME) + auto-bless with known patterns

Decisions made by DG:
- $0 rejected by compiler, converter rewrites to $(argv 0)
- IFS just another env var via getenv, not special
- Function positionals ($1..$9, $@) clean within functions
- Drop: SECONDS, PIPESTATUS, OLDPWD, SHLVL, _, RANDOM
- Keywords: getpid, getppid, getcpid, getcwd, epochseconds,
  signum, argc, argv, getenv, status
