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

void require(bool condition, const char *message) {
    if (!condition) {
        throw std::runtime_error(message);
    }
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
        std::vector<std::string> arguments{"fds"};
        for (int fd = 3; fd < 128; ++fd) {
            const int flags = ::fcntl(fd, F_GETFD);
            if (flags >= 0 && (flags & FD_CLOEXEC) == 0) {
                arguments.push_back(std::to_string(fd));
            } else if (flags < 0) {
                require(errno == EBADF, "inspect inherited descriptors");
            }
        }
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
        if (mode == "--fork-failure") {
            return run_test(argv[1], [&] { fork_failure(helper); }) ? 0 : 1;
        }
        if (mode == "--wait-interrupted") {
            return run_test(argv[1], [&] { interrupted_wait(helper); }) ? 0 : 1;
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
    };
    for (const auto &test : tests) {
        if (!run_test(test.first, test.second)) {
            return 1;
        }
    }
}
