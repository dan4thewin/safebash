title: DD#49 — Tautological bless rejection
status: implemented

Test expression in `bless VAR [[ expr ]]` must contain `$VAR`.
If it doesn't, rejected with SB4012. Syntactic check — catches
wrong variable and literal tests with zero false positives on real code.
