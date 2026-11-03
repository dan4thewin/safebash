#include "sash/runtime.hpp"

#include <algorithm>
#include <cerrno>
#include <csignal>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fcntl.h>
#include <filesystem>
#include <functional>
#include <limits>
#include <stdexcept>
#include <sys/resource.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>
#include <utility>

namespace {

enum class AllocationFailureMode { AtLeast, Exact };

std::size_t allocation_failure_size = 0;
AllocationFailureMode allocation_failure_mode = AllocationFailureMode::AtLeast;

} // namespace

// Test-only allocation faults leave the small launch allocations available.
void *operator new(std::size_t size) {
    if (allocation_failure_size != 0 && (allocation_failure_mode == AllocationFailureMode::Exact
                                             ? size == allocation_failure_size
                                             : size >= allocation_failure_size)) {
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
    explicit AllocationFailure(std::size_t size,
                               AllocationFailureMode mode = AllocationFailureMode::AtLeast) {
        allocation_failure_size = size;
        allocation_failure_mode = mode;
    }
    ~AllocationFailure() {
        allocation_failure_size = 0;
        allocation_failure_mode = AllocationFailureMode::AtLeast;
    }
};

struct HandshakeWriter {
    int &fd;
    ~HandshakeWriter() {
        const int owned = std::exchange(fd, -1);
        if (::close(owned) < 0) {
            (void)std::fputs("test: cannot close handshake writer\n", stderr);
            std::_Exit(1);
        }
    }
};

void require(bool condition, const char *message) {
    if (!condition) {
        throw std::runtime_error(message);
    }
}

void default_signal(int number) {
    require(std::signal(number, SIG_DFL) != SIG_ERR, "restore default signal disposition");
    sigset_t signals;
    require((::sigemptyset)(&signals) == 0 && (::sigaddset)(&signals, number) == 0 &&
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

void capture_storage_contract(const std::string &helper) {
    require(sash::capture_stdout([] {
                struct stat storage{};
                require(::fstat(1, &storage) == 0 && S_ISREG(storage.st_mode),
                        "in-process capture must provide a regular file");
                require(storage.st_nlink == 0, "capture storage must be unlinked before the body");
#if defined(__APPLE__)
                require((storage.st_mode & 0077) == 0, "capture file must be private");
#endif
                sash::write("ok");
            }) == "ok",
            "unlinked capture storage must support ordinary fd writes");
    in_child([&] {
        const auto missing = std::filesystem::absolute(helper).string() + ".capture-temp.missing";
        require(!std::filesystem::exists(missing), "capture temp fixture must not exist");
        require(::setenv("TMPDIR", missing.c_str(), 1) == 0, "set invalid capture temp directory");
#if defined(__APPLE__)
        bool called = false;
        bool failed = false;
        try {
            (void)sash::capture_stdout([&] { called = true; });
        } catch (const sash::SetupError &error) {
            failed =
                (error.code() == std::errc::no_such_file_or_directory ||
                 error.code() == std::errc::not_a_directory) &&
                std::string(error.what()).find("locate capture directory") != std::string::npos;
        }
        require(failed && !called, "invalid capture directory must fail before the body");
#else
        require(sash::capture_stdout([] { sash::write("memfd"); }) == "memfd",
                "Linux memfd capture must not consult a temporary directory");
#endif
        require(sash::capture_pipeline({{helper, {"stdout-pipe"}}}) == "pipe",
                "external-only capture must not require a temporary directory");
    });
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
#if defined(__APPLE__)
        const char *storage_operation = "create capture file";
#else
        const char *storage_operation = "memfd_create";
#endif
        try {
            (void)sash::capture_stdout([&] { called = true; });
        } catch (const sash::SetupError &error) {
            failed = error.code().value() == EMFILE &&
                     std::string(error.what()).find(storage_operation) != std::string::npos;
        }
        require(::setrlimit(RLIMIT_NOFILE, &original) == 0, "restore fd limit");
        require(failed && !called, "capture storage failure must not execute the body");

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
            "external-only capture must give its terminal command a pipe, not a capture file");

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
                "raw capture must leave the enclosing capture destination and reader intact");
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

void lifecycle_completion() {
    int ends = 0;
    bool unwound = false;
    struct BodyLifetime {
        bool &finished;
        ~BodyLifetime() { finished = true; }
    };
    require(sash::run(
                "complete", [&] { BodyLifetime lifetime{unwound}; },
                [&](const sash::Shutdown &shutdown) {
                    ++ends;
                    require(unwound && shutdown.reason == sash::ShutdownReason::Completed &&
                                shutdown.status == 0 && shutdown.signal == 0,
                            "normal END must follow body unwinding");
                }) == 0 &&
                ends == 1,
            "normal completion runs END once");
    ends = 0;
    unwound = false;
    require(sash::run(
                "exit",
                [&] {
                    BodyLifetime lifetime{unwound};
                    sash::exit_program(7);
                },
                [&](const sash::Shutdown &shutdown) {
                    ++ends;
                    require(unwound && shutdown.reason == sash::ShutdownReason::ExplicitExit &&
                                shutdown.status == 7 && shutdown.signal == 0,
                            "explicit exit must remain distinct from command failure");
                }) == 7 &&
                ends == 1,
            "END must preserve the requested status");
    ends = 0;
    require(sash::run(
                "override", [] { sash::exit_program(7); },
                [&](const sash::Shutdown &shutdown) {
                    ++ends;
                    require(shutdown.status == 7, "END receives the original status");
                    sash::exit_program(9);
                }) == 9 &&
                ends == 1,
            "exit in END overrides without re-entering it");
    require(sash::run("without-END", [] { sash::exit_program(6); }) == 6,
            "the original entry form must support explicit exit without END");
}

void lifecycle_failures() {
    const auto failed = run_child([] {
        int ends = 0;
        const int status = sash::run(
            "failed", [] { sash::check(sash::CommandResult{7, "body failed"}); },
            [&](const sash::Shutdown &shutdown) {
                ++ends;
                require(shutdown.reason == sash::ShutdownReason::Failure && shutdown.status == 7 &&
                            shutdown.signal == 0,
                        "END receives the uncaught body failure");
                bool caught = false;
                try {
                    sash::write("not written", -1);
                } catch (const sash::CommandError &error) {
                    caught = error.status() == 2;
                }
                require(caught, "END handles its own checked failure explicitly");
                (void)sash::try_write("discarded", -1);
            });
        require(ends == 1, "body failure must run END once");
        return status;
    });
    require(failed.status == 7 && failed.diagnostic == "failed: body failed\n",
            "explicit END handling must neither replace nor rediagnose incoming failure");

    const auto allocated = run_child([] {
        int ends = 0;
        const int status = sash::run(
            "allocation",
            [] {
                AllocationFailure fault(1);
                sash::signal(SIGUSR1, sash::SignalDisposition::Flag);
            },
            [&](const sash::Shutdown &shutdown) {
                ++ends;
                require(shutdown.reason == sash::ShutdownReason::Failure && shutdown.status == 2,
                        "signal snapshot allocation uses the ordinary body boundary");
            });
        require(ends == 1, "allocation failure must still run END");
        return status;
    });
    require(allocated.status == 2 && allocated.diagnostic == "allocation: out of memory\n",
            "the final allocation diagnostic must not allocate");

    bool escaped = false;
    bool later = false;
    try {
        (void)sash::run(
            "invalid-END", [] {},
            [&](const sash::Shutdown &) {
                sash::write("unhandled", -1);
                later = true;
            });
    } catch (const sash::CommandError &error) {
        escaped = error.status() == 2;
    }
    require(escaped && !later,
            "invalid unhandled END must escape to its C++ caller, not be automatically handled");

    bool caught_body = false;
    bool caught_allocation = false;
    require(sash::run(
                "END-function", [] {},
                [&](const sash::Shutdown &) {
                    const auto f = []() -> int {
                        sash::check(sash::CommandResult{4, "function body failed"});
                        return 7;
                    };
                    try {
                        (void)f();
                    } catch (const sash::CommandError &error) {
                        caught_body = error.status() == 4;
                    }
                    try {
                        AllocationFailure fault(1);
                        std::string value(128, 'x');
                    } catch (const std::bad_alloc &) {
                        caught_allocation = true;
                    }
                }) == 0 &&
                caught_body && caught_allocation,
            "END explicitly catches allocation and unchanged outside-function body failures");

    const auto body_call = run_child([] {
        return sash::run(
            "outside-function",
            [] {
                const auto f = []() -> int {
                    sash::check(sash::CommandResult{4, "function body failed"});
                    return 7;
                };
                (void)f();
            },
            [](const sash::Shutdown &shutdown) {
                require(shutdown.reason == sash::ShutdownReason::Failure && shutdown.status == 4,
                        "discarding a function result must not discard its body failure");
            });
    });
    require(body_call.status == 4 &&
                body_call.diagnostic == "outside-function: function body failed\n",
            "ordinary function checks remain independent of result discard");
}

void lifecycle_scopes(const std::string &helper) {
    in_child([&] {
        default_signal(SIGPIPE);
        const auto probe = descriptor_probe_arguments();
        struct stat before{};
        require(::fstat(1, &before) == 0, "inspect program stdout before shutdown");
        std::string target = "unchanged";
        int ends = 0;
        const auto output = sash::capture_stdout([&] {
            const int status = sash::run(
                "unwind",
                [&] {
                    target = sash::capture_stdout([&] {
                        sash::write("private\n");
                        sash::read_pipeline(
                            {{helper, {"flood-records"}}, {helper, {"relay"}}},
                            [&](sash::LineReader &input) {
                                std::string line;
                                require(input.readline(line) && line.size() == 8191,
                                        "the producers must be active before program exit");
                                sash::exit_program(5);
                            });
                    });
                },
                [&](const sash::Shutdown &shutdown) {
                    ++ends;
                    require(shutdown.reason == sash::ShutdownReason::ExplicitExit &&
                                shutdown.status == 5 && target == "unchanged",
                            "exit unwinds capture without assigning partial output");
                    int child_status;
                    require(::waitpid(-1, &child_status, WNOHANG) < 0 && errno == ECHILD,
                            "END must see every foreground producer already reaped");
                    require(sash::try_execute(helper, probe).success(),
                            "capture and pipeline-private fds must close before END");
                    sash::write("END\n");
                });
            require(status == 5 && ends == 1, "resource exit must preserve its status once");
            sash::write("caller\n");
        });
        require(output == "END\ncaller\n", "END must write to the restored program channel");
        struct stat after{};
        require(::fstat(1, &after) == 0 && before.st_dev == after.st_dev &&
                    before.st_ino == after.st_ino,
                "shutdown must restore the enclosing program stdout");
    });
}

void lifecycle_signals() {
    in_child([] {
        default_signal(SIGUSR1);
        default_signal(SIGUSR2);
        require(sash::run(
                    "FLAG",
                    [] {
                        sash::signal(SIGUSR1, sash::SignalDisposition::Flag);
                        sash::signal(SIGUSR2, sash::SignalDisposition::Exit);
                        require(std::raise(SIGUSR1) == 0, "deliver FLAG");
                        sash::checkpoint();
                        require(sash::signum() == SIGUSR1, "FLAG must retain its signal identity");
                        sash::signal(SIGUSR1, sash::SignalDisposition::Exit);
                        sash::checkpoint();
                        sash::clear_signum();
                        require(sash::signum() == 0, "clear only the most recent signal");
                    },
                    [](const sash::Shutdown &shutdown) {
                        require(shutdown.reason == sash::ShutdownReason::Completed,
                                "a historical FLAG cannot become an EXIT request");
                    }) == 0,
                "FLAG must not trigger a different or newly armed EXIT disposition");
        require(sash::run(
                    "EXIT",
                    [] {
                        sash::signal(SIGUSR1, sash::SignalDisposition::Exit);
                        sash::signal(SIGUSR2, sash::SignalDisposition::Flag);
                        require(std::raise(SIGUSR1) == 0 && std::raise(SIGUSR2) == 0,
                                "deliver EXIT followed by FLAG");
                        sash::checkpoint();
                        throw std::runtime_error("EXIT checkpoint returned");
                    },
                    [](const sash::Shutdown &shutdown) {
                        require(shutdown.reason == sash::ShutdownReason::Signal &&
                                    shutdown.status == 128 + SIGUSR1 &&
                                    shutdown.signal == SIGUSR1 && sash::signum() == SIGUSR2,
                                "latest FLAG must not overwrite the pending EXIT identity");
                    }) == 128 + SIGUSR1,
                "encode the triggering signal only at the exit boundary");
        require(sash::run(
                    "clear-pending",
                    [] {
                        sash::signal(SIGUSR1, sash::SignalDisposition::Exit);
                        require(std::raise(SIGUSR1) == 0, "deliver clear-pending signal");
                        sash::clear_signum();
                        sash::checkpoint();
                    },
                    [](const sash::Shutdown &shutdown) {
                        require(shutdown.signal == SIGUSR1 && sash::signum() == 0,
                                "clearing signum must not suppress pending EXIT");
                    }) == 128 + SIGUSR1,
                "pending EXIT survives an explicit most-recent reset");
        int ends = 0;
        require(sash::run(
                    "END-signal", [] {},
                    [&](const sash::Shutdown &shutdown) {
                        ++ends;
                        sash::signal(SIGUSR1, sash::SignalDisposition::Exit);
                        require(std::raise(SIGUSR1) == 0, "deliver signal within END");
                        sash::checkpoint();
                        require(shutdown.status == 0 && sash::signum() == SIGUSR1,
                                "END keeps signal information without re-entry");
                    }) == 0 &&
                    ends == 1,
                "END checkpoints must not start a second shutdown");
        require(sash::run("ignored",
                          [] {
                              sash::signal(SIGUSR1, sash::SignalDisposition::Ignore);
                              require(std::raise(SIGUSR1) == 0 && sash::signum() == 0,
                                      "ignored signals must be invisible");
                              sash::signal(SIGUSR1, sash::SignalDisposition::Default);
                              struct sigaction action{};
                              require(::sigaction(SIGUSR1, nullptr, &action) == 0 &&
                                          action.sa_handler == SIG_DFL,
                                      "DFL must install the kernel default");
                          }) == 0,
                "IGN and DFL are ordinary explicit dispositions");
    });
}

void lifecycle_signal_scopes() {
    in_child([] {
        default_signal(SIGUSR1);
        struct sigaction original{};
        require(::sigaction(SIGUSR1, nullptr, &original) == 0, "read original signal disposition");
        bool escaped = false;
        try {
            (void)sash::run(
                "restore-END", [] { sash::signal(SIGUSR1, sash::SignalDisposition::Flag); },
                [](const sash::Shutdown &) { sash::write("unhandled", -1); });
        } catch (const sash::CommandError &) {
            escaped = true;
        }
        struct sigaction restored{};
        require(escaped && ::sigaction(SIGUSR1, nullptr, &restored) == 0 &&
                    restored.sa_handler == original.sa_handler &&
                    restored.sa_flags == original.sa_flags,
                "an escaping END error must restore the original disposition");
        sigset_t mask;
        require(::sigprocmask(SIG_BLOCK, nullptr, &mask) == 0 &&
                    (::sigismember)(&mask, SIGUSR1) == 0,
                "lifecycle handoffs must restore the incoming signal mask");
        require(sash::signum() == 0, "completed drivers must not leak signal state");
    });
    const auto invalid = run_child([] {
        return sash::run(
            "invalid-signal", [] { sash::signal(SIGKILL, sash::SignalDisposition::Flag); },
            [](const sash::Shutdown &shutdown) {
                require(shutdown.reason == sash::ShutdownReason::Failure && shutdown.status == 2,
                        "failed registration uses the ordinary body error boundary");
            });
    });
    require(invalid.status == 2 &&
                invalid.diagnostic.find("install signal disposition") != std::string::npos &&
                invalid.diagnostic.find("cleanup failed") == std::string::npos,
            "failed registration must not leave a snapshot that fails again during restoration");
}

void lifecycle_foreground_signals(const std::string &helper) {
    in_child([&] {
        default_signal(SIGUSR1);
        for (const auto disposition :
             {sash::SignalDisposition::Flag, sash::SignalDisposition::Exit}) {
            bool completed = false;
            int ends = 0;
            const int status = sash::run(
                "foreground-signal",
                [&] {
                    sash::signal(SIGUSR1, disposition);
                    sash::checkpoint();
                    require(sash::try_execute(helper, {"signal-parent", std::to_string(SIGUSR1)})
                                .success(),
                            "a signal must not erase foreground command completion");
                    completed = true;
                    require(sash::signum() == SIGUSR1, "record the actual foreground signal");
                    sash::checkpoint();
                },
                [&](const sash::Shutdown &shutdown) {
                    ++ends;
                    require(completed,
                            "blocking foreground work completes before checkpoint shutdown");
                    int child_status;
                    require(::waitpid(-1, &child_status, WNOHANG) < 0 && errno == ECHILD,
                            "signal shutdown must not leave a foreground child");
                    require(shutdown.reason == (disposition == sash::SignalDisposition::Exit
                                                    ? sash::ShutdownReason::Signal
                                                    : sash::ShutdownReason::Completed),
                            "foreground FLAG and EXIT must keep separate behavior");
                });
            require(status == (disposition == sash::SignalDisposition::Exit ? 128 + SIGUSR1 : 0) &&
                        ends == 1,
                    "foreground signals use ordinary-context END once");
        }
    });
    in_child([&] {
        default_signal(SIGPIPE);
        default_signal(SIGUSR1);
        int ack[2];
        require(::pipe(ack) == 0, "create signal producer handshake");
        require(::fcntl(ack[1], F_SETFD, FD_CLOEXEC) == 0,
                "the acknowledgement writer must not survive exec");
        std::string target = "unchanged";
        bool acknowledged = false;
        const auto output = sash::capture_stdout([&] {
            const int status = sash::run(
                "pipeline-signal",
                [&] {
                    sash::signal(SIGUSR1, sash::SignalDisposition::Exit);
                    target = sash::capture_stdout([&] {
                        sash::read_pipeline(
                            {{helper,
                              {"signal-handshake", std::to_string(SIGUSR1),
                               std::to_string(ack[0])}},
                             {helper, {"relay"}}},
                            [&](sash::LineReader &input) {
                                HandshakeWriter writer{ack[1]};
                                std::string line;
                                require(input.readline(line) && line == "ready",
                                        "signal producer must wait for the consumer");
                                sash::write("Y", ack[1]);
                                acknowledged = true;
                                require(input.readline(line), "retry signal-interrupted reader");
                                sash::checkpoint();
                                throw std::runtime_error("pipeline EXIT checkpoint returned");
                            });
                    });
                },
                [&](const sash::Shutdown &shutdown) {
                    require(
                        acknowledged && shutdown.signal == SIGUSR1 && target == "unchanged",
                        "signal during an active capture must unwind without partial assignment");
                    int child_status;
                    require(::waitpid(-1, &child_status, WNOHANG) < 0 && errno == ECHILD,
                            "signal unwind closes every pipeline endpoint before reaping");
                    sash::write("END\n");
                });
            require(status == 128 + SIGUSR1, "captured pipeline uses the recorded EXIT signal");
        });
        require(::close(ack[0]) == 0 && ack[1] == -1, "close signal handshake");
        require(output == "END\n", "signal END must use the restored program stdout");
    });
    const auto before_ack = run_child([&] {
        int ack[2];
        require(::pipe(ack) == 0 && ::fcntl(ack[1], F_SETFD, FD_CLOEXEC) == 0,
                "prepare an exec-private acknowledgement writer");
        int ends = 0;
        const int status = sash::run(
            "before-ack",
            [&] {
                sash::read_from(
                    helper, {"signal-handshake", std::to_string(SIGUSR1), std::to_string(ack[0])},
                    [&](sash::LineReader &input) {
                        HandshakeWriter writer{ack[1]};
                        std::string line;
                        require(input.readline(line) && line == "ready",
                                "the producer must wait for acknowledgement");
                        sash::check(
                            sash::CommandResult{4, "consumer failed before acknowledgement"});
                    });
            },
            [&](const sash::Shutdown &shutdown) {
                ++ends;
                require(shutdown.reason == sash::ShutdownReason::Failure && shutdown.status == 4,
                        "consumer failure must survive producer handshake EOF");
                int child_status;
                require(::waitpid(-1, &child_status, WNOHANG) < 0 && errno == ECHILD,
                        "pre-acknowledgement failure must reap its producer before END");
            });
        require(::close(ack[0]) == 0 && ack[1] == -1 && ends == 1,
                "the failed consumer must close its borrowed acknowledgement channel");
        return status;
    });
    require(before_ack.status == 4 && before_ack.diagnostic ==
                                          "command-helper: missing signal handshake reply\n"
                                          "before-ack: consumer failed before acknowledgement\n",
            "consumer failure before acknowledgement must finish without timeout or force-kill");
    const auto closed_output = run_child([&] {
        default_signal(SIGPIPE);
        int ack[2];
        require(::pipe(ack) == 0 && ::fcntl(ack[1], F_SETFD, FD_CLOEXEC) == 0,
                "prepare an exec-private output-close handshake");
        const auto result = sash::try_read_from(
            helper, {"signal-handshake", std::to_string(SIGUSR1), std::to_string(ack[0])},
            [&](sash::LineReader &input) {
                std::string line;
                require(input.readline(line) && line == "ready",
                        "read readiness before closing the unacknowledged producer output");
            });
        const auto *exit = std::get_if<sash::ProcessExit>(&result.outcome);
        require(exit != nullptr && exit->status == 2 && ::fcntl(ack[1], F_GETFD) >= 0,
                "closed output must release the producer while the acknowledgement writer lives");
        int child_status;
        require(::waitpid(-1, &child_status, WNOHANG) < 0 && errno == ECHILD,
                "the unacknowledged producer must be reaped after output closes");
        require(::close(ack[0]) == 0 && ::close(ack[1]) == 0, "close the output-only handshake");
        return 0;
    });
    require(closed_output.status == 0 &&
                closed_output.diagnostic ==
                    "command-helper: signal output closed before acknowledgement\n",
            "ready/read/close must detect pipe closure without acknowledgement or scheduling luck");
    const auto before_consumer = run_child([&] {
        default_signal(SIGPIPE);
        int ack[2];
        require(::pipe(ack) == 0 && ::fcntl(ack[1], F_SETFD, FD_CLOEXEC) == 0,
                "prepare a partial-pipeline acknowledgement channel");
        bool called = false;
        int ends = 0;
        const int status = sash::run(
            "before-consumer",
            [&] {
                try {
                    sash::read_pipeline(
                        {{helper,
                          {"signal-handshake", std::to_string(SIGUSR1), std::to_string(ack[0])}},
                         {helper + ".missing", {}}},
                        [&](sash::LineReader &) {
                            HandshakeWriter writer{ack[1]};
                            called = true;
                        });
                } catch (const sash::SetupError &error) {
                    require(error.code().value() == ENOENT, "the second stage must fail exec");
                    throw;
                }
            },
            [&](const sash::Shutdown &shutdown) {
                ++ends;
                require(!called && shutdown.reason == sash::ShutdownReason::Failure &&
                            shutdown.status == 2,
                        "partial setup must skip the consumer and enter END");
                int child_status;
                require(::waitpid(-1, &child_status, WNOHANG) < 0 && errno == ECHILD,
                        "closed output must release the unacknowledged producer");
            });
        require(::close(ack[0]) == 0 && ::close(ack[1]) == 0 && ends == 1,
                "close acknowledgement channel when no consumer adopted its writer");
        return status;
    });
    require(before_consumer.status == 2 &&
                before_consumer.diagnostic.find("exec " + helper + ".missing") != std::string::npos,
            "pre-consumer setup failure must complete without timeout or force-kill");
}

void lifecycle_main_only() {
    in_child([] {
        int marker[2];
        require(::pipe(marker) == 0, "create main-only END marker");
        pid_t child = -1;
        require(sash::run(
                    "main-only",
                    [&] {
                        child = ::fork();
                        require(child >= 0, "fork program body fixture");
                    },
                    [&](const sash::Shutdown &) { sash::write("E", marker[1]); }) == 0,
                "program owner must complete normally");
        int status;
        require(::waitpid(child, &status, 0) == child && WIFEXITED(status) &&
                    WEXITSTATUS(status) == 0,
                "forked body must leave without parent END");
        require(::close(marker[1]) == 0, "close END marker writer");
        char bytes[2];
        require(::read(marker[0], bytes, sizeof(bytes)) == 1 && bytes[0] == 'E',
                "only the main process may write the END marker");
        require(::close(marker[0]) == 0, "close END marker reader");

        child = -1;
        int ends = 0;
        require(sash::run(
                    "fork-in-END", [] {},
                    [&](const sash::Shutdown &) {
                        ++ends;
                        child = ::fork();
                        require(child >= 0, "fork END fixture");
                        if (child == 0) {
                            sash::exit_program(6);
                        }
                    }) == 0 &&
                    ends == 1,
                "fork inside END must not re-enter it");
        require(::waitpid(child, &status, 0) == child && WIFEXITED(status) &&
                    WEXITSTATUS(status) == 6,
                "a forked END child must take its own explicit exit");
    });
}

void lifecycle_signal_install_failure() {
    int ends = 0;
    const int status = sash::run(
        "signal-fault", [] { sash::signal(SIGUSR1, sash::SignalDisposition::Flag); },
        [&](const sash::Shutdown &shutdown) {
            ++ends;
            require(shutdown.reason == sash::ShutdownReason::Failure && shutdown.status == 2,
                    "expected injected signal installation failure");
        });
    require(status == 2 && ends == 1, "installation failure must reach END once");
}

void lifecycle_signal_end_failure(bool mask_failure) {
    bool caught = false;
    const int status = sash::run(
        "END-signal-fault", [] { sash::exit_program(7); },
        [&](const sash::Shutdown &) {
            try {
                sash::signal(SIGUSR1, sash::SignalDisposition::Flag);
            } catch (const sash::SetupError &error) {
                caught =
                    error.code().value() == EIO &&
                    std::string(error.what())
                            .find(mask_failure ? "block signals during lifecycle handoff"
                                               : "install signal disposition") != std::string::npos;
            }
        });
    require(caught && status == 7, "END must explicitly catch setup failure and preserve status");
}

void lifecycle_signal_restore_failure() {
    (void)sash::run(
        "restore-fault", [] { sash::signal(SIGUSR1, sash::SignalDisposition::Flag); },
        [](const sash::Shutdown &) { (void)sash::try_write("__END_BEFORE_RESTORE__\n", 2); });
    throw std::runtime_error("expected injected fatal signal restoration failure");
}

void lifecycle_handoff_failure() {
    {
        AllocationFailure fault(1);
        (void)sash::run(
            "handoff-fault", [] {},
            [](const sash::Shutdown &) { (void)sash::try_write("__HANDOFF_END__\n", 2); });
    }
    throw std::runtime_error("expected injected fatal lifecycle handoff failure");
}

void collection_cells() {
    sash::Array<std::string> strings;
    sash::Array<std::int64_t> integers;
    sash::Map<std::string> labels;
    sash::Map<std::int64_t> counts;
    require(sash::cell(strings, 99).empty() && sash::cell(integers, 99) == 0 &&
                sash::cell(labels, "absent").empty() && sash::cell(counts, "absent") == 0,
            "missing cells must read typed zero");
    require(strings.empty() && integers.empty() && labels.empty() && counts.empty(),
            "reads must not insert missing cells");
    sash::set_cell(strings, 2, "");
    sash::set_cell(strings, 5, "present");
    sash::set_cell(integers, 2, 0);
    sash::set_cell(integers, 5, 7);
    sash::set_cell(labels, "-1", "");
    sash::set_cell(counts, "-1", 0);
    require(sash::len(strings) == 2 && sash::len(integers) == 2 && sash::len(labels) == 1 &&
                sash::len(counts) == 1,
            "length must count present cells, including empty and zero values");
    auto forwarded = [](sash::Array<std::int64_t> &first, sash::Array<std::int64_t> &second) {
        sash::set_cell(first, 5, sash::cell(second, 5) + 1);
        sash::unset(second, 2);
    };
    auto forward = [&](sash::Array<std::int64_t> &array) { forwarded(array, array); };
    forward(integers);
    require(sash::cell(integers, 5) == 8 && sash::len(integers) == 1,
            "C++ forwarded and aliased references must preserve collection identity");
    auto copy = strings;
    sash::set_cell(strings, 5, "changed");
    require(sash::cell(copy, 5) == "present", "whole-container copies must be independent");

    for (const auto index : {std::int64_t{-1}, std::numeric_limits<std::int64_t>::min()}) {
        const std::function<void()> operations[] = {
            [&] { (void)sash::cell(strings, index); },
            [&] { sash::set_cell(strings, index, "invalid"); },
            [&] { sash::unset(strings, index); },
            [&] { (void)sash::cell(integers, index); },
            [&] { sash::set_cell(integers, index, 3); },
            [&] { sash::unset(integers, index); },
        };
        for (const auto &operation : operations) {
            bool rejected = false;
            try {
                operation();
            } catch (const sash::SetupError &error) {
                rejected = error.code() == std::errc::invalid_argument &&
                           std::string(error.what()).find("nonnegative") != std::string::npos;
            }
            require(rejected, "negative indices must fail for every indexed-cell operation");
        }
    }
    require(sash::len(strings) == 2 && sash::cell(strings, 5) == "changed" &&
                sash::len(integers) == 1 && sash::cell(integers, 5) == 8,
            "invalid indices must leave collections unchanged");
    sash::unset(labels, "-1");
    sash::unset(counts, "absent");
    require(labels.empty() && sash::cell(counts, "-1") == 0 && sash::len(counts) == 1,
            "negative-looking string keys remain valid and missing deletion does not insert");
    strings.clear();
    integers.clear();
    require(sash::len(strings) == 0 && sash::len(integers) == 0, "clear must remove all cells");
}

void collection_traversal() {
    const std::string original(256, 'a');
    sash::Array<std::string> array{{2, original}, {5, "old"}, {9, "deleted"}};
    auto &alias = array;
    std::vector<std::string> seen;
    for (const auto &value : sash::values(array)) {
        if (value == original) {
            sash::unset(alias, 2);
            require(value == original, "loop values must survive erasure of their source cell");
            sash::set_cell(alias, 1, "earlier");
            sash::set_cell(alias, 2, "reinserted");
            sash::set_cell(alias, 3, "inserted");
            sash::set_cell(alias, 5, "updated");
            sash::unset(alias, 9);
        } else if (value == "updated") {
            alias.clear();
            require(value == "updated", "loop values must survive clearing the collection");
            sash::set_cell(alias, 0, "earlier after clear");
            sash::set_cell(alias, 6, "last");
        }
        seen.push_back(value);
    }
    require(seen == std::vector<std::string>{original, "inserted", "updated", "last"},
            "live traversal must resume at the next greater key after aliased mutations");

    sash::Map<std::int64_t> map{{"delta", 4}, {"bravo", 2}, {"foxtrot", 6}};
    std::vector<std::string> entries;
    for (const auto &key : sash::keys(map)) {
        const auto value = sash::cell(map, key);
        if (key == "bravo") {
            sash::unset(map, key);
            require(key == "bravo", "key loop variables must survive erasing the current cell");
            sash::set_cell(map, "alpha", 1);
            sash::set_cell(map, "bravo", 20);
            sash::set_cell(map, "charlie", 3);
            sash::set_cell(map, "delta", 40);
            sash::unset(map, "foxtrot");
        }
        entries.push_back(key + ":" + std::to_string(value));
    }
    require(entries == std::vector<std::string>{"bravo:2", "charlie:3", "delta:40"},
            "map traversal must be sorted, live, and never revisit earlier/current keys");
    sash::Map<std::int64_t> clear_map{{"a", 1}, {"b", 2}};
    for (const auto value : sash::values(clear_map)) {
        clear_map.clear();
        require(value == 1, "map value traversal must copy the current value");
    }
    sash::Array<std::int64_t> numbers{{2, 2}, {5, 5}};
    std::vector<std::int64_t> visited;
    for (const auto value : sash::values(numbers)) {
        visited.push_back(value);
        if (value == 2) {
            sash::set_cell(numbers, 3, 3);
            continue;
        }
        break;
    }
    require(visited == std::vector<std::int64_t>{2, 3},
            "continue must advance the live cursor and break must stop it");
    const sash::Array<std::int64_t> empty;
    for (const auto value : sash::values(empty)) {
        (void)value;
        require(false, "empty live traversal must not produce a value");
    }
}

void collection_values() {
    const std::string bytes("a\0b", 3);
    sash::Map<std::string> map{{"z", "last"}, {"a", ""}, {"m", bytes}};
    auto snapshot = sash::snapshot_keys(map);
    sash::unset(map, "m");
    sash::set_cell(map, "b", "new");
    require(sash::join(snapshot, ",") == "a,m,z" && sash::len(snapshot) == 3,
            "key snapshots must be sorted and independent from later mutation");
    require(sash::join(map, "|") == "|new|last", "map join must traverse sorted values, not keys");
    sash::Array<std::string> binary{{2, ""}, {5, bytes}, {9, std::string(8192, 'x')}};
    require(sash::join(binary, std::string_view("\0", 1)) ==
                std::string("\0", 1) + bytes + std::string("\0", 1) + std::string(8192, 'x'),
            "join must retain empty cells, binary values/separators, and output beyond 4 KiB");
    require(sash::join(binary, "") == bytes + std::string(8192, 'x'),
            "an explicit empty separator must not become the default space");
    sash::Array<std::int64_t> integers{{2, 0}, {5, std::numeric_limits<std::int64_t>::min()}};
    require(sash::join(integers) == "0 -9223372036854775808",
            "integer joins must render typed values, including zero and the signed minimum");
    require(sash::join(sash::Array<std::string>{}).empty(), "empty joins must be empty");

    sash::Array<std::string> pairs{{0, "z"},  {3, "one"},    {5, "a"},      {9, "two"},
                                   {20, "z"}, {21, "three"}, {30, "orphan"}};
    const auto elided = sash::map_pairs_elided(pairs);
    require(
        elided == sash::Map<std::string>{{"a", "two"}, {"z", "three"}},
        "pair conversion must walk sparse values, overwrite duplicate keys and elide the odd tail");
    bool used = false;
    const auto filled = sash::map_pairs_or(pairs, [&] {
        used = true;
        return "extra";
    });
    require(used && sash::cell(filled, "extra") == "orphan" && sash::len(filled) == 3,
            "an odd pair default must supply the key, not the value");
    sash::Array<std::int64_t> numeric_pairs{{0, 2}, {1, 0}, {2, 3}, {3, -7}};
    const auto numeric = sash::map_pairs_required(numeric_pairs, [] {
        throw std::runtime_error("unused pair requirement evaluated");
        return "";
    });
    require(numeric == sash::Map<std::int64_t>{{"2", 0}, {"3", -7}},
            "integer pair conversion must format keys while retaining typed values");
    require(sash::map_pairs_required(sash::Array<std::string>{},
                                     [] {
                                         throw std::runtime_error(
                                             "empty even map evaluated an odd-tail requirement");
                                         return "";
                                     })
                .empty(),
            "an empty array is a valid empty pair map");
}

void collection_dispositions() {
    sash::Array<std::string> strings{{2, ""}, {5, "present"}};
    sash::Map<std::int64_t> numbers{{"zero", 0}, {"one", 1}};
    std::size_t calls = 0;
    auto fallback = [&] {
        ++calls;
        return "default";
    };
    require(sash::default_value(sash::cell(strings, 5), fallback) == "present" && calls == 0,
            "defaults must not evaluate an unused fallback");
    require(sash::default_value(sash::cell(strings, 2), fallback) == "default" &&
                sash::default_value(sash::cell(strings, 99), fallback) == "default" && calls == 2 &&
                sash::len(strings) == 2,
            "stored empty and missing string cells must use the same lazy scalar default");
    require(sash::default_value(sash::cell(numbers, "zero"), [] { return std::int64_t{8}; }) == 8 &&
                sash::default_value(sash::cell(numbers, "missing"),
                                    [] { return std::int64_t{9}; }) == 9,
            "integer defaults must test typed zero, not key presence or text");
    require(sash::default_value(std::string{"0"},
                                [] {
                                    throw std::runtime_error(
                                        "nonempty string zero evaluated its fallback");
                                    return "";
                                }) == "0",
            "string zero must remain distinct from integer zero for dispositions");
    require(sash::required_value(sash::cell(strings, 5),
                                 [] {
                                     throw std::runtime_error("unused requirement evaluated");
                                     return "";
                                 }) == "present",
            "required values must not evaluate the message on success");

    const std::function<void()> failures[] = {
        [&] { (void)sash::required_value(sash::cell(strings, 2), [] { return "empty cell"; }); },
        [&] {
            (void)sash::required_value(sash::cell(numbers, "zero"), [] { return "zero cell"; });
        },
        [&] {
            std::vector<std::string> arguments{"kept"};
            try {
                sash::append_splat_required(arguments, sash::Array<std::string>{},
                                            [] { return "empty splat"; });
            } catch (const sash::CommandError &) {
                require(arguments == std::vector<std::string>{"kept"},
                        "a required empty splat must leave earlier arguments unchanged");
                throw;
            }
        },
        [&] {
            (void)sash::map_pairs_required(sash::Array<std::string>{{2, "odd"}},
                                           [] { return "odd pair"; });
        },
    };
    for (const auto &failure : failures) {
        bool rejected = false;
        try {
            failure();
        } catch (const sash::CommandError &error) {
            rejected =
                error.status() == 2 && std::string(error.what()).find("required value: ") == 0;
        }
        require(rejected, "requirements must raise the ordinary checked failure");
    }
    std::vector<std::string> arguments;
    sash::append_argument(arguments, "");
    sash::append_elided(arguments, sash::cell(strings, 2));
    sash::append_elided(arguments, sash::cell(numbers, "zero"));
    sash::append_elided(arguments, sash::cell(numbers, "one"));
    sash::append_elided(arguments, "0");
    sash::append_splat_or(arguments, sash::Array<std::string>{}, [] { return ""; });
    sash::append_splat_or(arguments, strings, [] {
        throw std::runtime_error("unused splat default evaluated");
        return "";
    });
    require(arguments == std::vector<std::string>{"", "1", "0", "", "", "present"},
            "only explicit elision drops a scalar; splats retain present empty cells");
    const auto escaped = run_child([] {
        return sash::run("collection-report", [] {
            (void)sash::required_value(std::string{}, [] { return "missing option"; });
            std::_Exit(99);
        });
    });
    require(escaped.status == 2 &&
                escaped.diagnostic == "collection-report: required value: missing option\n",
            "uncaught collection requirements must reach the existing program boundary once");
}

void collection_arguments(const std::string &helper) {
    sash::Array<std::string> strings{{2, ""}, {7, "a b"}};
    sash::Map<std::int64_t> numbers{{"z", -4}, {"a", 0}};
    std::vector<std::string> arguments{"args"};
    sash::append_argument(arguments, "");
    sash::append_elided(arguments, "");
    sash::append_elided(arguments, std::int64_t{0});
    sash::append_argument(arguments, std::int64_t{0});
    sash::append_splat(arguments, strings);
    sash::append_splat_required(arguments, numbers, [] { return "expected counts"; });
    sash::set_cell(strings, 7, "changed");
    sash::set_cell(numbers, "a", 99);
    const char expected[] = "\0"
                            "0\0"
                            "\0"
                            "a b\0"
                            "0\0"
                            "-4\0";
    require(sash::capture_pipeline({{helper, arguments}}) ==
                std::string(expected, sizeof(expected) - 1),
            "exec arguments must own snapshots, preserve empty cells and stringify integer splats");
    require(sash::capture_pipeline({{helper, {"args"}}}).empty(),
            "zero arguments must remain distinct from one empty argument");
    std::vector<std::string> self{"owned"};
    sash::append_argument(self, self.front());
    require(self == std::vector<std::string>{"owned", "owned"},
            "scalar argument construction must be safe when its source aliases the destination");
    sash::Array<std::string> binary{{0, std::string("x\0y", 3)}};
    std::vector<std::string> invalid{"args"};
    sash::append_splat(invalid, binary);
    require(invalid[1] == std::string("x\0y", 3) && sash::join(binary) == invalid[1],
            "collection and argument preparation must retain counted binary bytes");
    const auto failed = sash::try_execute(helper, invalid);
    const auto *error = std::get_if<sash::SetupError>(&failed.outcome);
    require(error != nullptr && error->code() == std::errc::invalid_argument &&
                std::string(error->what()).find("argument 2 contains NUL") != std::string::npos,
            "the existing exec boundary must reject NUL instead of truncating an argument");
}

void collection_allocation() {
    sash::Array<std::string> array{{2, "short"}, {5, std::string(2048, 'x')}};
    std::vector<std::string> arguments{"kept"};
    bool failed = false;
    {
        AllocationFailure fault(1024);
        try {
            sash::append_splat(arguments, array);
        } catch (const std::bad_alloc &) {
            failed = true;
        }
    }
    require(failed && arguments == std::vector<std::string>{"kept"} && sash::len(array) == 2,
            "failed splat preparation must not publish partial arguments or change its source");
    sash::Map<std::string> map{{std::string(2048, 'k'), "value"}};
    sash::Array<std::string> snapshot{{0, "kept"}};
    failed = false;
    {
        AllocationFailure fault(1024);
        try {
            snapshot = sash::snapshot_keys(map);
        } catch (const std::bad_alloc &) {
            failed = true;
        }
    }
    require(failed && snapshot == sash::Array<std::string>{{0, "kept"}},
            "failed key snapshots must not replace the previous destination");
    failed = false;
    {
        AllocationFailure fault(1024);
        try {
            (void)sash::cell(array, 5);
        } catch (const std::bad_alloc &) {
            failed = true;
        }
        require(sash::cell(array, 99).empty(), "a missing valid cell must not need allocation");
    }
    require(failed && sash::len(array) == 2,
            "allocation errors in a present-cell read must not become a missing-cell zero");
    const auto reported = run_child([] {
        return sash::run("collection-report", [] {
            sash::Array<std::string> array{{2, std::string(2048, 'x')}};
            std::vector<std::string> arguments;
            AllocationFailure fault(1024);
            sash::append_splat(arguments, array);
        });
    });
    require(reported.status == 2 && reported.diagnostic == "collection-report: out of memory\n",
            "collection allocation failures must reach the existing allocation-safe reporter");
}

void arithmetic_values() {
    using Integer = std::int64_t;
    using Binary = Integer (*)(Integer, Integer);
    constexpr auto minimum = std::numeric_limits<Integer>::min();
    constexpr auto maximum = std::numeric_limits<Integer>::max();
    const struct {
        Binary operation;
        Integer left;
        Integer right;
        Integer expected;
        const char *message;
    } values[] = {
        {sash::add, 41, 1, 42, "ordinary checked addition"},
        {sash::add, minimum, maximum, -1, "addition across opposite extremes"},
        {sash::add, maximum, minimum, -1, "addition with INT64_MIN on the right"},
        {sash::add, maximum, -maximum, 0, "addition may cancel at the boundary"},
        {sash::sub, minimum, minimum, 0, "subtracting INT64_MIN from itself"},
        {sash::sub, -1, minimum, maximum, "subtraction of INT64_MIN can reach INT64_MAX"},
        {sash::sub, maximum, maximum, 0, "subtracting INT64_MAX from itself"},
        {sash::sub, minimum, -1, minimum + 1, "subtraction above INT64_MIN"},
        {sash::sub, maximum, 1, maximum - 1, "subtraction below INT64_MAX"},
        {sash::mul, -7, 6, -42, "negative times positive"},
        {sash::mul, 7, -6, -42, "positive times negative"},
        {sash::mul, -7, -6, 42, "negative times negative"},
        {sash::mul, minimum, 1, minimum, "INT64_MIN times one"},
        {sash::mul, 1, minimum, minimum, "one times INT64_MIN"},
        {sash::mul, minimum, 0, 0, "INT64_MIN times zero"},
        {sash::mul, 0, minimum, 0, "zero times INT64_MIN"},
        {sash::mul, maximum / 2, 2, maximum - 1, "positive product at the overflow boundary"},
        {sash::mul, minimum / 2, 2, minimum, "negative product at the overflow boundary"},
        {sash::mul, -maximum, -1, maximum, "negative product at INT64_MAX"},
        {sash::div, -17, 5, -3, "division truncates toward zero"},
        {sash::div, 17, -5, -3, "negative-divisor division"},
        {sash::div, minimum, 1, minimum, "representable division of INT64_MIN"},
        {sash::div, minimum, minimum, 1, "division of equal negative extremes"},
        {sash::mod, -17, 5, -2, "remainder follows the dividend sign"},
        {sash::mod, 17, -5, 2, "negative divisor does not change remainder sign"},
        {sash::mod, minimum, -1, 0, "INT64_MIN modulo minus one is not a fault"},
        {sash::mod, minimum, minimum, 0, "remainder of equal negative extremes"},
        {sash::pow, 0, 0, 1, "zero exponent returns one"},
        {sash::pow, 0, 7, 0, "positive power of zero"},
        {sash::pow, 2, 10, 1024, "ordinary checked exponentiation"},
        {sash::pow, -2, 3, -8, "odd power retains a negative sign"},
        {sash::pow, -2, 63, minimum, "a negative power may reach INT64_MIN exactly"},
        {sash::pow, maximum, 1, maximum, "exponent one must not square an unused base"},
        {sash::pow, minimum, 1, minimum, "INT64_MIN to exponent one"},
        {sash::pow, 1, maximum, 1, "large exponents require a bounded power loop"},
        {sash::pow, -1, maximum, -1, "large odd exponents retain parity"},
        {sash::pow, 2, -3, 0, "negative exponents retain sash's integer result"},
        {sash::pow, 1, minimum, 0, "negative exponents are not negated or converted to unsigned"},
    };
    for (const auto &value : values) {
        try {
            require(value.operation(value.left, value.right) == value.expected, value.message);
        } catch (const sash::ArithmeticError &) {
            require(false, value.message);
        }
    }
    require(sash::neg(0) == 0 && sash::neg(-17) == 17 && sash::neg(maximum) == -maximum,
            "representable negation must preserve the signed value");

    const struct {
        Binary operation;
        Integer left;
        Integer right;
        sash::ArithmeticFault fault;
        const char *diagnostic;
        const char *message;
    } faults[] = {
        {sash::add, maximum, 1, sash::ArithmeticFault::Overflow, "arithmetic overflow",
         "addition above INT64_MAX must fault"},
        {sash::add, minimum, -1, sash::ArithmeticFault::Overflow, "arithmetic overflow",
         "addition below INT64_MIN must fault"},
        {sash::add, -1, minimum, sash::ArithmeticFault::Overflow, "arithmetic overflow",
         "addition of INT64_MIN must check the bound without negating it"},
        {sash::sub, minimum, 1, sash::ArithmeticFault::Overflow, "arithmetic overflow",
         "subtraction below INT64_MIN must fault"},
        {sash::sub, maximum, -1, sash::ArithmeticFault::Overflow, "arithmetic overflow",
         "subtraction above INT64_MAX must fault"},
        {sash::sub, 0, minimum, sash::ArithmeticFault::Overflow, "arithmetic overflow",
         "subtraction of INT64_MIN must fault without negating it first"},
        {sash::mul, maximum, 2, sash::ArithmeticFault::Overflow, "arithmetic overflow",
         "positive multiplication overflow must fault"},
        {sash::mul, minimum, 2, sash::ArithmeticFault::Overflow, "arithmetic overflow",
         "negative multiplication overflow must fault"},
        {sash::mul, minimum, -1, sash::ArithmeticFault::Overflow, "arithmetic overflow",
         "INT64_MIN times minus one must fault"},
        {sash::mul, -1, minimum, sash::ArithmeticFault::Overflow, "arithmetic overflow",
         "minus one times INT64_MIN must fault"},
        {sash::div, minimum, -1, sash::ArithmeticFault::Overflow, "arithmetic overflow",
         "INT64_MIN divided by minus one must fault"},
        {sash::div, minimum, 0, sash::ArithmeticFault::DivisionByZero, "division by zero",
         "division by zero must precede any raw division"},
        {sash::mod, minimum, 0, sash::ArithmeticFault::DivisionByZero, "division by zero",
         "modulo by zero must precede any raw remainder"},
        {sash::pow, 2, 63, sash::ArithmeticFault::Overflow, "arithmetic overflow",
         "positive exponentiation overflow must fault"},
        {sash::pow, -2, 64, sash::ArithmeticFault::Overflow, "arithmetic overflow",
         "even exponentiation overflow must fault"},
    };
    for (const auto &fault : faults) {
        bool caught = false;
        try {
            (void)fault.operation(fault.left, fault.right);
        } catch (const sash::ArithmeticError &error) {
            caught =
                error.fault() == fault.fault && std::string_view(error.what()) == fault.diagnostic;
        }
        require(caught, fault.message);
    }
    bool caught = false;
    try {
        (void)sash::neg(minimum);
    } catch (const sash::ArithmeticError &error) {
        caught = error.fault() == sash::ArithmeticFault::Overflow;
    }
    require(caught, "negating INT64_MIN must fault");
}

void arithmetic_shifts() {
    using Integer = std::int64_t;
    using Binary = Integer (*)(Integer, Integer);
    constexpr auto minimum = std::numeric_limits<Integer>::min();
    constexpr auto maximum = std::numeric_limits<Integer>::max();
    const struct {
        Binary operation;
        Integer value;
        Integer count;
        Integer expected;
        const char *message;
    } values[] = {
        {sash::shl, minimum, 0, minimum, "zero left shift preserves the sign bit"},
        {sash::shl, 1, 63, minimum, "left shift can set the sign bit without overflow"},
        {sash::shl, -1, 63, minimum, "negative left operand is a bit pattern"},
        {sash::shl, maximum, 1, -2, "left shift discards overflowing value bits"},
        {sash::shl, minimum, 1, 0, "left shift can discard every set bit"},
        {sash::shl, -5, 1, -10, "negative left shift is well-defined"},
        {sash::shl, 8, 64, 0, "left shift by the width does not mask the count"},
        {sash::shl, -5, 200, 0, "wide left shift discards negative value bits"},
        {sash::shl, 8, maximum, 0, "large left shift counts remain unbounded"},
        {sash::shr, minimum, 0, minimum, "zero right shift preserves the value"},
        {sash::shr, -9, 1, -5, "signed right shift floors rather than truncates"},
        {sash::shr, minimum, 63, -1, "right shift fills the sign bit"},
        {sash::shr, maximum, 63, 0, "positive right shift does not add sign bits"},
        {sash::shr, 8, 64, 0, "wide positive right shift returns zero"},
        {sash::shr, -9, 64, -1, "wide negative right shift returns minus one"},
        {sash::shr, -1, maximum, -1, "large right shift counts saturate to the sign"},
        {sash::shr, 0, maximum, 0, "large right shift of zero remains zero"},
    };
    for (const auto &value : values) {
        require(value.operation(value.value, value.count) == value.expected, value.message);
    }
    for (Binary operation : {sash::shl, sash::shr}) {
        for (Integer count : {Integer{-1}, minimum}) {
            bool caught = false;
            try {
                (void)operation(0, count);
            } catch (const sash::ArithmeticError &error) {
                caught = error.fault() == sash::ArithmeticFault::NegativeShiftCount &&
                         std::string_view(error.what()) == "negative shift count";
            }
            require(caught, "even a zero operand must reject a negative shift count");
        }
    }
}

void arithmetic_failures() {
    constexpr auto minimum = std::numeric_limits<std::int64_t>::min();
    constexpr auto maximum = std::numeric_limits<std::int64_t>::max();
    const auto failed = run_child([&] {
        std::int64_t target = 73;
        int ends = 0;
        bool caught_as_command = false;
        bool later = false;
        const int status = sash::run(
            "arithmetic-discard",
            [&] {
                const auto f = [&]() -> std::int64_t {
                    target = sash::add(maximum, 1);
                    return target;
                };
                try {
                    (void)f();
                } catch (const sash::CommandError &) {
                    caught_as_command = true;
                }
                later = true;
            },
            [&](const sash::Shutdown &shutdown) {
                ++ends;
                require(shutdown.reason == sash::ShutdownReason::Failure && shutdown.status == 2 &&
                            target == 73 && !caught_as_command && !later,
                        "arithmetic faults preserve the target and cannot become command outcomes");
            });
        require(ends == 1, "arithmetic failure must unwind into owner END once");
        return status;
    });
    require(failed.status == 2 && failed.diagnostic == "arithmetic-discard: arithmetic overflow\n",
            "ignoring a function result must not suppress its arithmetic failure");

    const auto divided = run_child([] {
        return sash::run("arithmetic-condition", [] {
            if (sash::div(7, 0) != 0) {
                require(false, "a division fault must not reach the condition body");
            }
        });
    });
    require(divided.status == 2 && divided.diagnostic == "arithmetic-condition: division by zero\n",
            "an arithmetic condition must report its fault instead of yielding false");
    const auto shifted = run_child([] {
        return sash::run("arithmetic-argument", [] {
            const auto f = [](std::int64_t) {
                require(false, "a failed argument must not call f");
            };
            f(sash::shr(0, -1));
        });
    });
    require(shifted.status == 2 &&
                shifted.diagnostic == "arithmetic-argument: negative shift count\n",
            "argument evaluation retains the arithmetic failure boundary");

    bool denied = false;
    try {
        const AllocationFailure fault(1);
        const std::string control(128, 'x');
        require(control.size() == 128, "allocation-fault positive control");
    } catch (const std::bad_alloc &) {
        denied = true;
    }
    require(denied, "arithmetic allocation control must actually reject an allocation");
    bool caught = false;
    {
        const AllocationFailure fault(1);
        try {
            (void)sash::neg(minimum);
        } catch (const sash::ArithmeticError &error) {
            caught = error.fault() == sash::ArithmeticFault::Overflow &&
                     std::string_view(error.what()) == "arithmetic overflow";
        }
    }
    require(caught, "an arithmetic diagnostic payload must not require a C++ allocation");

    bool handled = false;
    require(sash::run(
                "arithmetic-END", [] {},
                [&](const sash::Shutdown &) {
                    try {
                        (void)sash::mod(7, 0);
                    } catch (const sash::ArithmeticError &error) {
                        handled = error.fault() == sash::ArithmeticFault::DivisionByZero;
                    }
                }) == 0 &&
                handled,
            "END can explicitly handle its own arithmetic failure");
    bool escaped = false;
    bool later = false;
    try {
        (void)sash::run(
            "invalid-arithmetic-END", [] {},
            [&](const sash::Shutdown &) {
                (void)sash::mul(maximum, 2);
                later = true;
            });
    } catch (const sash::ArithmeticError &error) {
        escaped = error.fault() == sash::ArithmeticFault::Overflow;
    }
    require(escaped && !later,
            "unhandled END arithmetic must not receive automatic failure handling");
}

void numeric_parsing() {
    constexpr auto minimum = std::numeric_limits<std::int64_t>::min();
    constexpr auto maximum = std::numeric_limits<std::int64_t>::max();
    const std::pair<std::string_view, std::int64_t> values[] = {
        {"0", 0},
        {"-0", 0},
        {"+0", 0},
        {"08", 8},
        {"010", 10},
        {"+42", 42},
        {"-17", -17},
        {"0xff", 255},
        {"-0XFF", -255},
        {"+0o10", 8},
        {"0O377", 255},
        {"0b1010", 10},
        {"-0B1010", -10},
        {"9223372036854775807", maximum},
        {"-9223372036854775808", minimum},
        {"0x7fffffffffffffff", maximum},
        {"-0x8000000000000000", minimum},
    };
    for (const auto &value : values) {
        require(sash::parse_int(value.first) == value.second && sash::is_int(value.first),
                "strict integer parsing must preserve complete signed decimal/prefixed values");
    }
    const std::string_view invalid[] = {
        {},
        "",
        "+",
        "-",
        " 42",
        "42 ",
        "42\n",
        "42abc",
        "16#ff",
        "--1",
        "+-1",
        "0x",
        "-0o",
        "0b2",
        "0o8",
        "0x+1",
        "1.0",
        "1_000",
        "9223372036854775808",
        "-9223372036854775809",
        "0x8000000000000000",
        "184467440737095516160",
        std::string_view{"42\0tail", 7},
        std::string_view{"\xff", 1},
    };
    for (const auto text : invalid) {
        require(!sash::parse_int(text) && !sash::is_int(text),
                "strict integer parsing must reject partial, malformed and out-of-range input");
    }
    const std::string zeros = std::string(512, '0') + "8";
    require(sash::parse_int(zeros) == 8, "long leading-zero input is still decimal data");
    bool denied = false;
    try {
        const AllocationFailure fault(1);
        const std::string control(128, 'x');
        require(control.size() == 128, "integer parser allocation control");
    } catch (const std::bad_alloc &) {
        denied = true;
    }
    require(denied, "integer parser allocation control must actually deny allocation");
    {
        const AllocationFailure fault(1);
        require(sash::parse_int("-0x8000000000000000") == minimum && sash::is_int(zeros) &&
                    sash::int_or("bad", 7) == 7 &&
                    sash::int_required("0", [] { return "unused"; }) == 0,
                "integer validation and successful/default conversion must not allocate");
    }
}

void program_arguments() {
    char word[] = "two words";
    const char *input[] = {"program", word, "", "08"};
    auto snapshot = sash::argv(4, input);
    const auto slice = sash::argv(4, input, 1);
    require(sash::len(snapshot) == 4 && sash::cell(snapshot, 0) == "program" &&
                sash::len(slice) == 3 && sash::cell(slice, 0) == "two words" &&
                sash::cell(slice, 1).empty() && sash::cell(slice, 2) == "08",
            "argv snapshots include argv0 and slices reindex without dropping empty arguments");
    word[0] = 'T';
    require(sash::cell(snapshot, 1) == "two words" && sash::cell(slice, 0) == "two words" &&
                sash::arg(4, input, 1) == "Two words",
            "argv snapshots must own copies independent of their C input");
    sash::set_cell(snapshot, 0, "changed");
    require(sash::arg(4, input, 0) == "program",
            "changing a snapshot must not mutate the original program arguments");
    require(sash::arg(4, input, 2, "fallback").empty() &&
                sash::arg(4, input, 4, "fallback") == "fallback" &&
                sash::argv(4, input, 4).empty() &&
                sash::argv(4, input, std::numeric_limits<std::int64_t>::max()).empty() &&
                sash::argv(0, nullptr).empty(),
            "argument defaults test presence and oversized slices are empty");
    bool caught = false;
    try {
        (void)sash::arg(4, input, 4);
    } catch (const sash::SetupError &error) {
        caught = error.code() == std::errc::result_out_of_range;
    }
    require(caught, "a missing required argument must not silently yield empty text");
    const std::function<void()> invalid[] = {
        [&] { (void)sash::argv(4, input, -1); },
        [&] { (void)sash::arg(4, input, -1, "fallback"); },
        [] { (void)sash::argv(-1, nullptr); },
        [] { (void)sash::arg(1, nullptr, 0, "fallback"); },
    };
    for (const auto &operation : invalid) {
        caught = false;
        try {
            operation();
        } catch (const sash::SetupError &error) {
            caught = error.code() == std::errc::invalid_argument;
        }
        require(caught, "invalid argument metadata and negative indices must fail explicitly");
    }
    const char *broken[] = {"program", nullptr};
    sash::Array<std::string> target{{9, "kept"}};
    caught = false;
    try {
        target = sash::argv(2, broken);
    } catch (const sash::SetupError &) {
        caught = true;
    }
    require(caught && target == sash::Array<std::string>{{9, "kept"}},
            "a malformed argv snapshot must not publish partial replacement");
    const std::string long_word(256, 'x');
    const char *large[] = {"program", long_word.c_str()};
    caught = false;
    try {
        const AllocationFailure fault(128);
        target = sash::argv(2, large);
    } catch (const std::bad_alloc &) {
        caught = true;
    }
    require(caught && target == sash::Array<std::string>{{9, "kept"}},
            "an argv allocation failure must preserve the destination after partial construction");
}

void environment_values() {
    const auto child = run_child([] {
        constexpr const char *key = "SASH_CPP_INPUT_TEST";
        require(::unsetenv(key) == 0, "unset input fixture environment");
        require(sash::run("missing-environment",
                          [&] {
                              require(!sash::getenv(key),
                                      "an absent environment name has no value");
                          }) == 0,
                "an absent environment lookup is a result, not an automatic abort");
        require(::setenv(key, "", 1) == 0, "set empty input fixture environment");
        const auto empty = sash::getenv(key);
        require(empty && empty->empty(),
                "a present empty environment value is a successful lookup");
        const std::string value = std::string("two words:") + char(0xff);
        require(::setenv(key, value.c_str(), 1) == 0, "set counted input fixture environment");
        const std::string extended_key = std::string(key) + ".unused";
        const auto copy = sash::getenv(std::string_view(extended_key).substr(0, std::strlen(key)));
        require(copy && *copy == value,
                "getenv must honor the counted key and copy exact value bytes");
        require(::setenv(key, "changed", 1) == 0 && copy && *copy == value,
                "environment values must remain owned after the host environment changes");
        bool caught = false;
        const std::string binary_key = std::string(key) + std::string("\0tail", 5);
        try {
            (void)sash::getenv(binary_key);
        } catch (const sash::SetupError &error) {
            caught = error.code() == std::errc::invalid_argument;
        }
        require(caught, "getenv must reject a NUL key rather than look up its truncated prefix");
        const std::string large(256, 'v');
        require(::setenv(key, large.c_str(), 1) == 0, "set allocating environment value");
        caught = false;
        try {
            const AllocationFailure fault(128);
            (void)sash::getenv(key);
        } catch (const std::bad_alloc &) {
            caught = true;
        }
        require(caught, "an environment allocation failure must not become a missing lookup");
        return 0;
    });
    if (child.status != 0 || !child.diagnostic.empty()) {
        throw std::runtime_error("environment fixture child exited " +
                                 std::to_string(child.status) + ": " + child.diagnostic);
    }
}

void numeric_dispositions() {
    const std::string partial = "12tail";
    require(sash::default_value(partial, [] { return "unused"; }) == partial &&
                sash::int_or(partial, 9) == 9 && sash::int_or("0", 9) == 0,
            "numeric defaults trigger on invalid text, not nonempty text or valid zero");
    sash::Array<std::string> strings{{0, "08"}, {2, "bad"}, {5, ""}};
    sash::Map<std::string> map{{"zero", "0"}, {"large", "9223372036854775808"}};
    require(sash::int_or(sash::cell(strings, 0), 3) == 8 &&
                sash::int_or(sash::cell(strings, 2), 11) == 11 &&
                sash::int_or(sash::cell(strings, 5), 13) == 13 &&
                sash::int_or(sash::cell(strings, 99), 17) == 17 &&
                sash::int_or(sash::cell(map, "large"), 19) == 19 &&
                sash::int_required(sash::cell(map, "zero"), [] { return "unused"; }) == 0 &&
                sash::len(strings) == 3 && sash::len(map) == 2,
            "scalar and array/map cell conversions must share strict value semantics");
    int messages = 0;
    const auto message = [&] {
        ++messages;
        return "not an integer";
    };
    require(sash::int_required("0", message) == 0 &&
                sash::int_required("-0x8000000000000000", message) ==
                    std::numeric_limits<std::int64_t>::min() &&
                messages == 0,
            "valid required conversions must not evaluate their message");
    for (std::string_view text : {"", "42tail", "9223372036854775808"}) {
        bool caught = false;
        try {
            (void)sash::int_required(text, message);
        } catch (const sash::CommandError &error) {
            caught = error.status() == 2 &&
                     std::string_view(error.what()) == "required value: not an integer";
        }
        require(
            caught,
            "invalid and out-of-range conversion must use the requirement, not ArithmeticError");
    }
    require(messages == 3, "each failed conversion must evaluate its requirement exactly once");
    const auto failed = run_child([] {
        std::int64_t target = 73;
        int ends = 0;
        const int status = sash::run(
            "numeric-input",
            [&] { target = sash::int_required("9223372036854775808", [] { return "bad count"; }); },
            [&](const sash::Shutdown &shutdown) {
                ++ends;
                require(target == 73 && shutdown.reason == sash::ShutdownReason::Failure &&
                            shutdown.status == 2,
                        "failed numeric conversion must preserve assignment and unwind into END");
            });
        require(ends == 1, "a conversion requirement must enter owner END once");
        return status;
    });
    require(failed.status == 2 && failed.diagnostic == "numeric-input: required value: bad count\n",
            "out-of-range input must report its requested message, not arithmetic overflow");
    bool handled = false;
    require(sash::run(
                "numeric-END", [] {},
                [&](const sash::Shutdown &) {
                    try {
                        (void)sash::int_required("bad", [] { return "explicit"; });
                    } catch (const sash::CommandError &) {
                        handled = true;
                    }
                }) == 0 &&
                handled,
            "END can explicitly handle a numeric conversion requirement");
    bool escaped = false;
    try {
        (void)sash::run(
            "invalid-numeric-END", [] {},
            [](const sash::Shutdown &) {
                (void)sash::int_required("bad", [] { return "explicit"; });
            });
    } catch (const sash::CommandError &error) {
        escaped = std::string_view(error.what()) == "required value: explicit";
    }
    require(escaped, "unhandled END numeric conversion must not get automatic handling");
}

void byte_substrings() {
    constexpr auto minimum = std::numeric_limits<std::int64_t>::min();
    constexpr auto maximum = std::numeric_limits<std::int64_t>::max();
    struct Slice {
        std::int64_t offset;
        std::optional<std::int64_t> length;
        std::string_view expected;
    };
    const Slice slices[] = {
        {0, std::nullopt, "abcdef"},
        {2, std::nullopt, "cdef"},
        {-2, std::nullopt, "ef"},
        {-99, std::nullopt, "abcdef"},
        {99, std::nullopt, ""},
        {minimum, std::nullopt, "abcdef"},
        {maximum, std::nullopt, ""},
        {2, 2, "cd"},
        {2, 0, ""},
        {2, -1, "cde"},
        {2, -99, ""},
        {-4, -1, "cde"},
        {-4, maximum, "cdef"},
        {0, minimum, ""},
        {minimum, 2, "ab"},
    };
    for (const auto &slice : slices) {
        require(sash::substring("abcdef", slice.offset, slice.length) == slice.expected,
                "byte slices must clamp signed bounds and distinguish omitted from zero length");
    }
    require(sash::strlen({}) == 0 && sash::substring({}, 0).empty() &&
                sash::substring({}, minimum, maximum).empty(),
            "default-empty views must support byte length and slicing");
    const std::string binary("A\0b\xff"
                             "D",
                             5);
    require(sash::strlen(binary) == 5 && sash::substring(binary, 1, 3) == std::string("\0b\xff", 3),
            "byte length and slices must preserve interior NUL and high bytes");
    const std::string utf8("caf\xc3\xa9", 5);
    require(sash::strlen(utf8) == 5 && sash::substring(utf8, 3, 1) == std::string("\xc3", 1),
            "string offsets and lengths count bytes, not Unicode characters");
    std::string source = "prefix:value:suffix";
    const auto copy = sash::substring(std::string_view(source).substr(7, 5), 1, 3);
    source[8] = 'X';
    require(copy == "alu", "a counted partial slice must own its bytes independently");
    source = sash::substring(source, 7, 5);
    require(source == "vXlue", "slice assignment must remain safe when the source is its target");
    sash::Array<std::string> array{{0, "Ab Cd"}};
    sash::Map<std::string> map{{"word", "Ab Cd"}};
    require(sash::strlen(sash::cell(array, 0)) == 5 &&
                sash::substring(sash::cell(map, "word"), 1, 3) == "b C" &&
                sash::substring(sash::cell(array, 99), 0).empty() && sash::len(array) == 1 &&
                sash::len(map) == 1,
            "scalar and missing/present string cells must share non-inserting byte operations");
}

void ascii_text() {
    const std::string mixed("aZ\0mIX\xc3\xa9\xff", 9);
    require(sash::toupper(mixed) == std::string("AZ\0MIX\xc3\xa9\xff", 9) &&
                sash::tolower(mixed) == std::string("az\0mix\xc3\xa9\xff", 9),
            "case conversion must map ASCII only and preserve counted NUL/UTF-8/high bytes");
    require(sash::toupper("@AZ[`az{") == "@AZ[`AZ{" && sash::tolower("@AZ[`az{") == "@az[`az{" &&
                sash::toupper({}).empty() && sash::tolower({}).empty(),
            "ASCII case bounds must leave neighboring punctuation and empty values unchanged");
    std::string source = "MiXeD";
    const auto upper = sash::toupper(source);
    source = sash::tolower(source);
    require(upper == "MIXED" && source == "mixed",
            "case results must own their bytes and permit source-target aliasing");
    sash::Array<std::string> array{{2, "MiXeD"}};
    sash::Map<std::string> map{{"word", "MiXeD"}};
    require(sash::toupper(sash::cell(array, 2)) == "MIXED" &&
                sash::tolower(sash::cell(map, "word")) == "mixed" &&
                sash::toupper(sash::cell(map, "missing")).empty() && sash::len(map) == 1,
            "case conversion must share scalar and non-inserting string-cell semantics");
    const std::string large(256, 'q');
    const std::function<std::string()> operations[] = {
        [&] { return sash::substring(large, 2, 200); },
        [&] { return sash::toupper(large); },
        [&] { return sash::tolower(large); },
    };
    for (const auto &operation : operations) {
        std::string target = "kept";
        bool caught = false;
        try {
            const AllocationFailure fault(128);
            target = operation();
        } catch (const std::bad_alloc &) {
            caught = true;
        }
        require(
            caught && target == "kept",
            "slice/case allocation failure must escape without publishing an empty replacement");
    }
}

void text_predicates() {
    require(sash::is_print("!Hello~") && sash::is_graph("!Hello~") && sash::is_print("two words") &&
                !sash::is_graph("two words") && sash::is_print(" ") && !sash::is_graph(" "),
            "ASCII print includes space while graph excludes it, with inclusive endpoints");
    const std::string_view rejected[] = {
        {}, "", "\t", "\n", "\x1f", "\x7f", "\x80", "\xc3\xa9", std::string_view("ok\0end", 6),
    };
    for (const auto text : rejected) {
        require(!sash::is_print(text) && !sash::is_graph(text),
                "ASCII shape tests require nonempty input and reject controls/NUL/high bytes");
    }
    sash::Array<std::string> array{{0, "two words"}};
    sash::Map<std::string> map{{"word", "Hello"}};
    require(sash::is_print(sash::cell(array, 0)) && !sash::is_graph(sash::cell(array, 0)) &&
                sash::is_graph(sash::cell(map, "word")) &&
                !sash::is_print(sash::cell(map, "missing")) && sash::len(map) == 1,
            "ASCII predicates must share scalar and non-inserting string-cell semantics");
    bool denied = false;
    try {
        const AllocationFailure fault(1);
        const std::string control(128, 'x');
        require(control.size() == 128, "text allocation control");
    } catch (const std::bad_alloc &) {
        denied = true;
    }
    require(denied, "text allocation control must actually deny allocation");
    {
        const AllocationFailure fault(1);
        require(sash::strlen("Hello") == 5 && sash::is_print("Hello world") &&
                    sash::is_graph("Hello") && !sash::is_print(""),
                "byte length and ASCII predicates must not allocate");
    }
}

void regex_values() {
    const auto assignment = sash::compile_regex("[a-z]+=[0-9]+");
    std::string subject = "prefix x=42 suffix";
    require(sash::match(subject, assignment) && !sash::match("unassigned", assignment),
            "regex match must search rather than require the entire subject");
    const auto owned = sash::match_text(subject, assignment);
    subject.assign("changed");
    require(owned == "x=42" && sash::match_text("unassigned", assignment).empty(),
            "whole-match extraction owns its bytes and returns empty on a normal miss");
    const auto empty = sash::compile_regex("");
    require(sash::match({}, empty) && sash::match("abc", empty) &&
                sash::match_text({}, empty).empty(),
            "an empty match is success even though its whole-match value is empty");
    const std::string outside = "x=42 outside";
    const std::string_view prefix(outside.data(), 4);
    require(sash::match_text(prefix, assignment) == "x=42" &&
                !sash::match(prefix, sash::compile_regex("outside")),
            "regex subjects use counted ranges, not trailing bytes of partial views");
    const std::string binary("a\0b\xff", 4);
    const auto through_nul = sash::compile_regex("a(.)b");
    require(sash::match(binary, through_nul) &&
                sash::match_text(binary, through_nul) == std::string("a\0b", 3) &&
                sash::match(binary, sash::compile_regex("b")) &&
                sash::match_text(binary, sash::compile_regex(std::string_view("\xff", 1))) ==
                    std::string("\xff", 1),
            "regex search and extraction must inspect counted NUL and high bytes");
    require(sash::match(binary, sash::compile_regex(R"(a\x00b)")),
            "an escaped NUL pattern must match the counted NUL in its subject");
    sash::Array<std::string> array{{0, "x=42"}};
    sash::Map<std::string> map{{"text", "x=42"}};
    require(sash::match(sash::cell(array, 0), assignment) &&
                sash::match_text(sash::cell(map, "text"), assignment) == "x=42" &&
                !sash::match(sash::cell(map, "missing"), assignment) && sash::len(map) == 1,
            "regex value helpers share scalar and non-inserting string-cell behavior");
}

void regex_captures() {
    const auto assignment = sash::compile_regex("([a-z]+)=([0-9]+)");
    sash::Array<std::string> captures{{99, "stale"}};
    require(sash::match("prefix x=42 suffix", assignment, captures) &&
                captures == sash::Array<std::string>{{0, "x=42"}, {1, "x"}, {2, "42"}},
            "single captures must replace the target with dense whole-plus-group values");
    require(!sash::match("unassigned", assignment, captures) && captures.empty(),
            "a normal single-match miss clears the previous target");
    require(sash::match({}, sash::compile_regex("(a)?(b*)"), captures) &&
                captures == sash::Array<std::string>{{0, ""}, {1, ""}, {2, ""}},
            "unmatched and participating-empty groups occupy their own empty capture cells");
    require(sash::match({}, sash::compile_regex(""), captures) &&
                captures == sash::Array<std::string>{{0, ""}},
            "a group-free empty single match stores one empty whole match");
    const auto many = sash::compile_regex("(.)(.)(.)(.)(.)(.)(.)(.)(.)(.)(.)(.)");
    require(sash::match("abcdefghijkl", many, captures) && sash::len(captures) == 13 &&
                sash::cell(captures, 0) == "abcdefghijkl" && sash::cell(captures, 12) == "l",
            "single extraction must retain every group, including groups beyond nine");
    captures = {{99, "x=42"}};
    const std::string_view borrowed = captures.at(99);
    require(sash::match(borrowed, assignment, captures) &&
                captures == sash::Array<std::string>{{0, "x=42"}, {1, "x"}, {2, "42"}},
            "single extraction must finish reading a target-borrowed subject before replacement");
    std::string binary("a\0b", 3);
    require(sash::match(binary, sash::compile_regex("a(.)b"), captures), "binary capture setup");
    binary[1] = 'X';
    require(sash::cell(captures, 0) == std::string("a\0b", 3) &&
                sash::cell(captures, 1) == std::string("\0", 1),
            "capture cells must own interior NUL independently of the original subject");
}

void regex_repeated() {
    sash::Array<std::string> captures{{99, "stale"}};
    const auto assignments = sash::compile_regex("([a-z]+)=([^;]*)");
    require(
        sash::matchall("a=1;b=;c=3", assignments, captures) &&
            captures ==
                sash::Array<std::string>{{0, "a"}, {1, "1"}, {2, "b"}, {3, ""}, {4, "c"}, {5, "3"}},
        "repeated extraction flattens groups only and retains empty values");
    require(sash::matchall("one two", sash::compile_regex("[a-z]+"), captures) &&
                captures == sash::Array<std::string>{{0, "one"}, {1, "two"}},
            "group-free repeated extraction stores each whole match");
    require(sash::matchall("ab", sash::compile_regex("(a)|(b)"), captures) &&
                captures == sash::Array<std::string>{{0, "a"}, {1, ""}, {2, ""}, {3, "b"}},
            "repeated alternatives retain nonparticipating group positions");
    require(sash::matchall("ab", sash::compile_regex("^"), captures) &&
                captures == sash::Array<std::string>{{0, ""}},
            "standard iteration preserves the original beginning rather than reanchoring suffixes");
    require(
        sash::matchall("a", sash::compile_regex("(|a)"), captures) &&
            captures == sash::Array<std::string>{{0, ""}, {1, "a"}, {2, ""}},
        "standard iteration retries a nonempty match at the same position after an empty match");
    require(sash::matchall("ab", sash::compile_regex(""), captures) &&
                captures == sash::Array<std::string>{{0, ""}, {1, ""}, {2, ""}} &&
                sash::matchall({}, sash::compile_regex(""), captures) &&
                captures == sash::Array<std::string>{{0, ""}},
            "empty-pattern iteration advances and includes the final boundary once");
    require(sash::matchall("ab", sash::compile_regex("$"), captures) &&
                captures == sash::Array<std::string>{{0, ""}},
            "end-anchor iteration terminates after the final empty match");
    require(!sash::matchall("ab", sash::compile_regex("z"), captures) && captures.empty(),
            "a normal repeated-match miss clears the previous target");
    const std::string binary("a\0b", 3);
    require(sash::matchall(binary, sash::compile_regex("."), captures) &&
                captures == sash::Array<std::string>{{0, "a"}, {1, std::string("\0", 1)}, {2, "b"}},
            "standard repeated matching includes and continues past interior NUL");
    captures = {{99, "a=1;b=2"}};
    const std::string_view borrowed = captures.at(99);
    require(sash::matchall(borrowed, assignments, captures) &&
                captures == sash::Array<std::string>{{0, "a"}, {1, "1"}, {2, "b"}, {3, "2"}},
            "repeated extraction owns every value before replacing a borrowed input target");
}

void regex_publication() {
    const std::string_view nul_patterns[] = {
        std::string_view("\0", 1),
        std::string_view("a\0b", 3),
        std::string_view("[\0]", 3),
    };
    for (const auto pattern : nul_patterns) {
        bool rejected = false;
        try {
            (void)sash::compile_regex(pattern);
        } catch (const sash::CommandError &error) {
            rejected = error.status() == 2 && std::string_view(error.what()) ==
                                                  R"(regex: literal NUL in pattern; use \x00)";
        }
        require(rejected,
                "literal NUL patterns must fail explicitly rather than compile incorrectly");
    }
    bool invalid = false;
    try {
        (void)sash::compile_regex("[");
    } catch (const sash::CommandError &error) {
        invalid = error.status() == 2 && std::string_view(error.what()).substr(0, 7) == "regex: ";
    }
    require(invalid, "invalid regex is a catchable status-2 error, not a normal non-match");
    const auto word = sash::compile_regex("([a-z]+)");
    const std::string large(1024, 'x');
    const std::string multiple = "tiny " + large;
    const auto copy_size = large.capacity() + 1;
    {
        const AllocationFailure fault(copy_size, AllocationFailureMode::Exact);
        require(sash::match(large, word),
                "allocation control must permit regex searching before denying owned copies");
        std::size_t count = 0;
        std::size_t bytes = 0;
        const std::regex_iterator<const char *> end;
        for (std::regex_iterator<const char *> it(multiple.data(),
                                                  multiple.data() + multiple.size(), word);
             it != end; ++it) {
            ++count;
            bytes += static_cast<std::size_t>(it->length());
        }
        require(count == 2 && bytes == multiple.size() - 1,
                "allocation control must permit every repeated search before denying owned copies");
    }
    bool copy_failed = false;
    try {
        const AllocationFailure fault(copy_size, AllocationFailureMode::Exact);
        (void)sash::match_text(large, word);
    } catch (const std::bad_alloc &) {
        copy_failed = true;
    }
    require(copy_failed, "owned whole-match allocation must fail visibly under the copy fault");
    const sash::Array<std::string> original{{99, "kept"}};
    sash::Array<std::string> target = original;
    bool single_failed = false;
    try {
        const AllocationFailure fault(copy_size, AllocationFailureMode::Exact);
        (void)sash::match(large, word, target);
    } catch (const std::bad_alloc &) {
        single_failed = true;
    }
    require(single_failed && target == original,
            "single capture allocation failure preserves the previous target");
    bool repeated_failed = false;
    try {
        const AllocationFailure fault(copy_size, AllocationFailureMode::Exact);
        (void)sash::matchall(multiple, word, target);
    } catch (const std::bad_alloc &) {
        repeated_failed = true;
    }
    require(repeated_failed && target == original,
            "later repeated capture allocation failure publishes no partial replacement");
    bool search_failed = false;
    try {
        const AllocationFailure fault(1);
        (void)sash::matchall("tiny", word, target);
    } catch (const std::bad_alloc &) {
        search_failed = true;
    }
    require(search_failed && target == original,
            "regex search allocation failure escapes instead of becoming false or clearing output");
}

void replacement_values() {
    const auto assignment = sash::compile_regex("([a-z])=([0-9])");
    std::string subject = "pre a=1;b=2 end";
    require(
        sash::sub(subject, assignment, "$2:$1") == "pre 1:a;b=2 end" &&
            sash::gsub(subject, assignment, "$2:$1") == "pre 1:a;2:b end",
        "sub replaces only the first match while gsub replaces all and preserves surrounding text");
    const auto unchanged = sash::gsub(subject, sash::compile_regex("NONE"), "unused");
    const auto owned = sash::sub(subject, assignment, "$2:$1");
    subject.assign("changed");
    require(unchanged == "pre a=1;b=2 end" && owned == "pre 1:a;b=2 end",
            "replacement and normal-miss results own bytes independently of the subject");
    require(sash::gsub("a1b2c", sash::compile_regex("[0-9]"), {}) == "abc" &&
                sash::sub({}, assignment, "$1").empty(),
            "empty replacement deletes matches and default-empty subjects remain valid");
    const std::string source = "a=1 outside";
    const std::string format = "$2:$1 outside";
    require(sash::sub(std::string_view(source.data(), 3), assignment,
                      std::string_view(format.data(), 5)) == "1:a",
            "replacement subjects and templates honor counted partial views");
    const std::string binary("a\0b\xff", 4);
    const std::string binary_format("X\0$1Y", 5);
    require(sash::sub(binary, sash::compile_regex("(b)"), binary_format) ==
                    std::string("a\0X\0bY\xff", 7) &&
                sash::gsub(binary, sash::compile_regex("a(.)b"), "$1") == std::string("\0\xff", 2),
            "replacement searches and formats retain counted NUL and high bytes");
    std::string target = "a=1";
    const std::string_view borrowed_subject = target;
    target = sash::sub(borrowed_subject, assignment, "$2:$1");
    require(target == "1:a", "replacement consumes a target-borrowed subject before assignment");
    target = "$&!";
    const std::string_view borrowed_format = target;
    target = sash::gsub("a a", sash::compile_regex("a"), borrowed_format);
    require(target == "a! a!", "replacement consumes a target-borrowed template before assignment");
    const sash::Array<std::string> array{{0, "a=1"}};
    const sash::Map<std::string> map{{"text", "a=1"}};
    require(sash::sub(sash::cell(array, 0), assignment, "$2:$1") == "1:a" &&
                sash::gsub(sash::cell(map, "text"), assignment, "$2:$1") == "1:a" &&
                sash::gsub(sash::cell(map, "missing"), assignment, "$1").empty() &&
                sash::len(map) == 1,
            "replacement helpers share scalar and non-inserting string-cell behavior");
}

void replacement_templates() {
    const auto assignment = sash::compile_regex("([a-z])=([0-9])");
    require(sash::sub("a=1", assignment, R"([$&][$2][$1][$$][\1])") == R"([a=1][1][a][$][\1])",
            "C++ templates expand whole/groups and dollars while backslashes remain literal");
    std::string computed = "$";
    computed += "2:$1";
    require(sash::gsub("a=1 b=2", assignment, computed) == "1:a 2:b",
            "evaluated replacement strings use the same standard template notation");
    require(sash::sub("b", sash::compile_regex("(a)?(b)"), "$1/$2/$9") == "/b/",
            "nonparticipating and absent standard replacement groups contribute empty strings");
    const auto many = sash::compile_regex("(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)(k)(l)");
    require(sash::sub("abcdefghijkl", many, "$12/$1") == "l/a",
            "standard two-digit replacement references retain groups beyond nine");
    require(sash::sub("a", sash::compile_regex("a"), R"($$:$x:$:\q)") == R"($:$x:$:\q)",
            "standard literal dollar and backslash text is not rejected by native template rules");
    require(sash::sub("preaafter", sash::compile_regex("a"), "$`<$&>$'") == "prepre<a>afterafter" &&
                sash::gsub("aba", sash::compile_regex("a"), "[$`][$']") == "[][ba]b[b][]",
            "prefix and suffix expansions use the standard per-match context");
    const auto empty = sash::compile_regex("");
    require(sash::sub("ab", empty, "X") == "Xab" && sash::gsub("ab", empty, "X") == "XaXbX" &&
                sash::gsub({}, empty, "X") == "X" && sash::gsub("ab", empty, {}) == "ab",
            "first and global empty replacements retain standard progress and the final boundary");
    require(sash::gsub("ab", sash::compile_regex("^"), "X") == "Xab" &&
                sash::gsub("ab", sash::compile_regex("$"), "X") == "abX",
            "global replacement preserves original anchor context and terminates at the end");
    const auto alternatives = sash::compile_regex("(|a)");
    require(
        sash::sub("a", alternatives, "[$1]") == "[]a" &&
            sash::gsub("a", alternatives, "[$1]") == "[][a][]",
        "global replacement retries a nonempty match after an empty one but sub stops at first");
}

void replacement_failures() {
    const auto literal = sash::compile_regex("x");
    const std::string subject = "lead:" + std::string(1024, 'x');
    const auto first = sash::sub(subject, literal, "y");
    const auto all = sash::gsub(subject, literal, "y");
    require(first == "lead:y" + std::string(1023, 'x') && all == "lead:" + std::string(1024, 'y'),
            "replacement allocation calibration has independently expected first/global results");
    using ReplaceFunction = std::string (*)(std::string_view, const std::regex &, std::string_view);
    const auto output_failure = [&](ReplaceFunction replace, const std::string &completed) {
        const auto size = completed.capacity() + 1;
        {
            const AllocationFailure fault(size, AllocationFailureMode::Exact);
            std::size_t count = 0;
            const std::regex_iterator<const char *> end;
            for (std::regex_iterator<const char *> it(subject.data(),
                                                      subject.data() + subject.size(), literal);
                 it != end; ++it) {
                ++count;
            }
            require(count == 1024,
                    "output fault control must permit the complete replacement search walk");
        }
        std::string target = "kept";
        bool failed = false;
        try {
            const AllocationFailure fault(size, AllocationFailureMode::Exact);
            target = replace(subject, literal, "y");
        } catch (const std::bad_alloc &) {
            failed = true;
        }
        require(failed && target == "kept",
                "replacement output allocation failure preserves the caller assignment");
    };
    output_failure(sash::sub, first);
    output_failure(sash::gsub, all);
    const auto word = sash::compile_regex("([a-z]+)");
    const std::string format(1024, 'R');
    const auto expanded = sash::gsub("tiny", word, format);
    require(expanded == format, "a long counted template produces independently expected output");
    const auto format_size = expanded.capacity() + 1;
    {
        const AllocationFailure fault(format_size, AllocationFailureMode::Exact);
        require(sash::match("tiny", word),
                "template-output fault control must permit the intended regex search");
    }
    std::string target = "kept";
    bool format_failed = false;
    try {
        const AllocationFailure fault(format_size, AllocationFailureMode::Exact);
        target = sash::gsub("tiny", word, format);
    } catch (const std::bad_alloc &) {
        format_failed = true;
    }
    require(format_failed && target == "kept",
            "long-template output allocation failure escapes without an empty replacement");
    bool search_failed = false;
    try {
        const AllocationFailure fault(1);
        target = sash::sub("tiny", word, "R");
    } catch (const std::bad_alloc &) {
        search_failed = true;
    }
    require(search_failed && target == "kept",
            "replacement search allocation failure is not a success-shaped unchanged result");
}

void nul_read_values() {
    using Records = sash::Array<std::string>;
    require(sash::read(std::string_view{}).empty(), "empty NUL input produces no records");
    require(sash::read(std::string_view("\0\0", 2)) == Records{{0, ""}, {1, ""}},
            "each NUL terminates a real record, including empty records");
    require(sash::read(std::string_view("a\0", 2)) == Records{{0, "a"}} &&
                sash::read(std::string_view("a\0\0", 3)) == Records{{0, "a"}, {1, ""}} &&
                sash::read("tail") == Records{{0, "tail"}},
            "terminal NUL adds no synthetic record and an unterminated tail survives");
    require(sash::read(std::string_view("\0Ada\0\0Grace\0", 12)) ==
                Records{{0, ""}, {1, "Ada"}, {2, ""}, {3, "Grace"}},
            "leading and interior empty NUL records retain dense zero-based indices");
    std::string bytes("\xff\r\n\0z", 5);
    const auto owned = sash::read(bytes);
    bytes.assign("changed");
    require(owned == Records{{0, "\xff\r\n"}, {1, "z"}} &&
                sash::read(" \t\r\n ") == Records{{0, " \t\r\n "}},
            "NUL reads own high bytes and preserve newline, CR and whitespace");
    const std::string backing("a\0b ignored", 11);
    require(sash::read(std::string_view(backing.data(), 3)) == Records{{0, "a"}, {1, "b"}},
            "NUL splitting respects the counted view, not backing storage or a C-string end");
    Records target{{8, std::string("a\0b", 3)}};
    target = sash::read(std::string_view(target.at(8)));
    require(target == Records{{0, "a"}, {1, "b"}},
            "a target-borrowed cell remains valid until the owned read completes");
    const sash::Map<std::string> cells{{"records", std::string("a\0b", 3)}};
    require(sash::read(sash::cell(cells, "records")) == target &&
                sash::read(sash::cell(cells, "missing")).empty() && cells.size() == 1,
            "scalar and string-cell reads share NUL semantics without inserting a missing cell");
    const auto encoded = sash::gsub("a||b|", sash::compile_regex("[|]"), std::string_view("\0", 1));
    require(sash::read(encoded) == Records{{0, "a"}, {1, ""}, {2, "b"}},
            "upstream counted replacement states the separator before NUL splitting");
    target[27] = "stale";
    target = sash::read("only");
    require(target == Records{{0, "only"}}, "shorter NUL input replaces every old array cell");
    target = sash::read(std::string_view{});
    require(target.empty(), "empty NUL input clears a populated assignment target");
}

void nul_read_streams(const std::string &helper) {
    using Records = sash::Array<std::string>;
    Records target{{9, "old"}};
    sash::read_from(helper, {"args", "", "Ada", "", "Grace", ""}, [&](sash::LineReader &input) {
        target = input.read();
        require(target == Records{{0, ""}, {1, "Ada"}, {2, ""}, {3, "Grace"}, {4, ""}},
                "stream NUL reads retain real empty records, not a final sentinel");
        target = input.read();
        require(target.empty(), "repeated bulk EOF clears the assignment target");
        std::string line = "old";
        require(!input.readline(line) && line.empty(),
                "a bulk read leaves the shared line reader at EOF");
    });
    sash::read_from(helper, {"records", "0"}, [&](sash::LineReader &input) {
        std::string header;
        require(input.readline(header) && header == "alpha", "read the line before bulk input");
        target = input.read();
    });
    require(target == Records{{0, "\nnul"}, {1, "inside\r\nlast"}},
            "bulk read consumes the line reader's prefetched tail before another fd read");
    sash::read_from(helper, {"binary"}, [&](sash::LineReader &input) { target = input.read(); });
    require(target == Records{{0, std::string(256 * 1024, 'x')}, {1, "end\n\n"}},
            "bulk read spans buffers and pipe capacity without treating newlines as separators");
    sash::read_from(helper, {"exit", "0"}, [&](sash::LineReader &input) { target = input.read(); });
    require(target.empty(), "empty producer output is an empty array, not an empty record");
    in_child([&] {
        install_stdin(std::string_view("Ada\0\0Grace\0", 11));
        const int flags = ::fcntl(0, F_GETFD);
        require(flags >= 0, "inspect borrowed stdin");
        require(sash::read() == Records{{0, "Ada"}, {1, ""}, {2, "Grace"}},
                "default NUL read consumes stdin");
        require(::fcntl(0, F_GETFD) == flags && sash::read(0).empty(),
                "explicit fd NUL read preserves the borrowed descriptor and its EOF");
    });
}

void nul_read_failures() {
    using Records = sash::Array<std::string>;
    const std::string long_record(1024, 'x');
    const std::string source = std::string("tiny\0", 5) + long_record + std::string("\0last", 5);
    const auto healthy = sash::read(source);
    require(healthy == Records{{0, "tiny"}, {1, long_record}, {2, "last"}},
            "NUL field-copy allocation calibration has independently expected records");
    const auto size = healthy.at(1).capacity() + 1;
    {
        const AllocationFailure fault(size, AllocationFailureMode::Exact);
        require(sash::read(std::string_view("a\0b", 3)) == Records{{0, "a"}, {1, "b"}},
                "field-copy fault control permits complete small-record splitting");
    }
    Records target{{9, "kept"}};
    bool failed = false;
    try {
        const AllocationFailure fault(size, AllocationFailureMode::Exact);
        target = sash::read(source);
    } catch (const std::bad_alloc &) {
        failed = true;
    }
    require(failed && target == Records{{9, "kept"}},
            "later field allocation failure preserves the caller's entire old array");
    bool invalid = false;
    try {
        target = sash::read(-1);
    } catch (const sash::SetupError &error) {
        invalid = error.code().value() == EBADF &&
                  std::string_view(error.what()).find("read NUL records") != std::string_view::npos;
    }
    require(invalid && target == Records{{9, "kept"}},
            "invalid borrowed fd is a labeled read error, not empty input");
    in_child([&] {
        int channel[2];
        require(::pipe(channel) == 0, "create post-prefix read-error pipe");
        const int flags = ::fcntl(channel[0], F_GETFL);
        require(flags >= 0 && ::fcntl(channel[0], F_SETFL, flags | O_NONBLOCK) == 0,
                "make the test reader nonblocking");
        sash::write(std::string_view("a\0b", 3), channel[1]);
        Records old{{9, "kept"}};
        bool read_failed = false;
        try {
            old = sash::read(channel[0]);
        } catch (const sash::SetupError &error) {
            read_failed =
                error.code().value() == EAGAIN &&
                std::string_view(error.what()).find("read NUL records") != std::string_view::npos;
        }
        require(read_failed && old == Records{{9, "kept"}},
                "I/O failure after a consumed prefix cannot publish partial records");
        char byte;
        require(::read(channel[0], &byte, 1) < 0 && errno == EAGAIN &&
                    ::fcntl(channel[0], F_GETFL) == (flags | O_NONBLOCK),
                "post-prefix error consumed the bytes without closing or reflagging the fd");
        require(::close(channel[1]) == 0, "close post-prefix writer");
        old = sash::read(channel[0]);
        require(old.empty() && ::fcntl(channel[0], F_GETFL) == (flags | O_NONBLOCK),
                "normal EOF after a read error clears the target and still borrows the fd");
        require(::close(channel[0]) == 0, "close post-prefix reader");
    });
}

void nul_read_ownership(const std::string &helper) {
    using Records = sash::Array<std::string>;
    Records target;
    sash::read_pipeline({{helper, {"args", "Ada", "", "Grace"}}, {helper, {"relay"}}},
                        [&](sash::LineReader &input) { target = input.read(); });
    require(target == Records{{0, "Ada"}, {1, ""}, {2, "Grace"}},
            "pipeline bulk reads return owned exact NUL records");
    const auto result = sash::try_read_from(
        helper, {"records", "7"}, [&](sash::LineReader &input) { target = input.read(); });
    const auto *exit = std::get_if<sash::ProcessExit>(&result.outcome);
    const Records expected{{0, "alpha\n\nnul"}, {1, "inside\r\nlast"}};
    require(exit != nullptr && exit->status == 7 && target == expected,
            "producer failure retains its verdict and already-published bulk records");
    bool checked = false;
    try {
        sash::check(result);
    } catch (const sash::CommandError &error) {
        checked = error.status() == 7;
    }
    require(checked && target == expected,
            "checking a failed producer does not roll back completed consumer assignment");
    in_child([&] {
        default_signal(SIGPIPE);
        Records old{{9, "kept"}};
        bool failed = false;
        try {
            (void)sash::try_read_from(helper, {"binary"}, [&](sash::LineReader &input) {
                const AllocationFailure fault(8192);
                old = input.read();
            });
        } catch (const std::bad_alloc &) {
            failed = true;
        }
        require(failed && old == Records{{9, "kept"}},
                "bulk collector allocation failure escapes without a partial assignment");
        int status;
        require(::waitpid(-1, &status, WNOHANG) < 0 && errno == ECHILD,
                "bulk collector unwinding closes the pipe before reaping its producer");
    });
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
        if (mode == "--nul-read") {
            return run_test(argv[1],
                            [&] {
                                nul_read_values();
                                nul_read_streams(helper);
                                nul_read_failures();
                                nul_read_ownership(helper);
                            })
                       ? 0
                       : 1;
        }
        if (mode == "--collections") {
            return run_test(argv[1],
                            [&] {
                                collection_cells();
                                collection_traversal();
                                collection_values();
                                collection_dispositions();
                                collection_arguments(helper);
                                collection_allocation();
                            })
                       ? 0
                       : 1;
        }
        if (mode == "--arithmetic") {
            return run_test(argv[1],
                            [] {
                                arithmetic_values();
                                arithmetic_shifts();
                                arithmetic_failures();
                            })
                       ? 0
                       : 1;
        }
        if (mode == "--numeric") {
            return run_test(argv[1],
                            [] {
                                numeric_parsing();
                                program_arguments();
                                environment_values();
                                numeric_dispositions();
                            })
                       ? 0
                       : 1;
        }
        if (mode == "--text") {
            return run_test(argv[1],
                            [] {
                                byte_substrings();
                                ascii_text();
                                text_predicates();
                            })
                       ? 0
                       : 1;
        }
        if (mode == "--regex") {
            return run_test(argv[1],
                            [] {
                                regex_values();
                                regex_captures();
                                regex_repeated();
                                regex_publication();
                            })
                       ? 0
                       : 1;
        }
        if (mode == "--replacement") {
            return run_test(argv[1],
                            [] {
                                replacement_values();
                                replacement_templates();
                                replacement_failures();
                            })
                       ? 0
                       : 1;
        }
        if (mode == "--capture-storage") {
            return run_test(argv[1], [&] { capture_storage_contract(helper); }) ? 0 : 1;
        }
        if (mode == "--process-descriptors") {
            return run_test(argv[1], [&] { process_descriptors(helper); }) ? 0 : 1;
        }
        if (mode == "--fork-failure") {
            return run_test(argv[1], [&] { fork_failure(helper); }) ? 0 : 1;
        }
        if (mode == "--wait-interrupted") {
            return run_test(argv[1], [&] { interrupted_wait(helper); }) ? 0 : 1;
        }
        if (mode == "--lifecycle") {
            return run_test(argv[1],
                            [&] {
                                lifecycle_completion();
                                lifecycle_failures();
                                lifecycle_scopes(helper);
                                lifecycle_signals();
                                lifecycle_signal_scopes();
                                lifecycle_foreground_signals(helper);
                                lifecycle_main_only();
                            })
                       ? 0
                       : 1;
        }
        if (mode == "--signal-install-failure") {
            return run_test(argv[1], lifecycle_signal_install_failure) ? 0 : 1;
        }
        if (mode == "--signal-END-install-failure" || mode == "--signal-END-mask-failure") {
            return run_test(
                       argv[1],
                       [&] { lifecycle_signal_end_failure(mode == "--signal-END-mask-failure"); })
                       ? 0
                       : 1;
        }
        if (mode == "--signal-restore-failure") {
            return run_test(argv[1], lifecycle_signal_restore_failure) ? 0 : 1;
        }
        if (mode == "--lifecycle-handoff-failure" ||
            mode == "--lifecycle-restore-handoff-failure") {
            return run_test(argv[1], lifecycle_handoff_failure) ? 0 : 1;
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
        {"capture storage contract", [&] { capture_storage_contract(helper); }},
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
        {"lifecycle completion and explicit exit", lifecycle_completion},
        {"explicit END handling and ordinary failures", lifecycle_failures},
        {"lifecycle reader and capture unwinding", [&] { lifecycle_scopes(helper); }},
        {"FLAG and EXIT identity", lifecycle_signals},
        {"signal registration lifetime", lifecycle_signal_scopes},
        {"foreground and captured pipeline signals", [&] { lifecycle_foreground_signals(helper); }},
        {"main-only END", lifecycle_main_only},
        {"collection cells and aliases", collection_cells},
        {"live sorted collection traversal", collection_traversal},
        {"collection joins, snapshots and pairs", collection_values},
        {"lazy collection dispositions", collection_dispositions},
        {"owned collection arguments", [&] { collection_arguments(helper); }},
        {"collection allocation failures", collection_allocation},
        {"checked integer arithmetic", arithmetic_values},
        {"defined integer shifts", arithmetic_shifts},
        {"arithmetic failure boundaries", arithmetic_failures},
        {"strict integer input", numeric_parsing},
        {"owned program arguments", program_arguments},
        {"environment lookup values", environment_values},
        {"numeric conversion boundaries", numeric_dispositions},
        {"counted byte substrings", byte_substrings},
        {"owned ASCII case conversion", ascii_text},
        {"nonempty ASCII shape predicates", text_predicates},
        {"owned regex search and whole matches", regex_values},
        {"single regex capture arrays", regex_captures},
        {"standard repeated regex extraction", regex_repeated},
        {"regex failure publication boundaries", regex_publication},
        {"owned first and global replacements", replacement_values},
        {"standard C++ replacement templates", replacement_templates},
        {"replacement allocation boundaries", replacement_failures},
        {"owned counted NUL records", nul_read_values},
        {"buffered and borrowed-fd NUL reads", [&] { nul_read_streams(helper); }},
        {"NUL read failure publication", nul_read_failures},
        {"NUL reader producer ownership", [&] { nul_read_ownership(helper); }},
    };
    for (const auto &test : tests) {
        if (!run_test(test.first, test.second)) {
            return 1;
        }
    }
}
