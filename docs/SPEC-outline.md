1. Language model
    1. Static code and command identities; no runtime evaluation, source inclusion, aliases or name indirection.
    2. Fixed semantics; no runtime shell options.
    3. Statements perform actions; expressions yield values. Bindings, mutations, dispatch and `if`/loop statements have no result.
    4. Scalar values, boolean verdicts and command statuses are distinct.
    5. No implicit word splitting, environment-variable bindings or argument globbing.
    6. Blocks and capture bodies do not introduce subshells.
2. Source and words
    1. UTF-8 source; runtime strings may contain arbitrary bytes.
    2. Names: `[A-Za-z_][A-Za-z0-9_]*`; reserved keywords, builtins and intrinsics excluded. `__` is available.
    3. Single quotes are literal; double quotes interpolate. Adjacent fragments concatenate.
    4. Backslash is literal except in `$'...'` and line continuation.
        1. Unquoted or double-quoted backslash-newline disappears; final unquoted backslash disappears.
        2. C-string escapes: `\n \t \r \a \b \e \E \f \v \\ \' \"`.
        3. `\x`: one or two hex digits, at most `7f`; NUL is `\x00`.
        4. `\u`/`\U`: up to four/eight hex digits; valid Unicode scalar, encoded as UTF-8.
        5. Missing digits, invalid scalars, octal escapes and `\c` forms reject; unknown nonnumeric escapes retain backslash.
        6. C-strings also expand inside double quotes.
    5. A dollar starting no recognized word expansion is literal; malformed recognized expansions reject.
    6. Comments start with `#` at source start or after space, tab or newline; run to newline.
        1. A hash glued to `; & | < > (` is an error; expansion closers are not included.
    7. Comparison operands and binding values absorb metacharacters as word text; command arguments retain connector/redirection syntax.
    8. Separate ordinary statements with newline or `;`; completed whole-statement closers need no extra separator.
        1. A closer within a value does not end its containing statement.
        2. Binding lists require a separator before the following statement; bindings execute in order.
        3. Bracketed tests/arithmetic allow newlines and comments; an owed operand permits continuation.
        4. Next-line `else`/`elif` may cross blank/comment lines, not `;` or another statement.
        5. Pipes, logical connectors, redirects and `&` do not attach backward across a newline.
3. Values and bindings
    1. Scalars: `str` and signed 64-bit `int`; no implicit string/integer coercion.
        1. Quoting renders an integer as a string.
        2. A new bare numeric-word binding is a string; an integer target accepts an integer literal.
        3. Arithmetic values use `$((...))`, not an unwrapped expression.
    2. Strings retain interior NUL; lengths and substring positions count bytes.
    3. Authored integers: decimal, `0x`, `0o`, `0b`; valid digits and range required.
        1. Decimal leading zeros reject except `0`; output is decimal.
        2. Runtime conversion accepts complete, representable prefixed integers and leading-zero decimal strings.
    4. Declarations: `str`, `int`, `array`, `map`, `int array`, `int map`; `const` freezes a scalar's value.
        1. `array a=(VALUE...)`; `map m=([KEY]=VALUE...)`; omitted initializers give meaningful zeros.
        2. Collection introduction requires its keyword or an unambiguous auto-declaring filler.
    5. First binding fixes type and collection kind; no redeclaration or shadowing.
    6. Meaningful zeros: `""`, `0`, empty collection; known but unwritten paths receive them. Unknown names reject.
    7. Every named binding needs a read. `_` is a repeatable, unreadable discard target.
    8. Assignment replaces; string `+=` concatenates; integer mutation calculates; array `+=` appends an array-shaped value.
        1. Declaration cannot append; append requires an existing mutable target. No map append.
        2. Whole-value shape must match the target; scalar assignment cannot create a collection.
        3. `unset` clears a declared collection or removes a cell; missing cells are harmless. Scalars cannot be unset.
