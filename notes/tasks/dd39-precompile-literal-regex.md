title: Precompile literal regex patterns at runtime init
status: ready
depends:
  - dd39-compiletime-regex-validation
gates: [39.9]
  - tests
  - commit

For literal regex patterns, emit one `regcomp9` call per unique
pattern at binary startup (or lazily on first use). Store the
`Reprog*` in a global. All `regexec9` / `sash_regex_replace` call
sites for that pattern reuse the cached compiled program.

### Codegen changes

- Collect all literal regex patterns during SSA walk
- Deduplicate
- Emit global slots for each `Reprog*`
- Emit init sequence: `regcomp9` each pattern, store in global
- At use sites: load from global instead of calling `regcomp9`

### Runtime changes

- `sash_regex_match` and `sash_regex_replace` need variants that
  accept a precompiled `Reprog*` instead of a pattern string
  (or add `sash_regex_match_compiled` / `sash_regex_replace_compiled`)

Ref: DD#39
