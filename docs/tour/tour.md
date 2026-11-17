---
title: A Tour of safebash
sub_title: "sash: a compiler for a safe subset of Bash"
---

Using this tour
===

Every `sash` snippet in this tour is a complete program. The
[GitHub edition](README.md) includes recorded output.

**When running under presenterm** (`mk -p 1 tour`):

* `→` / `l` and `←` / `h` move between slides; `Ctrl+P` opens the slide index.
* `Ctrl+E` compiles the snippet on the current slide with `sash` and runs the
  resulting binary. Its output appears below the code.
* `q` quits.

Programs are run as `snippet world 42`, so the tour can show program
arguments later on.

The tour is a plain markdown file, `docs/tour/tour.md`. While it is open in
presenterm, editing a snippet reloads it: change it, then press `Ctrl+E`
again. GitHub shows a static copy; `mk -p 1 tour-doc` refreshes its outputs.

<!-- end_slide -->

Hello, 世界
===

```bash +exec:sash
echo Hello, 世界
```

That looks like a shell script, and it is one — but nothing interprets it.
`sash` compiles it to a small, static x86-64 Linux executable. There is no
bash, no libc and no interpreter behind it at run time.

`sash` accepts a **safe subset** of Bash. It rejects at compile time what it
cannot prove safe, and the subset has a few spellings of its own that you
will meet along the way.

<!-- end_slide -->

Behaviour is fixed at compile time
===

```bash +exec:sash +expect:failure
cmd="echo hi"
eval $cmd # rejected
```

A compiled program cannot construct and run new code: there is no `eval`,
`source`, alias or indirect variable name.

Error messages carry a code (`SB4001`) and say what to write instead. Most
of this tour's "failing" snippets are there to show those messages.

<!-- end_slide -->

Commands are found when you compile
===

```bash +exec:sash
uname -s
date +%Y
```

Each external command is resolved against the **compile-time** `PATH` to an
absolute path, here `/usr/bin/uname` and `/usr/bin/date`. The binary never
searches `PATH` while it runs, so a planted `uname` cannot hijack it.

`sash --manifest prog.sh` lists the commands a program will run, and
`sash --extract prog` prints the source embedded in a compiled binary.

<!-- end_slide -->

<!-- jump_to_middle -->

Basics
===

<!-- end_slide -->

Variables
===

```bash +exec:sash
name=world            # a string
int count=3           # an integer
str greeting=Hello    # an explicitly typed string
echo $greeting, $name x$count
```

There are two scalar types: `str` and 64-bit signed `int`. A plain
`NAME=VALUE` introduces a string.

A variable's first binding fixes its type. There is no redeclaration or
shadowing, and no implicit conversion between the two types.

<!-- end_slide -->

Types do not convert silently
===

```bash +exec:sash +expect:failure
int n=3
n=three # type mismatch
```

The message says how to convert a string deliberately — with a
*disposition*, which we will meet shortly.

<!-- end_slide -->

Quoting
===

```bash +exec:sash
who=world
echo 'single quotes are literal: $who'
echo "double quotes interpolate: $who"
echo $'C-strings understand escapes:\tTAB'
echo "adjacent"'-'"pieces"' join'
```

Single quotes, double quotes and `$'...'` mean what they mean in Bash.
Backslash outside `$'...'` is literal, except before a newline.

<!-- end_slide -->

No word splitting
===

```bash +exec:sash +expect:failure
words="one two three"
for w in $words { echo [$w] } # scalar, not a list
```

In Bash an unquoted `$words` is split on spaces and then globbed. In `sash`
a variable is always exactly one value, quoted or not, so the rest of the
tour quotes only to keep spaces or special characters.

To work with several values, use an array — or read a command's output one
line at a time. Both come later.

<!-- end_slide -->

Integers
===

```bash +exec:sash
int a=7
int mask=$((0xf0))
((a += 1))
echo $(( a * 3 )) $(( a / 3 )) $(( a % 3 )) $(( a & mask )) $(( 1 << 10 ))
```

`$(( ))` computes a value; `(( ))` on its own updates variables.
Integer literals may be decimal, `0x`, `0o` or `0b`.

<!-- end_slide -->

Arithmetic never wraps
===

```bash +exec:sash +expect:failure
int big=$((0x7fffffffffffffff))
echo about to overflow
echo $(( big + 1 )) # overflow
```