4. Scope and lifetime
    1. Main-scope bindings are globals; named functions can access globals, not caller locals.
    2. A capture inherits live enclosing bindings; new names belong to that capture.
    3. Control-flow bodies introduce no scope; ordinary loop bindings survive the loop.
    4. Each function/capture invocation and self-tail entry gives newly owned locals meaningful zeros.
        1. Globals, arguments, collection parameters and active enclosing bindings retain their state.
    5. Ordinary `const` is scalar-only and unavailable in loop bodies; validation does not violate constness.
    6. Leading-underscore binding introductions create immutable scratch instances.
        1. The target must not already exist; another source binding site is redeclaration, not shadowing.
        2. Re-executing the same construction site creates a fresh instance, without the prior value, taint or regex proof.
        3. Skipping construction changes nothing; loop exit does not destroy the instance.
        4. Scalar initialization/capture, readers, loop binders, conversion targets and collection construction share this rule.
        5. No later assignment, cell write, append, unset or mutation through an alias; value-preserving validation is allowed.
        6. `_` remains discard, not readable scratch. Function names do not acquire scratch semantics.
5. Expansion and argument lists
    1. `$x`/`${x}` read a scalar; `$a[K]`/`${a[K]}` read one cell; `@a`/`@m` expand collection values.
    2. Scalar expansion never splits. A quoted word is one string; splats cannot be quoted.
    3. Reached words and components evaluate left-to-right; earlier values survive later effects.
    4. Short-circuited operands do not evaluate.
    5. Scalar/cell dispositions preserve the subject's type.
        1. `:-WORD`: default on meaningful zero in word context; integer subjects require an integer-literal default.
        2. String `:?MESSAGE`: require nonempty; otherwise abort.
        3. `:~`: omit an empty string in a list position; integer zero remains `"0"`.
        4. Defaults/messages are literal-only; a literal dollar requires a wholly single-quoted operand.
        5. Omitted or empty `:?` text generates a subject-and-reason diagnostic; nonempty custom text is retained.
    6. `:!` trusts one read; `:!~`, `:!-WORD`, `:!?MESSAGE` combine trust with a disposition.
    7. Braceless dispositions/trust work unquoted; use braces for interpolation, bounded concatenation or operand whitespace.
        1. Substrings and substitution remain braced; other braceless colon text is literal.
    8. External-command collection splats require an explicit empty disposition, regardless of earlier guards.
        1. `@{a:-WORD}` supplies exactly one literal argument when empty, including an empty argument.
        2. `@{a:?MESSAGE}` aborts when empty; `@{a:~}` supplies zero arguments.
        3. Bare external `@args` instead requires a statically guaranteed argument.
    9. `join COLLECTION [SEPARATOR]` produces one string; literal separator, default one space, empty means concatenate.
6. Collections
    1. Arrays have integer keys; maps have string keys. Elements are uniformly strings or integers.
    2. Arrays may be sparse; `len COLLECTION` counts present entries. Missing cells yield their element-type zero.
    3. Direct subscript keys are one explicit value: decimal integer, expansion, nested value/arithmetic expression, quoted string or identifier.
        1. Bare identifiers are literal map keys, not variable reads.
        2. Bare decimal indices are nonnegative and fit `int`; numeric map keys require quoting.
        3. No mixed bare expressions or implicit nested subscripts; keys are not silently converted.
        4. Every index is an independent read, including a write's index; trusting the cell does not trust its index.
    4. Literal elements, appends and cell writes require clean values.
    5. Array literals, appends and word-list loops accept glob expansions in source order; quote a literal `=~` element.
    6. `$(keys MAP)` supplies keys only as the sole `for` source; `keys MAP ARRAY` supplies a key snapshot in map iteration order.
    7. String-array fillers: `read`, `argv`, `keys`, `match`, `matchall`, `glob`, `rglob`.
        1. Ordinary destinations append a complete batch; eligible undeclared targets start empty.
        2. Scratch destinations construct a fresh immutable batch.
        3. Failure publishes no partial batch: an existing ordinary destination is unchanged; a new or scratch destination is empty.
        4. EOF/no-match describes this operation, not accumulated contents.
        5. Publication does not roll back consumed input, argument effects or file operations.
    8. Array-to-map assignment consumes key/value pairs and requires an odd-tail disposition.
        1. `@{a:-KEY}` makes the trailing item the value of literal `KEY`.
        2. `@{a:?MESSAGE}` aborts on odd count; `@{a:~}` drops the tail; `:!~` also trusts the read.
        3. Empty array produces empty map. Map-to-array conversion requires explicit traversal.
