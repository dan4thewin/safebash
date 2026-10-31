#include "sash/runtime.hpp"

#include <algorithm>
#include <cerrno>
#include <csignal>
#include <cstdio>
#include <cstdlib>
#include <fcntl.h>
#include <filesystem>
#include <functional>
#include <stdexcept>
#include <sys/resource.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

namespace {

std::size_t allocation_failure_size = 0;

} // namespace

// Test-only allocation faults leave the small launch allocations available.
void *operator new(std::size_t size) {
    if (allocation_failure_size != 0 && size >= allocation_failure_size) {
        throw std::bad_alloc{};
    }
    if (void *memory = std::malloc(size == 0 ? 1 : size)) {
        return memory;
    }
    throw std::bad_alloc{};
}

void *operator new[](std::size_t size) { return ::operator new(size); }
void operator delete(void *memory) noexcept { std::free(memory); }
void operator delete[](void *memory) noexcept { ::operator delete(memory); }
void operator delete(void *memory, std::size_t) noexcept { ::operator delete(memory); }
void operator delete[](void *memory, std::size_t) noexcept { ::operator delete(memory); }

namespace {

struct AllocationFailure {
    explicit AllocationFailure(std::size_t minimum) { allocation_failure_size = minimum; }
    ~AllocationFailure() { allocation_failure_size = 0; }
};

void require(bool condition, const char *message) {
    if (!condition) {
        throw std::runtime_error(message);
    }
}

void default_signal(int number) {
    require(std::signal(number, SIG_DFL) != SIG_ERR, "restore default signal disposition");
    sigset_t signals;
    require(::sigemptyset(&signals) == 0 && ::sigaddset(&signals, number) == 0 &&
                ::sigprocmask(SIG_UNBLOCK, &signals, nullptr) == 0,
            "unblock test signal");
}

struct ChildResult {
    int status;
    std::string diagnostic;
};

ChildResult run_child(const std::function<int()> &body) {
    int errors[2];
    require(::pipe(errors) == 0, "pipe test diagnostics");
    const pid_t pid = ::fork();
    if (pid < 0) {
        ::close(errors[0]);
        ::close(errors[1]);
        throw std::runtime_error("fork test child");
    }
    if (pid == 0) {
        ::close(errors[0]);
        if (::dup2(errors[1], 2) < 0) {
            std::_Exit(1);
        }
        ::close(errors[1]);
        try {
            std::_Exit(body());
        } catch (const std::exception &error) {
            std::fprintf(stderr, "child: %s\n", error.what());
            std::_Exit(1);
        }
    }
    ::close(errors[1]);
    std::string diagnostic;
    char buffer[1024];
    while (true) {
        const auto count = ::read(errors[0], buffer, sizeof(buffer));
        if (count < 0 && errno == EINTR) {
            continue;
        }
        require(count >= 0, "read test diagnostics");
        if (count == 0) {
            break;
        }
        diagnostic.append(buffer, static_cast<std::size_t>(count));
    }
    ::close(errors[0]);
    int status;
    pid_t waited;
    do {
        waited = ::waitpid(pid, &status, 0);
    } while (waited < 0 && errno == EINTR);
    require(waited == pid && WIFEXITED(status), "test child did not exit normally");
    return {WEXITSTATUS(status), std::move(diagnostic)};
}

void in_child(const std::function<void()> &body) {
    const auto result = run_child([&] {
        body();
        return 0;
    });
    if (result.status != 0) {
        throw std::runtime_error("test child failed: " + result.diagnostic);
    }
}

void command_outcomes() {
    const auto output = sash::capture_stdout([] {
        const auto failed = sash::try_write("not written", -1);
        require(!failed.success() && failed.status == 2, "caught write must retain failure");
        bool checked = false;
        try {
            sash::check(failed);
        } catch (const sash::CommandError &error) {
            checked = error.status() == 2;
        }
        require(checked, "checked write must throw");
        (void)sash::try_write("discarded", -1);
        sash::write("continued");
    });
    require(output == "continued", "discarded command must not stop later work");
    require(!sash::try_write("", -1).success(), "empty write must not hide an invalid fd");
}

void program_boundary() {
    const auto handled = run_child([] {
        if (sash::try_write("caught", -1).success()) {
            return 1;
        }
        (void)sash::try_write("discarded", -1);
        return 0;
    });
    require(handled.status == 0 && handled.diagnostic.empty(), "handled failures must be silent");

    const auto aborted = run_child([] {
        return sash::run("./demo", [] {
            sash::write("fail", -1);
            std::_Exit(99);
        });
    });
    const auto reason = std::error_code(EBADF, std::generic_category()).message();
    require(aborted.status == 2, "uncaught command error must exit nonzero");
    require(aborted.diagnostic == "./demo: write: " + reason + "\n",
            "uncaught error must be reported once with the invoked program name");
}

void binary_and_large_capture() {
    std::string expected(256 * 1024, 'x');
    expected += std::string("\0end\n\n", 6);
    const auto output = sash::capture_stdout([&] { sash::write(expected); });
    require(output == expected, "capture must preserve large binary output and newlines");
    require(sash::capture_stdout([] {}).empty(), "empty capture");
}

void echo_empty_words() {
    const auto output = sash::capture_stdout([] {
        sash::echo({std::string_view{}, "x", "", ""});
        sash::echo({});
    });
    require(output == " x  \n\n", "echo must retain empty-word separators");
}

void nested_capture_and_unwind() {
    std::string target = "unchanged";
    const auto outer = sash::capture_stdout([&] {
        sash::write("before:");
        require(sash::capture_stdout([] { sash::echo({"inner"}); }) == "inner\n", "inner capture");
        try {
            target = sash::capture_stdout([] {
                sash::write("partial");
                sash::write("fail", -1);
            });
        } catch (const sash::CommandError &) {
            sash::write("caught:");
        }
        sash::write("after");
    });
    require(outer == "before:caught:after", "unwinding must restore the enclosing capture");
    require(target == "unchanged", "failed capture must not assign partial output");
}

void restored_stdout() {
    in_child([] {
        struct stat before{};
        require(::fstat(1, &before) == 0, "inspect original stdout");
        const int flags = ::fcntl(1, F_GETFD);
        require(flags >= 0 && ::fcntl(1, F_SETFD, flags | FD_CLOEXEC) == 0, "mark stdout cloexec");
        require(sash::capture_stdout([] { sash::write("ok"); }) == "ok", "capture stdout");
        struct stat after{};
        require(::fstat(1, &after) == 0, "inspect restored stdout");
        require(before.st_dev == after.st_dev && before.st_ino == after.st_ino,
                "restore original stdout destination");
        require(::fcntl(1, F_GETFD) == (flags | FD_CLOEXEC), "restore stdout descriptor flags");
    });
    in_child([] {
        require(::close(1) == 0, "close original stdout");
        require(sash::capture_stdout([] { sash::write("closed"); }) == "closed",
                "closed stdout capture");
        errno = 0;
        require(::fcntl(1, F_GETFD) < 0 && errno == EBADF, "restore originally closed stdout");
    });
}

void allocation_failure_and_fd_lifetime() {
    in_child([] {
        struct rlimit original{};
        require(::getrlimit(RLIMIT_NOFILE, &original) == 0, "get fd limit");
        auto none = original;
        none.rlim_cur = 0;
        require(::setrlimit(RLIMIT_NOFILE, &none) == 0, "exhaust descriptor allocation");
        bool called = false;
        bool failed = false;
        try {
            (void)sash::capture_stdout([&] { called = true; });
        } catch (const sash::SetupError &error) {
            failed = error.code().value() == EMFILE &&
                     std::string(error.what()).find("memfd_create") != std::string::npos;
        }
        require(::setrlimit(RLIMIT_NOFILE, &original) == 0, "restore fd limit");
        require(failed && !called, "memfd failure must not execute the body");

        auto limited = original;
        limited.rlim_cur = 32;
        require(::setrlimit(RLIMIT_NOFILE, &limited) == 0, "limit descriptors for leak check");
        for (int i = 0; i < 128; ++i) {
            require(sash::capture_stdout([] { sash::write("x"); }) == "x",
                    "capture leaked descriptors");
        }
    });
}

void ecmascript_regex() {
    const auto empty = sash::compile_regex(std::string_view{});
    require(std::regex_search(std::string{}, empty), "empty pattern");
    const auto repeated = sash::compile_regex(R"(^([a-z]+)-\1$)");
    require(std::regex_match(std::string("go-go"), repeated), "ECMAScript backreference");
    require(!std::regex_match(std::string("go-no"), repeated), "regex non-match");
    bool failed = false;
    try {
        (void)sash::compile_regex("[");
    } catch (const sash::CommandError &error) {
        failed = error.status() == 2;
    }
    require(failed, "invalid regex must not become a non-match");
}

void process_outcomes(const std::string &helper) {
    for (const int status : {0, 7, 127, 128 + SIGTERM}) {
        const auto result = sash::try_execute(helper, {"exit", std::to_string(status)});
        const auto *exit = std::get_if<sash::ProcessExit>(&result.outcome);
        require(exit != nullptr && exit->status == status, "retain a real child exit");
        require(result.success() == (status == 0), "only exit zero is successful");
        if (status != 0) {
            bool checked = false;
            try {
                sash::check(result);
            } catch (const sash::CommandError &error) {
                checked = error.status() == status;
            }
            require(checked, "checked child exit must retain the status");
        }
    }
    const auto signaled = sash::try_execute(helper, {"signal", std::to_string(SIGTERM)});
    const auto *signal = std::get_if<sash::ProcessSignal>(&signaled.outcome);
    require(signal != nullptr && signal->number == SIGTERM && !signaled.success(),
            "signal termination must remain distinct from an encoded normal exit");
    bool checked = false;
    try {
        sash::check(signaled);
    } catch (const sash::CommandError &error) {
        checked = error.status() == 128 + SIGTERM;
    }
    require(checked, "encode a signal only at the checked boundary");
}

void process_program_boundary(const std::string &helper) {
    const auto handled = run_child([&] {
        if (sash::try_execute(helper, {"exit", "7"}).success()) {
            return 1;
        }
        (void)sash::try_execute(helper, {"exit", "9"});
        const auto missing = sash::try_execute(helper + ".missing");
        return std::holds_alternative<sash::SetupError>(missing.outcome) ? 0 : 1;
    });
    require(handled.status == 0 && handled.diagnostic.empty(),
            "caught and discarded process failures must be silent and continue");

    const auto aborted = run_child([&] {
        return sash::run("./demo", [&] {
            sash::execute(helper, {"exit", "7"});
            std::_Exit(99);
        });
    });
    require(aborted.status == 7 &&
                aborted.diagnostic == "./demo: " + helper + ": command failed (exit 7)\n",
            "an uncaught process failure must report once and stop later work");

    const auto setup = run_child([&] {
        return sash::run("./demo", [&] {
            sash::execute(helper + ".missing");
            std::_Exit(99);
        });
    });
    const auto reason = std::error_code(ENOENT, std::generic_category()).message();
    require(setup.status == 2 &&
                setup.diagnostic == "./demo: exec " + helper + ".missing: " + reason + "\n",
            "an uncaught exec error must retain its OS reason");
}

void external_output(const std::string &helper) {
    const std::vector<std::string> words{
        "alpha beta", "", "'quoted'", "$HOME; $(echo bad)", "*", "<", ">", "\\", "\n"};
    std::vector<std::string> arguments{"args"};
    std::string expected;
    for (const auto &word : words) {
        arguments.push_back(word);
        expected += word;
        expected += '\0';
    }
    require(sash::capture_stdout([&] { sash::execute(helper, arguments); }) == expected,
            "arguments must survive without shell parsing or losing empty words");
    require(sash::capture_stdout([&] { sash::execute(helper, {"args"}); }).empty(),
            "no arguments must remain distinct from an empty argument");

    expected.assign(256 * 1024, 'x');
    expected.append("\0end\n\n", 6);
    expected += "parent";
    require(sash::capture_stdout([&] {
                sash::execute(helper, {"binary"});
                sash::write("parent");
            }) == expected,
            "external binary output must share capture position with later parent output");

    std::string target = "unchanged";
    const auto output = sash::capture_stdout([&] {
        try {
            target = sash::capture_stdout([&] {
                sash::execute(helper, {"args", "partial"});
                sash::execute(helper, {"exit", "7"});
            });
        } catch (const sash::CommandError &) {
            sash::write("caught");
        }
    });
    require(output == "caught" && target == "unchanged",
            "external failure must restore capture before reporting or assigning");

    in_child([&] {
        const std::string value = "spaces=$HOME; literal";
        require(::setenv("SASH_CPP_LAUNCH_TEST", value.c_str(), 1) == 0, "set helper environment");
        require(sash::capture_stdout(
                    [&] { sash::execute(helper, {"env", "SASH_CPP_LAUNCH_TEST"}); }) == value,
                "inherit the actual environment without interpretation");
        const auto directory = std::filesystem::absolute(helper).parent_path().string();
        require(::setenv("PATH", directory.c_str(), 1) == 0, "set helper PATH");
        const auto bare = std::filesystem::path(helper).filename().string();
        require(!std::filesystem::exists(bare), "bare helper must not be in the working directory");
        const auto result = sash::try_execute(bare, {"exit", "0"});
        const auto *error = std::get_if<sash::SetupError>(&result.outcome);
        require(error != nullptr && error->code().value() == ENOENT, "do not search PATH");
    });
}

void exec_failures(const std::string &helper) {
    const auto directory = std::filesystem::path(helper).parent_path();
    for (const auto &fixture : {std::pair{helper + ".missing", ENOENT},
                                std::pair{(directory / "not-executable").string(), EACCES},
                                std::pair{(directory / "no-shebang").string(), ENOEXEC}}) {
        const auto result = sash::try_execute(fixture.first);
        const auto *error = std::get_if<sash::SetupError>(&result.outcome);
        require(error != nullptr && error->code().value() == fixture.second,
                "exec failure must not become a child exit or a shell fallback");
        bool checked = false;
        try {
            sash::check(result);
        } catch (const sash::SetupError &failure) {
            checked = failure.code().value() == fixture.second;
        }
        require(checked, "checked setup failure must retain its type and errno");
    }
    const auto output = sash::capture_stdout([&] {
        const auto path = helper + std::string("\0ignored", 8);
        const auto bad_path = sash::try_execute(path, {"args", "must not run"});
        const auto bad_argument = sash::try_execute(helper, {"args", std::string("one\0two", 7)});
        for (const auto *result : {&bad_path, &bad_argument}) {
            const auto *error = std::get_if<sash::SetupError>(&result->outcome);
            require(error != nullptr && error->code() == std::errc::invalid_argument,
                    "reject interior NUL before starting an executable");
        }
    });
    require(output.empty(), "invalid arguments must not run a truncated command");
}

std::vector<std::string> descriptor_probe_arguments() {
    std::vector<std::string> arguments{"fds"};
    for (int fd = 3; fd < 128; ++fd) {
        const int flags = ::fcntl(fd, F_GETFD);
        if (flags >= 0 && (flags & FD_CLOEXEC) == 0) {
            arguments.push_back(std::to_string(fd));
        } else if (flags < 0) {
            require(errno == EBADF, "inspect inherited descriptors");
        }
    }
    return arguments;
}

void process_descriptors(const std::string &helper) {
    for (int fd = 0; fd < 3; ++fd) {
        in_child([&] {
            const int flags = ::fcntl(fd, F_GETFD);
            int saved = -1;
            if (flags >= 0) {
                saved = ::fcntl(fd, F_DUPFD_CLOEXEC, 3);
                require(saved >= 0 && ::close(fd) == 0, "close a standard descriptor");
            } else {
                require(errno == EBADF, "inspect standard descriptor");
            }
            const auto result = sash::try_execute(helper, {"closed", std::to_string(fd)});
            const bool closed = ::fcntl(fd, F_GETFD) < 0 && errno == EBADF;
            if (saved >= 0) {
                require(::dup2(saved, fd) == fd && ::fcntl(fd, F_SETFD, flags) == 0 &&
                            ::close(saved) == 0,
                        "restore test descriptor");
            }
            require(result.success() && closed,
                    "exec error channels must preserve closed standard descriptors");
        });
    }
    in_child([&] {
        const auto arguments = descriptor_probe_arguments();
        require(sash::capture_stdout([&] {
                    sash::execute(helper, arguments);
                    sash::write("parent");
                }) == "parent",
                "capture and exec-private descriptors must not survive exec");
    });
}

void partial_error_pipe(const std::string &helper) {
    const int available = ::fcntl(2, F_DUPFD_CLOEXEC, 3);
    require(available >= 0 && ::close(available) == 0, "locate the first free private descriptor");
    if (::close(0) < 0) {
        require(errno == EBADF, "close test stdin");
    }
    struct rlimit original{};
    require(::getrlimit(RLIMIT_NOFILE, &original) == 0, "get partial-pipe fd limit");
    auto limited = original;
    limited.rlim_cur = static_cast<rlim_t>(available + 1);
    require(::setrlimit(RLIMIT_NOFILE, &limited) == 0, "limit private descriptor relocation");
    const auto result = sash::try_execute(helper, {"exit", "0"});
    require(::setrlimit(RLIMIT_NOFILE, &original) == 0, "restore partial-pipe fd limit");
    const auto *error = std::get_if<sash::SetupError>(&result.outcome);
    require(error != nullptr && error->code().value() == EMFILE &&
                std::string(error->what()).find("save descriptor") != std::string::npos,
            "pipe creation must succeed but relocation must fail");
    require(::fcntl(0, F_GETFD) < 0 && errno == EBADF, "failed setup must leave stdin closed");
    require(::fcntl(available, F_GETFD) < 0 && errno == EBADF,
            "failed relocation must release both pipe ends");
}

void process_resources(const std::string &helper) {
    in_child([&] {
        const auto output = sash::capture_stdout([&] {
            struct rlimit original{};
            require(::getrlimit(RLIMIT_NOFILE, &original) == 0, "get process fd limit");
            auto none = original;
            none.rlim_cur = 0;
            require(::setrlimit(RLIMIT_NOFILE, &none) == 0, "exhaust process descriptors");
            const auto result = sash::try_execute(helper, {"args", "must not run"});
            require(::setrlimit(RLIMIT_NOFILE, &original) == 0, "restore process fd limit");
            const auto *error = std::get_if<sash::SetupError>(&result.outcome);
            require(error != nullptr && error->code().value() == EMFILE &&
                        std::string(error->what()).find("create exec error pipe") !=
                            std::string::npos,
                    "error-pipe allocation failure must not become an ordinary exit");
        });
        require(output.empty(), "failed setup must not execute the command");
    });
    in_child([&] { partial_error_pipe(helper); });
    in_child([&] {
        struct rlimit limit{};
        require(::getrlimit(RLIMIT_NOFILE, &limit) == 0, "get process leak-check fd limit");
        limit.rlim_cur = std::min(limit.rlim_cur, rlim_t{32});
        require(::setrlimit(RLIMIT_NOFILE, &limit) == 0, "limit process descriptors");
        for (int i = 0; i < 64; ++i) {
            require(sash::try_execute(helper, {"exit", "0"}).success(),
                    "process descriptors leaked");
        }
        int status;
        errno = 0;
        require(::waitpid(-1, &status, WNOHANG) < 0 && errno == ECHILD,
                "foreground children must already be reaped");
    });
    in_child([&] {
        require(std::signal(SIGCHLD, SIG_IGN) != SIG_ERR, "ignore SIGCHLD for wait-error fixture");
        const auto result = sash::try_execute(helper, {"exit", "0"});
        const auto *error = std::get_if<sash::SetupError>(&result.outcome);
        require(error != nullptr && error->code().value() == ECHILD,
                "unavailable child status must be a wait error, not success");
    });
}

void reader_records(const std::string &helper) {
    const std::vector<std::string> expected{"alpha", "", std::string("nul\0inside\r", 11), "last"};
    std::vector<std::string> records;
    sash::read_from(helper, {"records", "0"}, [&](sash::LineReader &input) {
        std::string line = "old";
        while (input.readline(line)) {
            records.push_back(line);
        }
        require(line.empty(), "EOF must clear the line target");
        line = "old again";
        require(!input.readline(line) && line.empty(), "repeated EOF must clear the target");
    });
    require(records == expected, "readline must preserve binary records and an unterminated tail");
    sash::read_from(helper, {"exit", "0"}, [&](sash::LineReader &input) {
        std::string line = "old";
        require(!input.readline(line) && line.empty(),
                "empty input must be EOF, not an empty line");
    });
    sash::read_from(helper, {"binary"}, [&](sash::LineReader &input) {
        std::string line;
        require(input.readline(line) &&
                    line == std::string(256 * 1024, 'x') + std::string("\0end", 4),
                "read a binary line larger than pipe capacity");
        require(input.readline(line) && line.empty(), "a terminated empty record is successful");
        require(!input.readline(line) && line.empty(), "newline must not invent a final record");
    });
}

void reader_streaming(const std::string &helper) {
    in_child([&] {
        int control[2];
        require(::pipe(control) == 0 && ::fcntl(control[1], F_SETFD, FD_CLOEXEC) == 0,
                "create handshake channel");
        sash::read_from(helper, {"handshake", std::to_string(control[0])},
                        [&](sash::LineReader &input) {
                            std::string line;
                            const bool ready = input.readline(line) && line == "ready";
                            sash::write("Y", control[1]);
                            require(::close(control[1]) == 0, "close handshake writer");
                            require(ready, "consumer must run before producer can finish");
                            require(input.readline(line) && line == "done",
                                    "producer must see consumer's handshake");
                            require(!input.readline(line), "handshake producer must reach EOF");
                        });
        require(::close(control[0]) == 0, "close handshake reader");
    });
}

void reader_outcomes(const std::string &helper) {
    for (const int status : {7, 127}) {
        int consumed = 0;
        const auto result = sash::try_read_from(helper, {"records", std::to_string(status)},
                                                [&](sash::LineReader &input) {
                                                    std::string line;
                                                    while (input.readline(line)) {
                                                        ++consumed;
                                                    }
                                                });
        const auto *exit = std::get_if<sash::ProcessExit>(&result.outcome);
        require(exit != nullptr && exit->status == status && consumed == 4,
                "consume streaming effects before evaluating a producer's nonzero exit");
        bool checked = false;
        try {
            sash::check(result);
        } catch (const sash::CommandError &error) {
            checked = error.status() == status;
        }
        require(checked, "checking a producer must retain its exit status");
    }
    bool called = false;
    const auto missing =
        sash::try_read_from(helper + ".missing", {}, [&](sash::LineReader &) { called = true; });
    const auto *error = std::get_if<sash::SetupError>(&missing.outcome);
    require(error != nullptr && error->code().value() == ENOENT && !called,
            "failed exec must not invoke a consumer or become exit 127");
    const auto nul = sash::try_read_from(helper, {"args", std::string("one\0two", 7)},
                                         [&](sash::LineReader &) { called = true; });
    error = std::get_if<sash::SetupError>(&nul.outcome);
    require(error != nullptr && error->code() == std::errc::invalid_argument && !called,
            "a reader command must share argv NUL checks before running");
    in_child([&] {
        default_signal(SIGPIPE);
        const auto early = sash::try_read_from(helper, {"flood"}, [](sash::LineReader &) {});
        const auto *signal = std::get_if<sash::ProcessSignal>(&early.outcome);
        require(signal != nullptr && signal->number == SIGPIPE,
                "close an early reader before waiting and retain the producer's SIGPIPE");
        bool preserved = false;
        try {
            (void)sash::try_read_from(helper, {"flood"}, [](sash::LineReader &) {
                throw sash::SetupError(std::make_error_code(std::errc::permission_denied),
                                       "consumer marker");
            });
        } catch (const sash::SetupError &failure) {
            preserved = failure.code() == std::errc::permission_denied &&
                        std::string(failure.what()).find("consumer marker") != std::string::npos;
        }
        require(preserved, "consumer failure must propagate, not become the producer's result");
        int status;
        require(::waitpid(-1, &status, WNOHANG) < 0 && errno == ECHILD,
                "early and unwound producers must both be reaped");
    });
}

void install_stdin(std::string_view bytes) {
    int input[2];
    require(::pipe(input) == 0, "create original stdin");
    sash::write(bytes, input[1]);
    require(::close(input[1]) == 0, "close original stdin writer");
    if (input[0] != 0) {
        require(::dup2(input[0], 0) == 0 && ::close(input[0]) == 0, "install original stdin");
    }
}

void reader_scopes(const std::string &helper) {
    in_child([&] {
        install_stdin("program stdin");
        const auto descriptor_probe = descriptor_probe_arguments();
        std::vector<std::string> outer_records;
        const auto output = sash::capture_stdout([&] {
            sash::read_from(helper, {"records", "0"}, [&](sash::LineReader &outer) {
                std::string line;
                while (outer.readline(line)) {
                    outer_records.push_back(line);
                    int inner_records = 0;
                    sash::read_from(helper, {"records", "0"}, [&](sash::LineReader &inner) {
                        sash::execute(helper, descriptor_probe);
                        std::string nested;
                        while (inner.readline(nested)) {
                            ++inner_records;
                        }
                    });
                    require(inner_records == 4, "nested reader must have its own stream");
                    require(sash::capture_stdout([&] {
                                sash::execute(helper, {"args", "inner"});
                            }) == std::string("inner\0", 6),
                            "capture inside reader must use its own stdout and child");
                    if (outer_records.size() == 1) {
                        require(sash::capture_stdout([&] { sash::execute(helper, {"stdin"}); }) ==
                                    "program stdin",
                                "a command in the reader body must inherit program stdin");
                    }
                    sash::write("body\n");
                }
            });
            sash::write("after\n");
        });
        require(outer_records.size() == 4 && outer_records.back() == "last",
                "inner readers and captures must not steal outer records");
        require(output == "body\nbody\nbody\nbody\nafter\n",
                "reader commands must not rebind the parent's captured stdout");
    });
}

void reader_resources(const std::string &helper) {
    for (int fd = 0; fd < 3; ++fd) {
        in_child([&] {
            if (::close(fd) < 0) {
                require(errno == EBADF, "close standard descriptor for reader");
            }
            int count = 0;
            sash::read_from(helper, {"records", "0"}, [&](sash::LineReader &input) {
                std::string line;
                while (input.readline(line)) {
                    ++count;
                }
            });
            require(count == 4 && ::fcntl(fd, F_GETFD) < 0 && errno == EBADF,
                    "reader plumbing must leave closed standard descriptors closed");
            count = 0;
            sash::read_pipeline({{helper, {"records", "0"}}, {helper, {"relay"}}},
                                [&](sash::LineReader &input) {
                                    std::string line;
                                    while (input.readline(line)) {
                                        ++count;
                                    }
                                });
            require(count == 4 && ::fcntl(fd, F_GETFD) < 0 && errno == EBADF,
                    "pipeline wiring must preserve closed parent standard descriptors");
        });
    }
    in_child([&] {
        struct rlimit original{};
        require(::getrlimit(RLIMIT_NOFILE, &original) == 0, "get reader fd limit");
        auto none = original;
        none.rlim_cur = 0;
        require(::setrlimit(RLIMIT_NOFILE, &none) == 0, "exhaust reader descriptors");
        bool called = false;
        const auto result = sash::try_read_from(helper, {"records", "0"},
                                                [&](sash::LineReader &) { called = true; });
        require(::setrlimit(RLIMIT_NOFILE, &original) == 0, "restore reader fd limit");
        const auto *error = std::get_if<sash::SetupError>(&result.outcome);
        require(error != nullptr && error->code().value() == EMFILE && !called &&
                    std::string(error->what()).find("create command output pipe") !=
                        std::string::npos,
                "failed output-pipe construction must not invoke a consumer");
        auto limited = original;
        limited.rlim_cur = std::min(limited.rlim_cur, rlim_t{32});
        require(::setrlimit(RLIMIT_NOFILE, &limited) == 0, "limit reader leak-check descriptors");
        for (int i = 0; i < 32; ++i) {
            sash::read_from(helper, {"exit", "0"}, [](sash::LineReader &) {});
        }
        int status;
        require(::waitpid(-1, &status, WNOHANG) < 0 && errno == ECHILD,
                "reader completion must reap every producer without leaking descriptors");
    });
}

void pipeline_streaming(const std::string &helper) {
    sash::read_pipeline({{helper, {"binary"}}, {helper, {"relay"}}, {helper, {"relay"}}},
                        [&](sash::LineReader &input) {
                            std::string line;
                            require(input.readline(line) && line == std::string(256 * 1024, 'x') +
                                                                        std::string("\0end", 4),
                                    "stream a large binary line through multiple stages");
                            require(input.readline(line) && line.empty(), "retain the empty line");
                            require(!input.readline(line), "pipeline must deliver EOF");
                        });
    in_child([&] {
        int control[2];
        require(::pipe(control) == 0 && ::fcntl(control[1], F_SETFD, FD_CLOEXEC) == 0,
                "create pipeline handshake");
        sash::read_pipeline(
            {{helper, {"handshake", std::to_string(control[0])}},
             {helper, {"relay"}},
             {helper, {"relay"}}},
            [&](sash::LineReader &input) {
                std::string line;
                const bool ready = input.readline(line) && line == "ready";
                sash::write("Y", control[1]);
                require(::close(control[1]) == 0 && ready, "all stages must run before waiting");
                require(input.readline(line) && line == "done", "relay the consumer handshake");
                require(!input.readline(line), "handshake must reach pipeline EOF");
            });
        require(::close(control[0]) == 0, "close pipeline handshake reader");
    });
}

void pipeline_outcomes(const std::string &helper) {
    for (const auto statuses : {std::array{7, 0, 0}, std::array{7, 3, 2}, std::array{0, 4, 0}}) {
        int consumed = 0;
        const auto result =
            sash::try_read_pipeline({{helper, {"records", std::to_string(statuses[0])}},
                                     {helper, {"relay", std::to_string(statuses[1])}},
                                     {helper, {"relay", std::to_string(statuses[2])}}},
                                    [&](sash::LineReader &input) {
                                        std::string line;
                                        while (input.readline(line)) {
                                            ++consumed;
                                        }
                                    });
        require(!result.success() && result.setup_errors.empty() && consumed == 4,
                "upstream failures count after consumer effects complete");
        int rightmost = 0;
        for (std::size_t i = 0; i < statuses.size(); ++i) {
            require(result.stages[i] &&
                        std::get<sash::ProcessExit>(result.stages[i]->outcome).status ==
                            statuses[i],
                    "retain every stage's actual exit");
            if (statuses[i] != 0) {
                rightmost = statuses[i];
            }
        }
        bool checked = false;
        try {
            sash::check(result);
        } catch (const sash::CommandError &error) {
            checked = error.status() == rightmost;
        }
        require(checked, "checked pipeline must select the rightmost failure, not the maximum");
    }
    const auto signaled = sash::try_read_pipeline({{helper, {"exit", "0"}},
                                                   {helper, {"signal", std::to_string(SIGTERM)}},
                                                   {helper, {"relay"}}},
                                                  [](sash::LineReader &) {});
    require(signaled.stages[1] &&
                std::get<sash::ProcessSignal>(signaled.stages[1]->outcome).number == SIGTERM,
            "retain a pipeline stage's signal identity");
    bool checked = false;
    try {
        sash::check(signaled);
    } catch (const sash::CommandError &error) {
        checked = error.status() == 128 + SIGTERM;
    }
    require(checked, "encode pipeline signals only at the checked boundary");
    in_child([&] {
        require(std::signal(SIGCHLD, SIG_IGN) != SIG_ERR, "ignore pipeline child statuses");
        const auto result = sash::try_read_pipeline(
            {{helper, {"exit", "0"}}, {helper, {"relay"}}, {helper, {"relay"}}},
            [](sash::LineReader &input) {
                std::string line;
                require(!input.readline(line), "consume the empty pipeline before waiting");
            });
        require(result.setup_errors.size() == 3 && !result.success(),
                "retain each unavailable child status, not an invented success");
        for (const auto &stage : result.stages) {
            require(stage && std::get<sash::SetupError>(stage->outcome).code().value() == ECHILD,
                    "an unobservable started stage must retain its wait error");
        }
    });
}

void pipeline_construction(const std::string &helper) {
    bool called = false;
    bool empty = false;
    try {
        (void)sash::try_read_pipeline({}, [&](sash::LineReader &) { called = true; });
    } catch (const sash::SetupError &error) {
        empty = error.code() == std::errc::invalid_argument;
    }
    require(empty && !called, "an empty pipeline is invalid, not a successful empty stream");
    const auto nul = sash::try_read_pipeline(
        {{helper, {"records", "0"}}, {helper, {"args", std::string("bad\0arg", 7)}}},
        [&](sash::LineReader &) { called = true; });
    require(nul.setup_errors.size() == 1 && nul.setup_errors[0].stage == 1 && !nul.stages[0] &&
                !nul.stages[1] && !called &&
                nul.setup_errors[0].error.code() == std::errc::invalid_argument,
            "prepare every argv before launching any stage");
    in_child([&] {
        default_signal(SIGPIPE);
        std::string target = "unchanged";
        const auto missing =
            sash::try_read_pipeline({{helper, {"flood"}},
                                     {helper, {"relay"}},
                                     {helper + ".missing", {}},
                                     {helper, {"relay"}}},
                                    [&](sash::LineReader &input) { (void)input.readline(target); });
        require(missing.setup_errors.size() == 1 && missing.setup_errors[0].stage == 2 &&
                    missing.stages[0] && missing.stages[1] && missing.stages[2] &&
                    !missing.stages[3] && target == "unchanged",
                "late failed exec must skip the consumer and leave later stages unstarted");
        bool checked = false;
        try {
            sash::check(missing);
        } catch (const sash::SetupError &error) {
            checked = error.code().value() == ENOENT;
        }
        require(checked, "the original setup failure wins over cleanup SIGPIPE");
        int status;
        require(::waitpid(-1, &status, WNOHANG) < 0 && errno == ECHILD,
                "partial pipeline construction must reap every started child");
    });
    in_child([&] {
        const int available = ::fcntl(2, F_DUPFD_CLOEXEC, 3);
        require(available >= 0 && ::close(available) == 0, "locate free pipeline descriptors");
        struct rlimit original{};
        require(::getrlimit(RLIMIT_NOFILE, &original) == 0, "get pipeline fd limit");
        auto limited = original;
        limited.rlim_cur = static_cast<rlim_t>(available + 2);
        require(::setrlimit(RLIMIT_NOFILE, &limited) == 0, "limit pipeline pipe construction");
        const auto result = sash::try_read_pipeline(
            {{helper, {"exit", "0"}}, {helper, {"relay"}}, {helper, {"relay"}}},
            [&](sash::LineReader &) { called = true; });
        require(::setrlimit(RLIMIT_NOFILE, &original) == 0, "restore pipeline fd limit");
        require(result.setup_errors.size() == 1 && result.setup_errors[0].stage == 1 &&
                    !result.stages[0] && !result.stages[1] && !called &&
                    result.setup_errors[0].error.code().value() == EMFILE,
                "second pipe allocation failure must not run a consumer or invent stage exits");
        require(::fcntl(available, F_GETFD) < 0 && errno == EBADF &&
                    ::fcntl(available + 1, F_GETFD) < 0 && errno == EBADF,
                "release the first pipe after partial allocation");
    });
}

void pipeline_scopes(const std::string &helper) {
    in_child([&] {
        install_stdin("program stdin");
        const auto probe = descriptor_probe_arguments();
        int count = 0;
        const auto output = sash::capture_stdout([&] {
            sash::read_pipeline(
                {{helper, {"records", "0"}}, {helper, {"relay"}}}, [&](sash::LineReader &outer) {
                    std::string line;
                    while (outer.readline(line)) {
                        ++count;
                        int nested = 0;
                        sash::read_pipeline({{helper, {"records", "0"}}, {helper, {"relay"}}},
                                            [&](sash::LineReader &inner) {
                                                sash::execute(helper, probe);
                                                std::string value;
                                                while (inner.readline(value)) {
                                                    ++nested;
                                                }
                                            });
                        require(nested == 4, "nested pipeline keeps its own reader");
                        if (count == 1) {
                            require(sash::capture_stdout([&] {
                                        sash::execute(helper, {"stdin"});
                                    }) == "program stdin",
                                    "pipeline body commands retain parent stdin");
                        }
                        sash::write("body\n");
                    }
                });
            sash::write("after\n");
        });
        require(count == 4 && output == "body\nbody\nbody\nbody\nafter\n",
                "nested pipelines and captures preserve outer stream and parent output");
        default_signal(SIGPIPE);
        bool preserved = false;
        try {
            (void)sash::try_read_pipeline(
                {{helper, {"flood"}}, {helper, {"relay"}}, {helper, {"relay"}}},
                [](sash::LineReader &) { throw sash::CommandError(23, "consumer marker"); });
        } catch (const sash::CommandError &error) {
            preserved = error.status() == 23;
        }
        require(preserved, "consumer exception survives close-before-wait of every stage");
        struct rlimit limit{};
        require(::getrlimit(RLIMIT_NOFILE, &limit) == 0, "get pipeline leak-check limit");
        limit.rlim_cur = std::min(limit.rlim_cur, rlim_t{32});
        require(::setrlimit(RLIMIT_NOFILE, &limit) == 0, "limit pipeline leak-check descriptors");
        for (int i = 0; i < 16; ++i) {
            sash::read_pipeline({{helper, {"exit", "0"}}, {helper, {"relay"}}},
                                [](sash::LineReader &) {});
        }
        int status;
        require(::waitpid(-1, &status, WNOHANG) < 0 && errno == ECHILD,
                "completed and unwound pipeline children must all be reaped");
    });
}

void pipeline_fork_failure(const std::string &helper) {
    default_signal(SIGPIPE);
    bool called = false;
    const auto result =
        sash::try_read_pipeline({{helper, {"flood"}}, {helper, {"relay"}}, {helper, {"relay"}}},
                                [&](sash::LineReader &) { called = true; });
    require(result.setup_errors.size() == 1 && result.setup_errors[0].stage == 1 &&
                result.stages[0] && !result.stages[1] && !result.stages[2] && !called &&
                result.setup_errors[0].error.code().value() == EAGAIN,
            "the injected second fork must close pipes and retain the started producer");
    int status;
    require(::waitpid(-1, &status, WNOHANG) < 0 && errno == ECHILD,
            "failed later fork must not orphan the first producer");
}

void pipeline_wait_failure(const std::string &helper) {
    const auto result = sash::try_read_pipeline(
        {{helper, {"exit", "0"}}, {helper, {"relay"}}, {helper, {"relay"}}},
        [](sash::LineReader &input) {
            std::string line;
            require(!input.readline(line), "consume the empty pipeline before the wait fault");
        });
    require(result.setup_errors.size() == 1 && result.setup_errors[0].stage == 0 &&
                result.setup_errors[0].error.code().value() == EIO && !result.success(),
            "retain the first wait error separately from recovered stage outcomes");
    for (const auto &stage : result.stages) {
        require(stage && std::holds_alternative<sash::ProcessExit>(stage->outcome) &&
                    std::get<sash::ProcessExit>(stage->outcome).status == 0,
                "cleanup reaping must preserve every actual child exit");
    }
    bool checked = false;
    try {
        sash::check(result);
    } catch (const sash::SetupError &error) {
        checked = error.code().value() == EIO;
    }
    int status;
    require(checked && ::waitpid(-1, &status, WNOHANG) < 0 && errno == ECHILD,
            "the original wait failure remains checked after all children are accounted for");
}

void pipeline_capture_bytes(const std::string &helper) {
    const char records[] = "alpha\n\nnul\0inside\r\nlast";
    const auto captured = sash::try_capture_pipeline({{helper, {"records", "0"}}});
    require(captured.pipeline.success() &&
                captured.output == std::string(records, sizeof(records) - 1),
            "pipeline capture must retain separators, NUL, CR, and an unterminated tail");
    require(sash::capture_pipeline({{helper, {"exit", "0"}}}).empty(),
            "a successful empty producer yields an empty capture");
    require(sash::capture_pipeline({{helper, {"stdout-pipe"}}}) == "pipe",
            "external-only capture must give its terminal command a pipe, not a memfd");

    std::string expected(256 * 1024, 'x');
    expected.append("\0end\n\n", 6);
    for (const int count : {1, 3}) {
        std::vector<sash::Command> commands{{helper, {"binary"}}};
        for (int i = 1; i < count; ++i) {
            commands.push_back({helper, {"relay"}});
        }
        require(sash::capture_pipeline(commands) == expected,
                "drain large binary captures before waiting, without trimming final newlines");
    }
}

void pipeline_capture_outcomes(const std::string &helper) {
    const char records[] = "alpha\n\nnul\0inside\r\nlast";
    const auto failed =
        sash::try_capture_pipeline({{helper, {"records", "7"}}, {helper, {"relay", "3"}}});
    require(failed.output == std::string(records, sizeof(records) - 1) &&
                !failed.pipeline.success() && failed.pipeline.setup_errors.empty() &&
                failed.pipeline.stages.size() == 2 && failed.pipeline.stages[0] &&
                failed.pipeline.stages[1] &&
                std::get<sash::ProcessExit>(failed.pipeline.stages[0]->outcome).status == 7 &&
                std::get<sash::ProcessExit>(failed.pipeline.stages[1]->outcome).status == 3,
            "capture bytes and every failed producer outcome must survive together");
    std::string target = "unchanged";
    bool checked = false;
    try {
        target = sash::capture_pipeline({{helper, {"records", "7"}}, {helper, {"relay", "3"}}});
    } catch (const sash::CommandError &error) {
        checked = error.status() == 3;
    }
    require(checked && target == "unchanged",
            "checking the rightmost failure must precede capture assignment");

    bool empty = false;
    try {
        (void)sash::try_capture_pipeline({});
    } catch (const sash::SetupError &error) {
        empty = error.code() == std::errc::invalid_argument;
    }
    require(empty, "an empty command list is invalid, not a successful empty capture");
    const auto nul = sash::try_capture_pipeline(
        {{helper, {"records", "0"}}, {helper, {"args", std::string("bad\0arg", 7)}}});
    require(nul.output.empty() && nul.pipeline.setup_errors.size() == 1 &&
                nul.pipeline.setup_errors[0].stage == 1 &&
                nul.pipeline.setup_errors[0].error.code() == std::errc::invalid_argument &&
                !nul.pipeline.stages[0] && !nul.pipeline.stages[1],
            "invalid capture argv must leave all stages unstarted and output empty");
    in_child([&] {
        default_signal(SIGPIPE);
        const auto missing = sash::try_capture_pipeline(
            {{helper, {"flood"}}, {helper + ".missing", {}}, {helper, {"relay"}}});
        require(
            missing.output.empty() && missing.pipeline.setup_errors.size() == 1 &&
                missing.pipeline.setup_errors[0].stage == 1 && missing.pipeline.stages[0] &&
                missing.pipeline.stages[1] && !missing.pipeline.stages[2] &&
                std::get<sash::ProcessSignal>(missing.pipeline.stages[0]->outcome).number ==
                    SIGPIPE &&
                std::get<sash::SetupError>(missing.pipeline.stages[1]->outcome).code().value() ==
                    ENOENT,
            "late capture setup failure must close pipes and retain started stages");
        int status;
        require(::waitpid(-1, &status, WNOHANG) < 0 && errno == ECHILD,
                "partial capture setup must reap every started child");
    });
    in_child([&] {
        require(std::signal(SIGCHLD, SIG_IGN) != SIG_ERR, "ignore captured child status");
        const auto unobserved = sash::try_capture_pipeline({{helper, {"records", "0"}}});
        require(unobserved.output == std::string(records, sizeof(records) - 1) &&
                    unobserved.pipeline.setup_errors.size() == 1 &&
                    unobserved.pipeline.setup_errors[0].error.code().value() == ECHILD &&
                    !unobserved.pipeline.success(),
                "an unavailable child status must not erase already captured bytes");
    });
}

void pipeline_capture_scopes(const std::string &helper) {
    in_child([&] {
        install_stdin("program stdin");
        const auto probe = descriptor_probe_arguments();
        struct stat stdin_before{};
        require(::fstat(0, &stdin_before) == 0, "inspect original capture stdin");
        int count = 0;
        const auto output = sash::capture_stdout([&] {
            sash::write("before\n");
            sash::read_pipeline(
                {{helper, {"records", "0"}}, {helper, {"relay"}}}, [&](sash::LineReader &outer) {
                    std::string line;
                    while (outer.readline(line)) {
                        ++count;
                        require(
                            sash::capture_pipeline({{helper, probe}, {helper, {"relay"}}}).empty(),
                            "nested capture-private fds must not survive exec");
                        const auto label = "nested " + std::to_string(count);
                        require(sash::capture_pipeline({{helper, {"args", label}},
                                                        {helper, {"relay"}}}) == label + '\0',
                                "nested raw capture must not steal outer records");
                        if (count == 1) {
                            require(sash::capture_pipeline(
                                        {{helper, {"stdin"}}, {helper, {"relay"}}}) ==
                                        "program stdin",
                                    "captured commands inherit program stdin, not the reader");
                        }
                        sash::write("body\n");
                    }
                });
            std::string target = "unchanged";
            try {
                target = sash::capture_pipeline({{helper, {"records", "7"}}, {helper, {"relay"}}});
            } catch (const sash::CommandError &error) {
                require(error.status() == 7, "retain nested capture failure");
                sash::write("caught\n");
            }
            require(target == "unchanged", "failed pipe capture leaves its assignment untouched");
            sash::write("after\n");
        });
        require(count == 4 && output == "before\nbody\nbody\nbody\nbody\ncaught\nafter\n",
                "raw capture must leave the enclosing memfd destination and reader intact");
        struct stat stdin_after{};
        require(::fstat(0, &stdin_after) == 0 && stdin_before.st_dev == stdin_after.st_dev &&
                    stdin_before.st_ino == stdin_after.st_ino,
                "raw capture must not rebind parent stdin");
    });
}

void pipeline_capture_resources(const std::string &helper) {
    in_child([&] {
        for (int fd = 0; fd < 3; ++fd) {
            if (::close(fd) < 0) {
                require(errno == EBADF, "close standard descriptor for raw capture");
            }
        }
        require(sash::capture_pipeline({{helper, {"args", "closed"}}, {helper, {"relay"}}}) ==
                    std::string("closed\0", 7),
                "raw capture must work with every parent standard descriptor closed");
        for (int fd = 0; fd < 3; ++fd) {
            require(::fcntl(fd, F_GETFD) < 0 && errno == EBADF,
                    "raw capture plumbing must preserve closed parent descriptors");
        }
    });
    in_child([&] {
        default_signal(SIGPIPE);
        struct rlimit original{};
        require(::getrlimit(RLIMIT_NOFILE, &original) == 0, "get raw capture fd limit");
        auto none = original;
        none.rlim_cur = 0;
        require(::setrlimit(RLIMIT_NOFILE, &none) == 0, "exhaust raw capture descriptors");
        const auto failed = sash::try_capture_pipeline({{helper, {"records", "0"}}});
        require(::setrlimit(RLIMIT_NOFILE, &original) == 0, "restore raw capture fd limit");
        require(failed.output.empty() && failed.pipeline.setup_errors.size() == 1 &&
                    failed.pipeline.setup_errors[0].error.code().value() == EMFILE &&
                    !failed.pipeline.stages[0],
                "pipe allocation failure returns setup failure, not empty successful output");

        std::array<int, 128> descriptors{};
        for (int fd = 0; fd < static_cast<int>(descriptors.size()); ++fd) {
            descriptors[fd] = ::fcntl(fd, F_GETFD);
            require(descriptors[fd] >= 0 || errno == EBADF, "snapshot raw capture descriptors");
        }
        auto limited = original;
        limited.rlim_cur = std::min(limited.rlim_cur, rlim_t{32});
        require(::setrlimit(RLIMIT_NOFILE, &limited) == 0, "limit raw capture leak-check fds");
        for (int i = 0; i < 16; ++i) {
            const auto captured =
                sash::try_capture_pipeline({{helper, {"args", "bytes"}}, {helper, {"relay", "7"}}});
            require(captured.output == std::string("bytes\0", 6) && !captured.pipeline.success(),
                    "failed producer captures must not leak descriptors or bytes");
        }
        std::string target = "unchanged";
        bool allocation_failed = false;
        {
            const AllocationFailure fault(64 * 1024);
            try {
                target = sash::capture_pipeline({{helper, {"binary"}}, {helper, {"relay"}}});
            } catch (const std::bad_alloc &) {
                allocation_failed = true;
            }
        }
        require(::setrlimit(RLIMIT_NOFILE, &original) == 0, "restore raw capture leak-check limit");
        require(allocation_failed && target == "unchanged",
                "collector allocation failure must propagate before assignment");
        for (int fd = 0; fd < static_cast<int>(descriptors.size()); ++fd) {
            require(::fcntl(fd, F_GETFD) == descriptors[fd],
                    "completed and unwound captures must release every private descriptor");
        }
        int status;
        require(::waitpid(-1, &status, WNOHANG) < 0 && errno == ECHILD,
                "allocation failure and ordinary capture completion must reap every child");
    });
}

void pipeline_capture_read_failure(const std::string &helper) {
    default_signal(SIGPIPE);
    std::string target = "unchanged";
    bool failed = false;
    try {
        target = sash::capture_pipeline({{helper, {"binary"}}});
    } catch (const sash::SetupError &error) {
        failed = error.code().value() == EIO &&
                 std::string(error.what()).find("read command output") != std::string::npos;
    }
    int status;
    require(failed && target == "unchanged" && ::waitpid(-1, &status, WNOHANG) < 0 &&
                errno == ECHILD,
            "a consumer read error must throw, preserve the target, and close before reaping");
}

void pipeline_capture_read_interrupted(const std::string &helper) {
    std::string expected(256 * 1024, 'x');
    expected.append("\0end\n\n", 6);
    require(sash::capture_pipeline({{helper, {"binary"}}}) == expected,
            "retry interrupted capture reads without losing or duplicating bytes");
}

void pipeline_capture_wait_failure(const std::string &helper) {
    std::string expected(256 * 1024, 'x');
    expected.append("\0end\n\n", 6);
    const auto captured = sash::try_capture_pipeline({{helper, {"binary"}}});
    require(captured.output == expected && captured.pipeline.setup_errors.size() == 1 &&
                captured.pipeline.setup_errors[0].error.code().value() == EIO &&
                captured.pipeline.stages[0] &&
                std::get<sash::ProcessExit>(captured.pipeline.stages[0]->outcome).status == 0,
            "a recovered wait error retains captured bytes and the actual child exit");
    bool checked = false;
    try {
        sash::check(captured.pipeline);
    } catch (const sash::SetupError &error) {
        checked = error.code().value() == EIO;
    }
    int status;
    require(checked && ::waitpid(-1, &status, WNOHANG) < 0 && errno == ECHILD,
            "capture completion must account for the child without hiding the first wait error");
}

void fork_failure(const std::string &helper) {
    const auto output = sash::capture_stdout([&] {
        const auto result = sash::try_execute(helper, {"args", "must not run"});
        const auto *error = std::get_if<sash::SetupError>(&result.outcome);
        require(error != nullptr && error->code().value() == EAGAIN &&
                    std::string(error->what()).find("fork ") != std::string::npos,
                "expected the injected fork failure");
    });
    require(output.empty(), "failed fork must not execute the command");
}

void interrupted_wait(const std::string &helper) {
    const auto result = sash::try_execute(helper, {"exit", "7"});
    const auto *exit = std::get_if<sash::ProcessExit>(&result.outcome);
    require(exit != nullptr && exit->status == 7, "retry interrupted wait and retain child status");
}

bool run_test(const char *name, const std::function<void()> &body) {
    try {
        body();
    } catch (const std::exception &error) {
        std::fprintf(stderr, "FAIL %s: %s\n", name, error.what());
        return false;
    }
    std::fprintf(stderr, "ok %s\n", name);
    return true;
}

} // namespace

