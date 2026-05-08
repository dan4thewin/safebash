title: Heredoc with command substitution
status: ready
depends: []
gates: [5.1]
example: sash/src/rules/heredoc_cmdsub.rs:walk_for_cmdsub
index_keyword: heredoc
  - tests
  - commit

Lowerer doesn't expand command substitutions in heredoc bodies
(variable expansion works). SB5002 catches the dangerous case but
the safe coerced path isn't built — extract to var, bless, reference.