7. Arithmetic and string operations
    1. Integer arithmetic, bitwise operations, comparisons, lazy logic and ternaries.
    2. `((...))` statements mutate without a value/status; `$((...))` evaluates without mutation.
    3. Arithmetic conditions consume a read-only result, true when nonzero.
        1. Statement commas contain only mutations.
        2. Condition commas permit mutation preludes followed by one read-only result.
        3. Postfix `++`/`--` are statement-only; prefix forms and writes used as values reject.
    4. Bare arithmetic names must resolve to bindings or valid nullary integer calls.
    5. Bare `a[EXPR]` accesses an integer-array location using an arithmetic index; dollar-prefixed subscripts import values only.
        1. Bare map subscripts reject; string cells follow the same conversion rules as string scalars.
    6. String-to-integer reads require `:-N` or `:?MESSAGE`.
        1. Invalid, empty, partial or out-of-range conversion selects the fallback or aborts.
        2. `:-N` requires a canonical decimal fallback; `i?` checks validity but does not replace the disposition.
    7. Overflow, zero division and negative shifts are fatal; no silent wrap or substitute result.
        1. Literal-expression-proven zero divisors and negative shifts reject, including in dead branches.
        2. Proven overflow remains a runtime fault; minimum integer modulo `-1` is zero.
        3. Shift counts are nonnegative, not masked; large left shifts yield zero, right shifts sign-fill.
    8. String operations: `strlen VALUE`, `toupper VALUE`, `tolower VALUE`, `basename VALUE`, `dirname VALUE`, `${x:OFFSET:LENGTH}`.
        1. Every string operand must be a string; quote integers explicitly.
8. Functions and calls
    1. `fn NAME { ... }`; `NAME() { ... }` and `function NAME { ... }` are aliases.
    2. Exactly one global, hoisted definition per name, directly in the main statement list; no replacement or overloading.
    3. Unannotated functions return void or status; `-> int` declares an integer result.
        1. Bare `return` and fall-through supply no result.
        2. Every returning nonvoid path supplies the declared/inferred kind; constant results, including zero, are legal.
        3. No implicit last-command return; `return PRODUCER` forwards a compatible kind, including through redirects.
        4. Numeric return operands must be integers; string conversion is explicit.
    4. Integer producers work as `$(f ...)` values and `(f ...)` arithmetic calls; not as status predicates or direct pipeline stages.
    5. Self-tail calls repeat with fresh locals; other recursion rejects. Collection-slot tail rebinding is unsupported.
        1. Evaluate arguments before rebinding the next invocation's arguments/slots.
    6. Function arguments form immutable, clean, zero-indexed `args`.
        1. `$1` denotes `args[0]`; `$#` denotes `len args`; `$0` remains the program name.
        2. No writes or attest-all on `args`; use `@args`, cell reads, `len` or `join`.
    7. One inferred scalar-argument contract per function.
        1. Literal positions are contiguous: required prefix, then optional tail.
        2. Bare/`:?` reads require a position; only `:-`/`:~` reads make it optional. Mixed classifications reject.
        3. Whole-argument use or dynamic indexing makes the function variadic; counting alone does not.
        4. Runtime-sized splats/elisions require a variadic callee and a guaranteed fixed prefix.
    8. Scalar arguments precede up to five collection identities, written `=name`, bound as `A` through `E`.
        1. Quoted `=name` is scalar text. A slot aliases the actual collection and preserves immutability.
        2. One collection count and array/map plus string/int signature across every call and forwarding path.
        3. Identities must be distinct and cannot alias a global named by the callee or its transitive callees.
        4. Conditional access, writes, validation, metadata and identity forwarding all count; taint bypass grants no exception.
        5. Passing identity is not a value read; actual reads count, and write-only outputs still require a reader.
