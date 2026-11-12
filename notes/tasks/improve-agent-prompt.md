title: Improve autonomous agent prompt for spec-gated tasks
status: done
severity: infrastructure
gates: []
resolved: 2026-06-07
resolution: "All four deliverables shipped in run-compiler-tasks.sh. (1) `example:` field added to 16 task frontmatters. (2) Prompt template rewritten — no reading lists, agent encouraged to discover via indexnudge. (3) extract_example() function programmatically pulls code from `example: file:symbol` via sqlite3 queries to .indexnudge-rust.db. (4) sqlite3/indexnudge appears 9 times in the script with explicit \"compile after every edit, test frequently\" loop guidance. Validated on subsequent task waves."

The current `run-compiler-tasks.sh` prompt causes agents to fail because it
prescribes too much reading and too little discovery. Rewrite the prompt template:

## Problems with current prompt
1. "Read AGENTS.md" → cascades into reading status, decisions, journal
2. "Read relevant source files" → agent reads 4500 lines of codegen.rs
3. No iterative feedback — agent edits then tests once at the end
4. No codeindex.db usage — agent can't find relevant code quickly
5. Task files describe design intent, not implementation patterns

## New prompt design
- **No reading lists.** Provide search tools and pointers.
- **`.codeindex.db` as primary entry point.** Agent queries for symbols/lines.
- **One concrete example per task.** 20-40 lines showing the pattern to mimic.
- **Gates inline.** Include exact `chk` assertion lines from spec file.
- **Compile-test-fix loop.** "Compile after every edit. Test frequently."

## Task file changes
Add `example:` field to task frontmatter pointing at the code pattern to follow:
```yaml
example: sash/src/codegen.rs:sash_random  # $(signum) follows this pattern
```

## Example mapping by task type
| Task type              | Example source                              |
|------------------------|---------------------------------------------|
| New builtin $(foo)     | $(random) in codegen.rs                     |
| New validator rule     | extglob_rejected.rs (full file)             |
| New SSA instruction    | existing instruction in ssa.rs+transform.rs |
| Codegen behavior       | exit check pattern in codegen.rs            |
| Converter feature      | convert_declaration in convert.rs           |

## Deliverables
1. Add `example:` to each spec-gated task file
2. Rewrite prompt template in run-compiler-tasks.sh
3. Script extracts example code programmatically (grep/sed from example: field)
4. Re-run on 2-3 simple tasks to validate