Overflow, division by zero and negative shifts stop the program instead of
producing a wrong number. When the compiler can prove one from literals, it
refuses to compile.

<!-- end_slide -->

Strings into numbers
===

```bash +exec:sash
good=42
bad=forty-two
echo $(( ${good:?not a number} + 1 ))
echo $(( ${bad:-0} + 1 ))
```

A string enters arithmetic only with a stated plan for when it is not a
number: `:-N` substitutes `N`, `:?MESSAGE` stops the program.

Writing `$(( good + 1 ))` is a compile error.

<!-- end_slide -->

Defaults and requirements
===

```bash +exec:sash +expect:failure
str nobody
name=sash
echo ${nobody:-anonymous} $name:-anonymous
echo before ${nobody:~} after
echo ${nobody:?a name is required} # empty
```

These *dispositions* say what an empty value means:

* `:-WORD` substitutes `WORD`;
* `:?MESSAGE` stops the program with `MESSAGE`;
* `:~` drops the word entirely.

The braces are optional outside double quotes.

<!-- end_slide -->

Every variable is read
===

```bash +exec:sash +expect:failure
total=10
totl=20 # unused
echo $total
```

A variable that is written but never read is an error. It is usually a
typo, as here.

To deliberately throw a value away, assign it to `_`.

<!-- end_slide -->

Constants
===

```bash +exec:sash +expect:failure
const limit=3
echo limit is $limit
limit=4 # const
```

`const` freezes a scalar's value.

<!-- end_slide -->

String operations
===

```bash +exec:sash
s="Hello, World"
echo $(strlen $s) $(toupper $s) ${s:7:5}
path=/usr/local/bin/sash
echo $(dirname $path) $(basename $path)
```

`strlen`, `toupper`, `tolower`, `basename` and `dirname` are built in;
none of them start a process. `${s:OFFSET:LENGTH}` takes a substring.
Lengths and offsets count bytes.

<!-- end_slide -->

<!-- jump_to_middle -->

Flow control
===

<!-- end_slide -->

If
===

```bash +exec:sash
int n=7
if (( n % 2 == 0 )) {
  echo even
} elif (( n > 5 )) {
  echo odd and greater than five
} else {
  echo odd
}
```

Blocks use braces. The familiar `then`/`fi` and `do`/`done` spellings are
also accepted, but one construct uses one style.

<!-- end_slide -->

Tests
===

```bash +exec:sash
s=apple
if [[ $s == apple && -n $s ]] { echo exact match }
if [[ $s < banana ]] { echo sorts before banana }
if [[ -d /tmp && ! -e /no/such/file ]] { echo file tests too }
if [[ i? 42 && p? $s ]] { echo 42 is an integer, apple is printable }
```

`==` is exact comparison, never a pattern match. `<` and `>` compare
bytes, so the result does not depend on the locale. `i?` tests whether a
string is a valid integer.

<!-- end_slide -->

Loops
===

```bash +exec:sash
for fruit in apple banana cherry { echo $fruit }
for ((i = 0; i < 3; i++)) { echo i=$i }
int n=3
while (( n > 0 )) {
  echo n=$n
  ((n--))
}
```

There are also `until`, `break` and `continue`.

<!-- end_slide -->

And, or
===

```bash +exec:sash
[[ -d /tmp ]] && echo /tmp exists
[[ -d /no/such ]] || echo /no/such does not
```

`&&` and `||` short-circuit as usual. They cannot be mixed in one list:
`a && b || c` is a compile error, because it rarely means what it looks
like. Write an `if`.

<!-- end_slide -->

Dispatch
===

```bash +exec:sash
for v in apple 2026 "x y" {
  [[ $v == apple pear { echo $v: fruit }
           || { echo $v: something else } ]]
}
for v in apple 2026 {
  [[ $v =~ ^[0-9]+$ { echo $v: number }
           ^[a-z]+$ { echo $v: word } ]]
}
```

`case` is gone. A dispatch matches one subject against exact strings
(`==`) or regular expressions (`=~`) and runs the first matching arm.
`|| { ... }` is the default arm.

<!-- end_slide -->

<!-- jump_to_middle -->

Functions
===

<!-- end_slide -->

Functions
===