9. Results and failure handling
    1. Operational status zero means success; unhandled, undiscarded failure aborts.
    2. Boolean verdicts, nonvoid function results and `getenv` status require explicit consumption or discard.
    3. Conditions consume boolean/status results, not integer values, void calls, bindings or control-flow statements.
        1. Redirects do not change result kind.
    4. `- COMMAND` discards that eligible command/call result only.
        1. No reach into operands, captures or called bodies; fatal faults remain fatal.
        2. No prefix on assignments, mutations, resultless control flow, declarations or control transfers.
        3. A discarded result cannot also be tested, forwarded or captured as that same result.
        4. Capturing a value does not create a separate status to discard.
    5. `!` consumes and inverts one boolean/status result; its result must itself be consumed.
        1. No `!` combined with `-` on the same unit.
    6. Reject known result-only unused work, even with explicit discard; retain useful effects and uncertain cases.
        1. `:` deliberately evaluates/discards its operands; it does not make an otherwise pointless enclosing test useful.
    7. `$(status)`/`$?` is available only in the first evaluated handling expression, not later statements.
        1. Named and anonymous bodies start without caller-status permission; direct getters use the surrounding context.
    8. Recoverability follows cause and available recovery state, not setup position or missing handling syntax.
        1. Ordinary path-specific I/O failures are recoverable; fork, heap and arithmetic faults are fatal.
        2. Handling one operation never handles unrelated operand/body failures.
10. Tests and control flow
    1. `if COND { ... } [elif COND { ... }]* [else { ... }]`; `else if` nests another `if`.
    2. `while COND { ... }`, `until COND { ... }`, `for NAME in ITEMS { ... }`, `for ((INIT; COND; STEP)) { ... }`.
        1. `then`/`fi` and `do`/`done` are equivalent delimiters; do not mix delimiter styles within a construct.
        2. Conditions must exist and supply a real result.
        3. Word-list `for` requires `in` and a written source item; no semicolon before `in`.
        4. Runtime-empty data is legal; an empty string is one item.
        5. Named-collection iteration uses a sole bare splat; no stdout/file-capture word-splitting iteration.
        6. `break [N]`/`continue [N]` target enclosing loops in the current body; a while/until condition belongs to its loop.
    3. `&&`/`||` short-circuit; do not mix them at one list level.
        1. Left/middle clauses and a consumed tail must supply results; an unconsumed final action need not.
        2. `cmd && { ... }` is `if cmd { ... }`; `cmd || { ... }` is `if ! cmd { ... }`.
        3. No standalone command groups or subshell groups.
    4. Boolean `[[...]]`: explicit tests, lazy conjunction/disjunction and negation.
        1. String `==`/`!=` are exact; `<`/`>` use unsigned-byte ordering; quotes do not change comparison semantics.
        2. String operands must be strings; numerical comparisons use arithmetic.
        3. `-n`/`-z` test nonempty/empty; no implicit nonempty test or `=` equality.
        4. `i?` tests integer validity; `p?`/`g?` test nonempty ASCII printable/graphical strings; `r?` tests regex validity.
        5. File-age tests use strict nanosecond timestamps; `-ef` compares device/inode identity.
        6. Written-static vacuous string tests reject; variables are not constant-propagated for this rule. File tests remain runtime tests.
        7. Glued comparison operators require a left operand beginning `$`; otherwise separate operands/operators.
    5. Dispatch: `[[ SUBJECT == PAT { ... } ... || { ... } ]]`, or `=~` for regex.
        1. One mode; subject evaluated once, patterns lazily in source order; first matching arm only.
        2. Exact arms may list alternatives; regex arms take one pattern. Optional default; otherwise no match does nothing.
        3. No result or fall-through; reject a written-static subject with entirely static patterns.
