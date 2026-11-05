---
title: "Run the full sanitizer gate without hiding descriptor-exhaustion cases"
status: open
kind: tooling
depends: []
gates:
  - "a consistent host compiler, linker, and sanitizer runtime start and run the complete fixture set"
  - "the real zero-fd memfd failure is tested without suppressing unrelated undefined-behavior checks"
  - "independent memory-error and arithmetic-error controls are rejected"
---

## What works

On the initial Linux host, GCC and Clang ordinary builds pass. This GCC
AddressSanitizer command also passes all eight fixture groups and the example:

```sh
cd sash-cpp
mk -p 1 'CXX=g++ -B/usr/bin' BUILD=build/asan \
  'SANITIZE=-fsanitize=address -fno-omit-frame-pointer' test
```

An independent heap-use-after-free control is rejected by ASan. An
independent signed-overflow control is rejected by UBSan with
`-fno-sanitize-recover=all`.

## Remaining failures

Clang 20's combined ASan/UBSan executable fails before the first fixture:
`SetAlternateSignalStack` tries to allocate zero bytes and reports EINVAL.
An empty `int main() { return 0; }` reproduces it, so it is not evidence
against the runtime.

GCC 9 needs `-B/usr/bin` here: its default linker resolves Homebrew glibc
components alongside the host sanitizer libraries and fails to link them.
Selecting the system tools resolves that failure.

The resulting combined ASan/UBSan build passes six groups, then reports an
invalid vptr in a standard `system_error` constructor during the zero-fd
probe. A standalone derived `std::system_error` control reproduces it
without sash code. `strace` shows the sanitizer's pipe attempts failing
with EMFILE after `RLIMIT_NOFILE` becomes zero. Restoring the limit before
inspecting the caught exception does not help: construction already fails.

Minimal independent control, compiled with the same combined sanitizer
flags and `-fno-sanitize-recover=all`:

```cpp
#include <cerrno>
#include <sys/resource.h>
#include <system_error>

class FixtureError final : public std::system_error {
  public:
    using std::system_error::system_error;
};

int main() {
    struct rlimit limit {};
    if (::getrlimit(RLIMIT_NOFILE, &limit) != 0) {
        return 3;
    }
    limit.rlim_cur = 0;
    if (::setrlimit(RLIMIT_NOFILE, &limit) != 0) {
        return 4;
    }
    try {
        throw FixtureError(EMFILE, std::generic_category(), "control");
    } catch (const FixtureError &error) {
        return error.code().value() == EMFILE ? 0 : 1;
    }
}
```

Resolve the host/tooling conflict or use a suitable host, then rerun the
complete gate. Do not remove the fault fixture, globally disable vptr
checks, change the runtime's exception type to appease instrumentation,
or count the partial UBSan run as passing. Keep the memfd allocation
failure real and distinguish it from later descriptor-duplication failure.

## Fourteenth-unit evidence: initialize metadata before real fd limits

The temporary-redirection controls exposed a related cold UBSan stall at
RLIMIT_NOFILE=5, with fd 0 free but no allocatable private slot.
GDB stopped in sanitizer `ReserveStandardFds` duplicating invalid fd -9
after its lazy mapping read could not be relocated. An independent derived
`std::system_error` control reproduces status 124; initialization before
the same real constraint yields 0, and an unconstrained control yields 0.

The new fd-limit scaffolding initializes ordinary exception/vptr metadata
before closing slots or lowering limits. Both partial pipe/channel failures
remain real, with no disabled UBSan category. The six-group redirect
selector, file golden, and signed-overflow detection canary pass.
[Journal 021](../journal/021-temporary-redirections.md) records the evidence.

This does not close the task: the complete combined sanitizer gate and
original zero-fd memfd case have not been rerun or claimed resolved.

## Fifteenth-unit evidence: fatal entry and bad_alloc metadata

The EXIT/ERR unit passes all 73 groups and fifteen goldens under supported
GNU Linux ASan and actual Apple ASan. Fresh independent heap-use-after-free
canaries are rejected on both; Apple reports the diagnostic and aborts
with shell status 134, rather than GNU's status 1.

The all-category GNU UBSan lifecycle selector and both lifecycle goldens
pass with `-fno-sanitize-recover=all`; a fresh signed-overflow canary fails
with its intended diagnostic. The real-limit fixture now initializes
`bad_alloc` metadata alongside the existing exception/vptr warm-up before
applying limits. Allocation remains denied during terminal entry, and
the startup control proves exactly one denied allocation before recovery,
not an allocating handler or a disabled sanitizer category.
[Journal 022](../journal/022-checked-exit-explicit-err.md) records the scope.

This focused result is not a full-runtime, combined-sanitizer, or original
zero-fd resolution claim. The task remains open.
