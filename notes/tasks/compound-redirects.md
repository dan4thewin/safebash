title: Compound command redirects
status: done
severity: feature
gates: [14.4, 14.5, 14.6]

Redirects on compound commands: for...done > file, while...done | cmd,
if...fi > file. Codegen has partial support but edge cases remain.

Historical completed record. DG's command-group retirement (DD70,
journal 596) now requires source groups with redirects to reject;
`retire-command-groups` owns that pending change. It does not remove
redirect support needed by still-legal named functions or other bodies.