11. Captures
    1. `$(...)` captures stdout unless its direct producer yields a typed value.
    2. Stdout bodies execute in the same process, inheriting live bindings; global/enclosing effects persist.
        1. Fresh private locals, status/latch permissions, loop and return context; nested bodies obey the same boundary.
        2. Anonymous bodies may return void only; named callees retain their own return kinds.
        3. Process-replacing `exec` is forbidden directly and through callees; descriptor-only `exec` and ordinary externals remain legal.
        4. A potentially connected writer is required after redirects.
    3. An integer-producer capture always consumes its integer, including through redirects; stdout remains an ordinary side effect.
    4. Value-only intrinsics occupy their own direct capture, not arbitrary statement positions; a statusless value capture cannot use `-`.
    5. Capture-conditions combine an untyped binding with one eligible producer.
        1. External command, named predicate, otherwise-legal pipeline, `getenv`, direct `$(<PATH)`, or value-bearing regex producer.
        2. Execute once; bind this attempt's bytes on success and failure, including empty/partial failure output.
        3. Consume the producer's actual result; no assignment status or automatic taint clearance.
        4. A named predicate's same-process effects persist on either outcome; completed empty file reads succeed.
        5. No typed declaration, general expression, void/integer function or multi-statement-body capture-condition.
12. Input, output and process values
    1. `read [SOURCE] ARRAY` consumes stdin, or splits scalar `SOURCE`, on NUL only.
        1. Bulk read is not a while/until condition, including through a pipeline.
    2. `readline SCALAR` consumes one newline-delimited record, removing the delimiter and preserving other bytes.
        1. Ordinary scalar targets replace; scratch targets construct a fresh instance, including an EOF attempt.
    3. Readers take no flags; input acquisition and result handling do not imply validation.
    4. `echo` joins arguments with spaces and adds newline; `write` omits newline. Neither takes flags.
        1. Join collections explicitly for echo.
        2. Each descriptor-backed invocation emits one complete write request after arguments and redirects.
        3. Pipe/FIFO non-interleaving is guaranteed only through `PIPE_BUF`, not for arbitrary output sizes/destinations.
    5. `$(<PATH)` reads a file directly; uncaught read failure aborts before assignment.
    6. Program arguments: `$(argc)` includes argument zero; `$(arg INDEX [LITERAL_DEFAULT])` reads one argument.
        1. Missing index aborts without a default; literal indices are nonnegative decimal digits, leading zeros allowed; computed indices are integers.
        2. `argv [INDEX] ARRAY` supplies the tail, default index zero, beyond-end empty.
        3. `$(argv [INDEX])` is a source splat only as the sole `for` source.
        4. `$1` and `$#` are function-only; program arguments use the explicit intrinsics.
    7. `$(getenv NAME)` reads one environment name: present-empty succeeds; absent returns empty and false.
        1. Explicit presence handling or discard is mandatory; capturing/attesting the string is not handling.
        2. `$(- getenv NAME)` retains the tainted string while discarding presence.
    8. Process/environment values: `getpid` (also `$$`), `getppid`, `getcwd`, `epochseconds`, `random`.
        1. `$(signum)` is the latest recorded signal.
13. External commands and descriptors
    1. External command heads resolve to absolute executable regular-file paths at compilation.
        1. Bare names use compile-time `PATH`, skipping relative entries; relative command paths and runtime head expansion reject.
        2. No runtime `PATH` search; explicit `/usr/bin/env` may delegate lookup or per-command environment changes.
    2. External arguments containing interior NUL abort rather than truncate.
    3. Evaluate and retain arguments first; then evaluate/apply attached redirects in order; run the body last.
        1. Stop on first redirect failure; do not run the body or undo earlier argument/file effects.
        2. Restore scoped descriptors before continuing after a handled failure.
    4. Filename targets are strings and provably nonempty.
        1. Every string variable/cell component needs its own nonempty default or require disposition; neighboring literals do not suffice.
        2. Explicitly quoted integers qualify; direct captures/intrinsics must be bound before use as targets.
        3. Empty defaults and elision do not establish nonemptiness.
    5. Redirect/close descriptor numbers are literal integers in `0..2147483647`.
    6. `cd DIRECTORY` takes exactly one directory; `close FD` closes one descriptor.
    7. Descriptor-only `exec` changes descriptors persistently; `exec PROGRAM ...` replaces the process and does not return on success.
    8. Unhandled operational failures report one contextual, program-prefixed line; handled failures are silent.
        1. Diagnostics use the invoker's stderr, not temporary command redirects; persistent descriptor changes redefine that channel.
        2. A child's own exec-error diagnostic is separate.
