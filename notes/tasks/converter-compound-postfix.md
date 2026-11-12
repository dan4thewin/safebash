title: Converter handles compound-command postfix rejections
status: ready
depends: [align-parser-with-decisions]
gates: []

# Converter: compound-command postfix operators

The parser now rejects redirects, pipes, &&, ||, and & on compound
commands (SB4021–SB4024). The converter (`sash-convert`) must
detect these patterns in input bash and rewrite them to the sash
idiom: wrap in a function, apply the operator to the function call.

## Input patterns to detect

```bash
for x in a b c; do echo $x; done > file
while cond; do work; done | cmd
if cond; then cmd; fi && other
while true; do bg_work; done &
{ cmd1; cmd2; } > file
```

## Output (sash idiom)

```bash
fn __for_1 {
    for x in a b c; do echo $x; done
}
__for_1 > file

fn __while_1 {
    while cond; do work; done
}
__while_1 | cmd

fn __if_1 {
    if cond; then cmd; fi
}
__if_1 && other
```

## Notes

- Generated function names should be unique (`__compound_N` or similar)
- The converter should emit a comment indicating the rewrite:
  `# sash-convert: wrapped compound command for postfix operator`
- For `&` (background), the function call gets `&` appended
- Group `{ }` is also compound — same treatment