```bash +exec:sash
fn greet {
  echo "Hello, $1!"
}
greet world
greet sash
```

`fn NAME { ... }` defines a function; `NAME() { ... }` also works. Inside,
`$1` is the first argument and `$#` the argument count.

The compiler infers how many arguments a function takes, and checks every
call against it.

<!-- end_slide -->

Variadic functions
===

```bash +exec:sash
fn show {
  echo $# arguments: $(join args ", ")
  for a in @args { echo "  - $a" }
}
show one two three
show four
```

The canonical forms are `$args[0]` for `$1`, `$args[1]` for `$2`, and
`$(len args)` for `$#`. `args` is the function's read-only argument array.

Using the whole array (`join args` or `@args` here) makes the function
variadic; reading only its length does not.

<!-- end_slide -->

Status functions
===

```bash +exec:sash
fn is_even {
  return (( $1 % 2 == 0 ))
}
for n in 1 2 3 4 {
  if is_even $n { echo $n is even }
}
```

A function that returns a status works like a command in a condition.
There is no implicit "status of the last command": a status function
`return`s one on every path.

<!-- end_slide -->

Results are never ignored by accident
===

```bash +exec:sash +expect:failure
fn is_even {
  return (( $1 % 2 == 0 ))
}
is_even 3 # unhandled status
echo after
```

A status must be tested, or explicitly discarded with `- is_even 3`. The
same holds for commands, as we will see.

<!-- end_slide -->

Integer functions
===

```bash +exec:sash
fn square -> int {
  return $(( $1 * $1 ))
}
int nine=$(square 3)
echo $nine $(( (square 3) + (square 4) ))
```

`-> int` declares an integer result. Call such a function as `$(square 3)`,
or as `(square 3)` inside arithmetic.

<!-- end_slide -->

Recursion is a loop
===

```bash +exec:sash
fn fact -> int {
  return $(_fact $1 1)
}
fn _fact -> int {
  (( n = $1, acc = $2 ))
  if (( n <= 1 )) { return $acc }
  (( acc *= n, n-- ))
  return $(_fact $n $acc)
}
echo "20! = $(fact 20)"
```

A function may call itself only as its final action, which compiles to a
loop: no stack growth. Other recursion is rejected. `_fact` carries the
running product in its second argument; `fact` hides that from callers.

<!-- end_slide -->

Passing collections
===

```bash +exec:sash
fn total -> int {
  int sum=0
  for v in @A { (( sum += ${v:-0} )) }
  return $sum
}
int array prices=(3 4 5)
int array weights=(10 20 30 40)
echo prices $(total =prices), weights $(total =weights)
```

Write `=name` to pass a collection itself rather than its values. The callee
sees up to five passed collections as `A` through `E`.

<!-- end_slide -->

<!-- jump_to_middle -->

Collections
===

<!-- end_slide -->

Arrays
===

```bash +exec:sash
array fruit=(apple banana)
fruit+=(cherry)
fruit[5]=fig
echo $(len fruit) items: $(join fruit ", ")
echo first: $fruit[0], missing: [$fruit[3]]
for f in @fruit { echo - $f }
```

Arrays may be sparse. `len` counts the cells that are present, and a
missing cell reads as the empty string. `@fruit` *splats* the elements as
separate words.

<!-- end_slide -->

Maps
===

```bash +exec:sash
map age=([alice]=30 [bob]=25)
age[carol]=41
unset age[bob]
for name in $(keys age) { echo $name is $age[$name] }

int map seen
for w in a b a c a { seen[$w]+=1 }
echo a was seen $seen[a] times
```

Maps have string keys. Both kinds of collection hold either strings or,
when declared `int`, integers.

<!-- end_slide -->

Splatting into commands
===

```bash +exec:sash
array files=(a.txt b.txt)
printf '<%s>\n' @{files:~}
array none
printf '<%s>\n' @{none:-nothing}
```

A bare `@files` cannot be passed to an external command: if the array
were empty, the command would silently get fewer arguments. Say which
empty case you mean — `:~` (no arguments), `:-WORD` (one `WORD`) or
`:?MESSAGE` (stop).

<!-- end_slide -->

Globs
===

```bash +exec:sash
touch notes.txt todo.txt
array txt=(*.txt)
for f in @txt { echo $f }
```