14. Pipelines and background jobs
    1. Ordinary stages run in children; stage arguments evaluate there after pipe plumbing, before attached redirects.
    2. User-function filters remain eligible when pipeline output is captured; integer-returning functions are not direct stages.
    3. Direct state-only/control-transfer stages reject; `echo`, `write`, `:` and eligible functions/externals retain their roles.
    4. Final `read`, `readline` or reader-owning `while` runs in the parent.
        1. The pipe belongs to the designated reader, not parent stdin; other parent reads use ordinary stdin.
        2. Calls, captures, redirects and background bodies do not inherit reader ownership.
        3. Parent-reader stages take no attached redirects.
    5. Aggregate the worst unsuppressed stage status; any unsuppressed failure fails the pipeline.
        1. `-` excludes one stage's result, not its execution or body checks.
        2. Only the terminal stage permits `!`; invert before aggregation. No whole-pipeline head negation.
        3. A consumed aggregate needs an unsuppressed stage; all-suppressed pipelines are statement-only.
        4. Final reader results may be discarded/negated; reader loops have no result and accept neither prefix.
    6. Pipelines may be whole conditions or final actions, not inline left operands of `&&`/`||`.
    7. Consume concurrently, then close, reap and check.
        1. Early departure cleans nested readers inner-first; partial construction closes pipes and waits for started stages.
        2. Failed construction does not execute its reader or overwrite the reader target.
        3. No forced kills/timeouts; completed consumer effects persist. Producer SIGPIPE follows ordinary status rules.
    8. `COMMAND &` starts a job; no immediate status consumer.
        1. A single external job execs directly; `getcpid` names that command, with no automatic completion report.
        2. Direct `exec COMMAND &` is redundant and rejected; explicit function jobs retain their checked bodies.
        3. Direct state-only actions/control transfers cannot be backgrounded.
        4. Waiting is optional; jobs need not end with their parent.
    9. `wait [SELECTOR]` reaps one child using `wait(2)`/`waitpid(2)` selection; returns pid or `-1`, never implicitly aborts.
        1. Consume its integer or write `- wait`; it does not change `status`.
        2. `$(waitstatus)` reads the reaped outcome, or `-1` when none was reaped.
        3. `$(getcpid)`/`$!` and `waitstatus` are readable only in the immediately following statement under control-flow rules.
        4. Named/anonymous bodies start without inherited latch permission.
