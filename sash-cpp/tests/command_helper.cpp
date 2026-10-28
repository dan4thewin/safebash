#include <array>
#include <cerrno>
#include <charconv>
#include <csignal>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fcntl.h>
#include <string_view>

namespace {

int fail(const char *message) {
    std::fprintf(stderr, "command-helper: %s\n", message);
    return 2;
}

bool number(const char *text, int &value) {
    const auto *end = text + std::strlen(text);
    const auto result = std::from_chars(text, end, value);
    return result.ec == std::errc{} && result.ptr == end;
}

bool output(const char *data, std::size_t size) {
    return std::fwrite(data, 1, size, stdout) == size;
}

int flushed() {
    if (std::fflush(stdout) != 0) {
        std::perror("command-helper: flush stdout");
        return 2;
    }
    return 0;
}

} // namespace

int main(int argc, char **argv) {
    if (argc < 2) {
        return fail("expected a mode");
    }
    const std::string_view mode(argv[1]);
    if (mode == "exit" && argc == 3) {
        int status;
        if (!number(argv[2], status) || status < 0 || status > 255) {
            return fail("invalid exit status");
        }
        return status;
    }
    if (mode == "args") {
        for (int i = 2; i < argc; ++i) {
            if (!output(argv[i], std::strlen(argv[i])) || std::fputc('\0', stdout) == EOF) {
                return fail("cannot write arguments");
            }
        }
        return flushed();
    }
    if (mode == "binary" && argc == 2) {
        std::array<char, 8192> block{};
        block.fill('x');
        for (int i = 0; i < 32; ++i) {
            if (!output(block.data(), block.size())) {
                return fail("cannot write binary output");
            }
        }
        const char tail[] = "\0end\n\n";
        if (!output(tail, sizeof(tail) - 1)) {
            return fail("cannot write binary tail");
        }
        return flushed();
    }
    if (mode == "env" && argc == 3) {
        const auto *value = std::getenv(argv[2]);
        if (value == nullptr) {
            return 1;
        }
        if (!output(value, std::strlen(value))) {
            return fail("cannot write environment value");
        }
        return flushed();
    }
    if (mode == "signal" && argc == 3) {
        int signal;
        if (!number(argv[2], signal) || signal <= 0 || std::signal(signal, SIG_DFL) == SIG_ERR) {
            return fail("invalid signal");
        }
        sigset_t unblocked;
        if (::sigemptyset(&unblocked) != 0 || ::sigaddset(&unblocked, signal) != 0 ||
            ::sigprocmask(SIG_UNBLOCK, &unblocked, nullptr) != 0 || std::raise(signal) != 0) {
            return fail("cannot raise signal");
        }
        return fail("signal did not terminate the helper");
    }
    if (mode == "closed" && argc == 3) {
        int fd;
        if (!number(argv[2], fd) || fd < 0) {
            return fail("invalid descriptor");
        }
        if (::fcntl(fd, F_GETFD) < 0 && errno == EBADF) {
            return 0;
        }
        return fail("descriptor should be closed");
    }
    if (mode == "fds") {
        std::array<bool, 128> allowed{};
        for (int i = 2; i < argc; ++i) {
            int fd;
            if (!number(argv[i], fd) || fd < 3 || fd >= static_cast<int>(allowed.size())) {
                return fail("invalid inherited descriptor");
            }
            allowed[fd] = true;
        }
        for (int fd = 3; fd < static_cast<int>(allowed.size()); ++fd) {
            if (::fcntl(fd, F_GETFD) >= 0) {
                if (!allowed[fd]) {
                    return fail("private descriptor survived exec");
                }
            } else if (errno != EBADF) {
                return fail("cannot inspect descriptor");
            }
        }
        return 0;
    }
    return fail("unknown mode or wrong argument count");
}