int main(int argc, char **argv) {
    if (argc == 3) {
        const std::string_view mode(argv[1]);
        const std::string helper(argv[2]);
        if (std::getenv("SASH_CPP_TEST_TRACE") != nullptr) {
            sash::write("__SASH_CPP_TEST_BEGIN__\n", 2);
        }
        if (mode == "--fork-failure") {
            return run_test(argv[1], [&] { fork_failure(helper); }) ? 0 : 1;
        }
        if (mode == "--wait-interrupted") {
            return run_test(argv[1], [&] { interrupted_wait(helper); }) ? 0 : 1;
        }
        if (mode == "--pipeline-fork-failure") {
            return run_test(argv[1], [&] { pipeline_fork_failure(helper); }) ? 0 : 1;
        }
        if (mode == "--pipeline-wait-failure") {
            return run_test(argv[1], [&] { pipeline_wait_failure(helper); }) ? 0 : 1;
        }
        if (mode == "--capture-no-memfd" || mode == "--capture-bytes") {
            return run_test(argv[1], [&] { pipeline_capture_bytes(helper); }) ? 0 : 1;
        }
        if (mode == "--capture-outcomes") {
            return run_test(argv[1], [&] { pipeline_capture_outcomes(helper); }) ? 0 : 1;
        }
        if (mode == "--capture-scopes") {
            return run_test(argv[1], [&] { pipeline_capture_scopes(helper); }) ? 0 : 1;
        }
        if (mode == "--capture-resources") {
            return run_test(argv[1], [&] { pipeline_capture_resources(helper); }) ? 0 : 1;
        }
        if (mode == "--capture-read-failure") {
            return run_test(argv[1], [&] { pipeline_capture_read_failure(helper); }) ? 0 : 1;
        }
        if (mode == "--capture-read-interrupted") {
            return run_test(argv[1], [&] { pipeline_capture_read_interrupted(helper); }) ? 0 : 1;
        }
        if (mode == "--capture-wait-failure") {
            return run_test(argv[1], [&] { pipeline_capture_wait_failure(helper); }) ? 0 : 1;
        }
        if (mode == "--dup-failure") {
            return run_test(argv[1], [&] { partial_error_pipe(helper); }) ? 0 : 1;
        }
        if (mode == "--exec-failure") {
            return run_test(argv[1], [&] { exec_failures(helper); }) ? 0 : 1;
        }
    }
    if (argc != 2) {
        std::fprintf(stderr, "usage: runtime-tests COMMAND_HELPER\n");
        return 2;
    }
    const std::string helper(argv[1]);
    const std::pair<const char *, std::function<void()>> tests[] = {
        {"command outcomes", command_outcomes},
        {"program boundary", program_boundary},
        {"binary and large capture", binary_and_large_capture},
        {"echo empty words", echo_empty_words},
        {"nested capture and unwind", nested_capture_and_unwind},
        {"restored stdout", restored_stdout},
        {"allocation failure and fd lifetime", allocation_failure_and_fd_lifetime},
        {"ECMAScript regex", ecmascript_regex},
        {"process outcomes", [&] { process_outcomes(helper); }},
        {"process program boundary", [&] { process_program_boundary(helper); }},
        {"external output and arguments", [&] { external_output(helper); }},
        {"exec failures", [&] { exec_failures(helper); }},
        {"process descriptors", [&] { process_descriptors(helper); }},
        {"process resources", [&] { process_resources(helper); }},
        {"reader records", [&] { reader_records(helper); }},
        {"reader streaming handshake", [&] { reader_streaming(helper); }},
        {"reader outcomes and early completion", [&] { reader_outcomes(helper); }},
        {"nested reader and capture scopes", [&] { reader_scopes(helper); }},
        {"reader resources", [&] { reader_resources(helper); }},
        {"pipeline streaming and handshake", [&] { pipeline_streaming(helper); }},
        {"pipeline stage outcomes", [&] { pipeline_outcomes(helper); }},
        {"pipeline construction failures", [&] { pipeline_construction(helper); }},
        {"pipeline scopes and resources", [&] { pipeline_scopes(helper); }},
        {"pipeline capture bytes", [&] { pipeline_capture_bytes(helper); }},
        {"pipeline capture outcomes", [&] { pipeline_capture_outcomes(helper); }},
        {"pipeline capture scopes", [&] { pipeline_capture_scopes(helper); }},
        {"pipeline capture resources", [&] { pipeline_capture_resources(helper); }},
    };
    for (const auto &test : tests) {
        if (!run_test(test.first, test.second)) {
            return 1;
        }
    }
}