Globs appear only where an array is built. Each match is an absolute path,
so a file named `-rf` can never be mistaken for an option.

A glob supports one `*`, in the final path component.

<!-- end_slide -->

<!-- jump_to_middle -->

Commands and failure
===

<!-- end_slide -->

Failure stops the program
===

```bash +exec:sash +expect:failure
echo before
ls /no/such/dir # missing directory
echo never printed
```

A command that fails and is not handled stops the program, with a message
naming what failed. There is no `set -e` to forget: this is the only
behaviour.

<!-- end_slide -->

Handling failure
===

```bash +exec:sash
if ! ls /no/such/dir 2>/dev/null { echo not there }
ls /no/such/dir 2>/dev/null || echo ls said $(status)
- ls /no/such/dir 2>/dev/null
echo still running
```

Test a result with `if`, `&&` or `||`. A leading `-` discards one command's
result on purpose. `$(status)` (also `$?`) is available only in the handling
context, before another command executes.

<!-- end_slide -->

Redirects
===

```bash +exec:sash
echo line one > out.txt
echo line two >> out.txt
wc -l < out.txt
ls /no/such 2>/dev/null || echo silenced
```

Redirects work as usual. A redirect target held in a variable must be
provably non-empty — write `> ${file:?no file}` rather than `> $file`.

<!-- end_slide -->

Pipelines
===

```bash +exec:sash +expect:failure
seq 1 5 | sort -r | head -3
false | true # failed stage
echo never printed
```

A pipeline fails if **any** stage fails, not just the last one: Bash's
`pipefail`, always on.

<!-- end_slide -->

<!-- jump_to_middle -->

Taint
===

<!-- end_slide -->

Input is tainted
===

```bash +exec:sash +expect:failure
user=$(getenv USER) || user=nobody
echo Hello, $user # tainted
```

Values from outside the program — the environment, its arguments, and
the output of commands — are *tainted*. A tainted value cannot be used
until it has been checked.

Environment variables are not shell variables: `$HOME` is an error, and
you write `$(getenv HOME)`.

<!-- end_slide -->

Attest
===

```bash +exec:sash
user=$(getenv USER) || user=nobody
attest user p?
echo Hello, $user
```

`attest user p?` is shorthand for `attest user [[ p? $user ]]`.

* `p?`: non-empty ASCII printable text, including spaces.
* `g?`: non-empty ASCII graphical text, excluding spaces.

On success the value is unchanged and no longer tainted. Ordinary code can
then rely on that character shape; it still decides what the value means.
Failure stops the program.

It must come right after the value was acquired, so the check and the input
it protects can be read together.

<!-- end_slide -->

Compound attests
===

```bash +exec:sash
s=$(arg 1 "")
attest s [[ -z $s || p? $s ]]
echo ${s:-empty}
```

`$(arg 1 "")` reads the first program argument, defaulting to empty if
absent. An attest predicate can combine ordinary tests with `&&` and `||`.

Here empty is explicitly allowed; otherwise the value must pass `p?`.
After validation, ordinary code handles the empty case with `:-empty`.

<!-- end_slide -->

Attest into a number
===

```bash +exec:sash
count=$(arg 2 0)
attest count (( n = count ))
echo twice $count is $(( n * 2 ))
```

`$(arg N DEFAULT)` reads program argument `N`, or `DEFAULT` when there is
none. This program was run as `snippet world 42`.

`attest SOURCE (( TARGET = SOURCE ))` validates a string as an integer and
stores it in the integer `TARGET`.

<!-- end_slide -->

When the check fails
===

```bash +exec:sash
name=$(arg 3 "")
if ! attest name p? {
  echo missing or non-printable, using a default
  name=Anonymous
}
echo Hello, $name
```

The tour supplies only two arguments, so the missing third defaults to
empty and fails `p?`.

Test an attest to handle failure yourself. On the failure path the variable
is still tainted, so it must be replaced with clean data — or the block must
leave (`exit`, `return`, `break`, `continue`).

<!-- end_slide -->

Trusting one read
===

```bash +exec:sash
input=$(arg 1 none)
echo argument length: $(strlen $input:!)
```

`:!` says this use is justified without a separate `attest`. Here we only
measure the input's length, which is safe for any contents.

Another example is dispatching on `sash-getopt` output and rejecting every
unexpected case: the code itself performs the validation, so an `attest`
would duplicate it.