15. Taint and attestation
    1. Environment, program arguments, descriptor reads and kernel-backed captures introduce taint, including builtin-only pipes.
    2. Literals, clean arithmetic, managed process values and clean function arguments are clean; memory copies introduce no new taint and erase none.
    3. Every value read is checked, not just external-command arguments; ordinary use requires legal validation or explicit per-read trust.
    4. `:!` bypasses one read's taint guard; it does not validate or clear the source binding.
    5. `attest SUBJECT [[ PREDICATE ]]` validates unchanged data; unary shorthand and collection-wide forms share the predicate rules.
        1. Every subject/binder must be tested, on the left of binary tests; other inputs must be clean.
        2. `attest ELEMENT in ARRAY [[...]]`; `attest KEY VALUE in MAP [[...]]`; binders are predicate-local.
        3. `attest SUBJECT OP` applies unary `OP`; collection shorthand applies it to every binder, including both map binders.
        4. No captures, intrinsic calls, substitution sugar or arithmetic expansions anywhere in predicates, including indices/defaults.
        5. Evaluate once with short-circuiting; collection validation stops at the first counterexample; empty collections pass.
    6. `attest SOURCE (( TARGET = EXPR ))` checks and converts to an integer.
        1. Reference the source; no calls; source and target are distinct scalar names.
        2. Target is a new integer or an existing mutable integer; a failed new target is zero.
        3. Const sources may be validated; failed conversion leaves the source unresolved.
    7. Taint-clearing attest must immediately follow acquisition in the same body/block.
        1. Condition branches, reader-loop entry and source-iteration entry provide their defined adjacency.
        2. Each binding advances adjacency separately; no batch of captures followed by batch attests.
    8. Ordinary caught attest must resolve every continuing path.
        1. Success validates; failure must directly replace the whole mutable subject with clean data, or leave the continuation.
        2. Replacement permission is subject-specific and does not enter helpers, captures or children.
        3. Ordinary writes, unset, clean loop rebinding and trusted self-copy do not clear live unresolved taint.
        4. Noncontinuing paths retain their real outgoing state and terminal-handler obligations.
    9. Body ownership
        1. Resolve own acquisitions in the producing body, or immediately after its sole syntactic call site.
        2. Ignore self-tail edges when counting sites; anonymous bodies are single-use.
        3. A body cannot attest received taint.
        4. Shared bodies cannot export unresolved own taint through globals/slots; callers cannot attest their tainted output.
        5. Internally validated, buffer-only output can cross cleanly; a later kernel read is a new acquisition.
        6. Child effects do not change parent bindings; parent readers retain their own effects.
    10. Caught attest on immutable scratch
        1. Ordinary reads only in the attest-success arm.
        2. Failure may fall through without replacement; the instance remains intact, immutable and unresolved.
        3. Failure-arm and later ordinary-data reads require `:!`; they cannot supply regex patterns.
        4. Permission never escapes the construct, even when failure exits, returns or continues.
        5. Standalone checked attest permits subsequent use; ordinary mutable bindings/consts gain no scratch exception.
    11. Disabling taint checks does not waive typing, arity, nonaliasing, predicate structure or regex-proof rules.
16. Regexes and substitution
    1. Case-sensitive, unanchored matching; `^`/`$` anchor the complete subject.
    2. Operators: `. * + ? | ( ) [ ] [^ ]`, anchors and counted repetition with authored bounds at most 32; no lookaround or pattern backreferences.
        1. `\d` is ASCII digits, `\w` ASCII letters/digits/underscore, `\s` tab-through-CR plus space; uppercase forms negate.
        2. Dot and negated classes exclude newline; escape opening metacharacters for literal use and `\-` for a literal class hyphen.
        3. Pattern `\xNN` requires exactly two hex digits; quoting does not literalize regex syntax.
    3. Literal pattern operands are checked at compilation.
    4. Runtime pattern operands must be immutable and successfully tested by `r?` on every reaching path for that same value/instance.
        1. `r?` validates the complete candidate using the consumers' dialect; malformed candidates, even literals, give quiet false.
        2. Resource faults remain fatal.
        3. Cleanliness, immutability, freshness and regex validity are independent.
        4. Whole-scalar quoting preserves proof; byte-changing composition requires validation of the finished pattern.
        5. Checked immutable elements may retain proof during iteration; a missing-cell default was not checked.
        6. Calls, globals, captures, children and handlers gain no proof merely by naming a previously checked binding.
        7. No trusted (`:!`-bearing) pattern reads, even after attest success or with taint checks disabled.
        8. Trusted copies are ordinary data and need independent regex validation; subject/replacement trust follows ordinary rules.
    5. `match SUBJECT PATTERN` yields a boolean and, in value form, the whole match.
        1. `match SUBJECT PATTERN ARRAY` fills whole match plus up to nine capture groups.
        2. `matchall SUBJECT PATTERN ARRAY` fills groups per match, or whole matches when ungrouped; no scalar form.
    6. `sub SUBJECT PATTERN REPLACEMENT` replaces the first match; `gsub` replaces all.
        1. Both yield text plus matched/no-match boolean; no match returns original text. Unchanged bytes can still mean matched.
        2. Replacement `\0` is the whole match, `\1` through `\9` groups, `\\` a backslash; no other replacement escapes.
        3. Value-bearing captures may consume both results in a capture-condition; `$(- sub ...)`/`$(- gsub ...)` retain text and discard boolean.
    7. `${x:s/RE/REPLACEMENT/[g]}` projects text; `x =~ s/RE/REPLACEMENT/[g]` mutates without a result.
        1. Slash delimiter and final slash required; fields interpolate; pattern proof still applies.
        2. Comma-chained edits run left-to-right; a pending comma permits newline but requires another edit.
        3. Each edit projects text; no chain-wide boolean.
    8. Regex subjects stop at the first interior NUL; substitution preserves the unexamined suffix.
