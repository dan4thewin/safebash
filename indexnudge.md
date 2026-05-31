<!-- BEGIN indexnudge AGENTS.md block -->
## Code index database

This project has an **indexnudge** SQLite database at `.indexnudge-rust.db`
in the repo root containing every line of source, every symbol definition,
every call site, every import, and full-text search over the code.

**Use `sqlite3` against the index for ALL code navigation.** Do NOT use
`grep`, `sed`, `cat`, `head`, `tail`, `find`, `view`, or `glob` to read
source files — the database is faster, gives structural context, and
keeps line numbers stable for editing.

Find the database and discover the schema on your first read:

```bash
DB=.indexnudge-rust.db
sqlite3 "$DB" ".schema"
```

### Core query patterns


```bash
# Project overview
sqlite3 "$DB" "SELECT file, lines, language FROM files ORDER BY file;"

# Symbols in a file (functions, methods, types)
sqlite3 "$DB" "SELECT name, kind, start_line, end_line, signature
               FROM symbols WHERE file='src/foo.rs' ORDER BY start_line;"

# Find a symbol by name across the whole codebase
sqlite3 "$DB" "SELECT name, kind, file, start_line, end_line
               FROM symbols WHERE name LIKE '%Parser%';"

# Read specific lines (use start_line/end_line from symbols query)
sqlite3 "$DB" "SELECT line_num, content FROM lines
               WHERE file='src/foo.rs' AND line_num BETWEEN 120 AND 180;"

# Full-text search (FTS5 — supports phrase, prefix, AND/OR/NOT)
sqlite3 "$DB" "SELECT file, line_num, content FROM lines_fts
               WHERE content MATCH '\"render_template\"';"

# Call graph: who calls X? what does Y call?
sqlite3 "$DB" "SELECT caller_file, caller_symbol, line_num
               FROM calls WHERE callee='render_template';"
sqlite3 "$DB" "SELECT DISTINCT callee FROM calls
               WHERE caller_symbol='handle_request' ORDER BY callee;"

# Imports — what does this file pull in?
sqlite3 "$DB" "SELECT module, line_num FROM imports WHERE file='src/foo.rs';"
```

### Schema reference

| Table         | Columns of interest                                                |
|---------------|--------------------------------------------------------------------|
| `lines`       | `file, line_num, content, symbol`                                  |
| `lines_fts`   | FTS5 over `(file, line_num, content, symbol)`                      |
| `symbols`     | `name, qualified_name, kind, file, start_line, end_line, parent, signature` |
| `calls`       | `caller_file, caller_symbol, callee, kind, line_num`               |
| `defs`        | `symbol, file, line_num, var, kind`  *(variable/binding definitions)* |
| `imports`     | `file, module, line_num`                                           |
| `files`       | `file, lines, language, indexed_mtime`                             |
| `directories` | `path, file_count, total_lines`                                    |


### Freshness

If the `sqlite3` shim is on `PATH`, the database is re-freshened
automatically before each query (only changed files are re-parsed).
You don't need to re-run the indexer after editing source files.

### Editing

When you need to mutate a file, prefer tools that take **line-number
addresses** (e.g. `ed`, `sed -i` for targeted substitutions) — line
numbers from `symbols.start_line` and `symbols.end_line` pair naturally
with these. Avoid exact-string-match editors when the surrounding code
might appear more than once.

### What NOT to do

- ❌ `grep -r 'pattern' .` — use `lines_fts MATCH 'pattern'`
- ❌ `cat src/foo.rs` / `view src/foo.rs` — use `SELECT content FROM lines WHERE file=... AND line_num BETWEEN ...`
- ❌ `sed -n '80,160p' src/foo.rs` — same as above
- ❌ `find . -name '*.rs'` — use `SELECT file FROM files WHERE language='rust'`
- ❌ Routing around the shims with `/usr/bin/grep`, `command cat`, or `awk 'NR>=X && NR<=Y'`
<!-- END indexnudge AGENTS.md block -->