Trust applies to this read only. It neither validates the data nor clears
the variable's taint for later reads.

<!-- end_slide -->

Checking every element
===

```bash +exec:sash
argv 1 words
attest w in words g?
for w in @words { echo word: $w }
```

`argv 1 words` fills an array with the program arguments from index 1 on.
`attest w in words g?` checks that every element is non-empty printable
ASCII without spaces. The loop can then use each word normally.

<!-- end_slide -->

Capturing output
===

```bash +exec:sash
kernel=$(uname -r)
attest kernel g?
echo kernel $kernel

fn banner { echo == $1 == }
b=$(banner tour)
echo $b
```

A command's output is input like any other. Capturing a function that only
prints clean values gives a clean value — nothing outside the program was
read.

<!-- end_slide -->

Capture and test at once
===

```bash +exec:sash
if line=$(grep '^root:' /etc/passwd) {
  attest line p?
  if [[ $line =~ ^root:[^:]*:0: ]] { echo root has uid 0 }
}
if ! other=$(grep '^nobody-here:' /etc/passwd) {
  echo "no match; captured [${other:!}]"
}
```

`if NAME=$(COMMAND)` binds the output and tests the command's status in one
step. The output is bound either way.

The attest establishes printable text; the following `if` checks its
application-specific meaning.

<!-- end_slide -->

Reading lines
===

```bash +exec:sash
seq 1 3 | while readline line {
  attest line (( n = line ))
  echo line $(( n * 10 ))
}
text=$(</etc/hostname)
attest text p?
echo hostname: $text
```

`readline` reads one line. At the end of a pipeline the loop runs in the
program itself, not a child, so variables it sets survive.

`$(<FILE)` reads a whole file without running `cat`.

<!-- end_slide -->

<!-- jump_to_middle -->

Regular expressions
===

<!-- end_slide -->

Matching
===

```bash +exec:sash
s=2026-11-13
if match $s ^\d+-\d+-\d+$ { echo looks like a date }
if match v1.2 ^v\d+\.\d+$ { echo dotted version }
```

`match SUBJECT PATTERN` tests for a match. `[[ SUBJECT =~ PATTERN ]]`
is the convenience spelling of the same operation.

Regex arguments are ordinary words. **Quotes are usually unnecessary**:
`\` is literal inside a word, so `\d` and `\.` reach the engine unchanged.
Quote spaces and structural punctuation (`(...)`, `{n}`, `|`) in builtin
arguments. Quoting does not make regex metacharacters literal.

<!-- end_slide -->

Capturing matches
===

```bash +exec:sash
s=2026-11-13
if year=$(match $s ^\d+) { echo year: $year }
if match $s '^(\d{4})-(\d{2})-(\d{2})$' parts {
  echo month: $parts[2], day: $parts[3]
}
if matchall $s \d+ numbers { echo $(join numbers ", ") }
```

`match` supplies a boolean and, when captured, the whole matched text.
`if year=$(match ...)` binds the text and tests whether a match occurred.
`$(- match ...)` keeps the text and explicitly discards that boolean.

With an output array, `match` appends the whole match followed by capture
groups 1-9. A newly introduced array starts empty.

`matchall` collects successive whole matches when the pattern has no groups,
or just the capture groups when it does.

<!-- end_slide -->

A regex attest
===

```bash +exec:sash
text=$(arg 2 "")
attest text [[ $text =~ \d+ ]]
echo first number: $(- match $text \d+)
```

The guarantee is exactly what the predicate says: digits occur somewhere
in the input, **not** that the whole string is numeric. Ordinary code can
then extract that digit sequence.

Literal patterns are checked at compile time; the input is checked at run
time. Try changing `arg 2` to `arg 1`: `world` has no digits.

<!-- end_slide -->

Replacing with sub and gsub
===

```bash +exec:sash
s=bananas
if first=$(sub $s a A) { echo first: $first }
echo all: $(- gsub $s a A)
echo original: $s
```

The underlying operations are `sub SUBJECT PATTERN REPLACEMENT` and
`gsub SUBJECT PATTERN REPLACEMENT`: replace the first match, or all matches.

Both produce text without changing `s`, plus a boolean saying whether a
match occurred. An identity replacement still counts as a match; no match
returns the original subject. Handle the boolean with a capture-condition
or discard it with `$(- ...)`. The call form needs no slash delimiters.

<!-- end_slide -->

Convenience spellings
===

```bash +exec:sash
s=bananas
echo first: ${s:s/a/A/}
echo all: ${s:s/a/A/g}
echo unchanged: $s
s =~ s/a/A/g
echo changed: $s
```

`${s:s/a/A/}` is shorthand for `$(- sub $s a A)`; adding `g` selects `gsub`.
These expansions keep only the text and do not change the subject.

`s =~ s/a/A/g` writes the result back to `s`. Its left side is a binding
name, not a `$s` read. The final slash is required.

<!-- end_slide -->

Chaining replacements
===

```bash +exec:sash
foo=aa_cc
foo =~ s/a/b/g, s/c/d/g,
       s/b/B/