17. Directory selection
    1. `glob ARRAY PATTERN...`; `rglob ARRAY DIRECTORY REGEX...`.
    2. Glob: at most one `*`, in the final filename component; literal directory prefix.
        1. No `?`, bracket wildcard, multiple stars or wildcard directory traversal.
        2. Filename patterns must start with literal `.` to select leading-dot entries.
        3. Computed pattern strings obey the same bounds; ordinary computed array elements do not become glob syntax.
    3. Regex selector matches basenames with ordinary regex rules and proof requirements; no hidden-file prefilter.
    4. Both omit `.`/`..`; sort each pattern's batch bytewise, concatenate in source order, retain duplicates.
    5. Results are clean, normalized absolute paths; relative prefixes use current directory, without resolving symlinks.
    6. Successful no-match is success with an empty batch; directory/read/current-directory failures are not empty success or relative fallback.
    7. Assignment, append and iteration sugar use checked results; they do not acquire a status result.
18. Termination and signals
    1. Optional, unique main-top-level `EXIT { ... }` and `ERR { ... }`; no function definitions inside them.
    2. Main process only; forked children run neither.
    3. Fall-through, every `exit [N]` and configured EXIT signals select EXIT; uncaught failure, `fail [N]` and first fatal faults select ERR.
        1. `exit` defaults to zero; `fail` to one; `fail 0` is legal. Cause selects the handler, not numeric status.
        2. An absent selected handler terminates; it does not select the other handler.
        3. Successful process-replacing exec runs neither handler.
    4. Handler fall-through preserves incoming status; explicit termination replaces it.
        1. Full integer status survives until final low-eight-bit process-status conversion.
        2. Incoming `status` is readable in the first evaluated handler expression.
    5. EXIT failure may enter ERR; exit inside either handler terminates directly; fail inside ERR terminates directly.
        1. No resumption or re-entry; ERR never transfers to EXIT.
        2. First fatal fault attempts ERR once, including heap exhaustion; another fatal fault terminates. Cleanup is not guaranteed.
    6. Handlers observe actual completed global writes.
        1. EXIT reads must be valid at every reaching orderly entry.
        2. ERR requires explicit trust for potentially tainted incoming globals; fresh acquisitions may be attested locally.
        3. Partial refresh does not justify attesting inherited input.
        4. Every recoverable failure reachable during ERR must be explicitly handled where it occurs, including helpers/operands.
        5. Restore invoker stdin/stdout/stderr before entry, not interrupted temporary redirects.
    7. `signal N IGN|DFL|FLAG|EXIT`: literal signal 1 through 64 except 9/19.
        1. IGN ignores; DFL restores kernel behavior; FLAG records delivery; EXIT records an orderly-exit request.
        2. `signum` starts at zero and records the latest FLAG/EXIT delivery; disposition at delivery governs the request.
        3. EXIT dispatch occurs at safe command/function/loop/reader checkpoints, normally with status `128 + signum`.
        4. Legal adjacent acquisition/resolution defers dispatch, not delivery, ordinary failure or explicit exit/fail.
