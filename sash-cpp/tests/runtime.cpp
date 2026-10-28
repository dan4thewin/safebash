#include "sash/runtime.hpp"

#include <cerrno>
#include <cstdio>
#include <cstdlib>
#include <fcntl.h>
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

} // namespace

int main() {
    const std::pair<const char *, void (*)()> tests[] = {
        {"command outcomes", command_outcomes},
        {"program boundary", program_boundary},
        {"binary and large capture", binary_and_large_capture},
        {"echo empty words", echo_empty_words},
        {"nested capture and unwind", nested_capture_and_unwind},
        {"restored stdout", restored_stdout},
        {"allocation failure and fd lifetime", allocation_failure_and_fd_lifetime},
        {"ECMAScript regex", ecmascript_regex},
    };
    for (const auto &test : tests) {
        try {
            test.second();
        } catch (const std::exception &error) {
            std::fprintf(stderr, "FAIL %s: %s\n", test.first, error.what());
            return 1;
        }
        std::fprintf(stderr, "ok %s\n", test.first);
    }
}