echo $foo
echo ${foo:s/B/b/,s/d/c/g}
```

Commas chain edits **left to right**: each sees the preceding result.
Each edit owns its `g` flag; the last statement edit replaces only one `b`.

A comma also permits continuation on the next line. Expansion chains work
the same way, but produce text without changing `foo`.

<!-- end_slide -->

<!-- jump_to_middle -->

Processes and termination
===

<!-- end_slide -->

Background jobs
===

```bash +exec:sash
sleep 0.1 &
int pid=$(getcpid)
echo started job $pid
- wait $pid
echo it exited with status $(waitstatus)
```

`COMMAND &` starts a job; `$(getcpid)` (also `$!`) names it and must be
read in the immediately following statement. `wait` reaps one child and
returns its pid; `$(waitstatus)` reads that child's exit status, not
`$(status)`.

<!-- end_slide -->

Exit and fail
===

```bash +exec:sash +expect:failure
EXIT { - echo EXIT handler: status $(status) }
ERR { - echo ERR handler: status $(status) }
echo working
fail 3 # explicit failure
```

`exit [N]` ends the program in an orderly way and runs `EXIT`. `fail [N]`
or an unhandled failure runs `ERR`. The cause picks the handler, not the
number.

Try changing `fail 3` to `exit 3`.

<!-- end_slide -->

Signals
===

```bash +exec:sash +expect:failure
signal 15 EXIT
EXIT { - echo EXIT after signal $(signum), status $(status) }
kill -TERM $(getpid) # signal exit
sleep 1
echo not reached
```

`signal N EXIT` turns signal `N` into an orderly exit at the next safe
point, so the `EXIT` handler can clean up. `IGN`, `DFL` and `FLAG` are the
other choices.

`$(getpid)` is this process's ID; `$$` is the shorthand.

<!-- end_slide -->

<!-- jump_to_middle -->

Exercises
===

<!-- end_slide -->

Exercise: FizzBuzz
===

```bash +exec:sash
for ((i = 1; i <= 15; i++)) {
  # Print Fizz for multiples of 3, Buzz for multiples of 5,
  # FizzBuzz for multiples of both, and the number otherwise.
  echo $i
}
```

Edit `docs/tour/tour.md` to finish this program, then press `Ctrl+E` in
presenterm (or regenerate the GitHub edition).

Hint: an `if`/`elif` chain on `(( i % 15 == 0 ))` and friends works.

<!-- end_slide -->

Exercise: word frequencies
===

```bash +exec:sash
printf 'the cat\nthe dog\nthe end\n' | while readline line {
  # Count each word in a `int map` and print the counts.
  # `read line words` splits a string into an array on NUL only,
  # so split the words with a regex: see `matchall`.
  echo ${line:!}
}
```

You will need an `attest` for each line, and a map that survives the loop.

<!-- end_slide -->

<!-- jump_to_middle -->

Where next
===

<!-- end_slide -->

Where next
===

* `sash --help` lists the compiler's options; `man docs/sash.1` is the
  manual.
* `sash-convert script.sh` translates an existing Bash script into `sash`
  source, as far as it can, and marks what needs attention.
* `notes/decisions/` holds the design decisions behind every rule in this
  tour, one file each.
* `--no-taint` disables taint checking while you port a script. Nothing
  else is relaxed.

```bash +exec:sash
printf 'name=$1\necho "hi $name"\n' > old.sh
sash-convert old.sh
```

This safebash snippet creates a two-line Bash script and runs `sash-convert`
on it.
