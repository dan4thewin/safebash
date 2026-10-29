#include <array>
#include <cerrno>
#include <charconv>
#include <csignal>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fcntl.h>
#include <string_view>
#include <unistd.h>

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
    if (mode == "records" && argc == 3) {
        int status;
        if (!number(argv[2], status) || status < 0 || status > 255) {
            return fail("invalid record-producer status");
        }
        const char records[] = "alpha\n\nnul\0inside\r\nlast";
        if (!output(records, sizeof(records) - 1) || flushed() != 0) {
            return fail("cannot write records");
        }
        return status;
    }
    if (mode == "flood" && argc == 2) {
        std::array<char, 8192> block{};
        block.fill('x');
        while (output(block.data(), block.size())) {
        }
        return fail("cannot write flood");
    }
    if (mode == "handshake" && argc == 3) {
        int fd;
        if (!number(argv[2], fd) || fd < 0) {
            return fail("invalid handshake descriptor");
        }
        if (!output("ready\n", 6) || flushed() != 0) {
            return fail("cannot announce handshake");
        }
        char reply;
        ssize_t count;
        do {
            count = ::read(fd, &reply, 1);
        } while (count < 0 && errno == EINTR);
        if (count != 1 || reply != 'Y') {
            return fail("missing handshake reply");
        }
        if (!output("done\n", 5)) {
            return fail("cannot finish handshake");
        }
        return flushed();
    }
    if (mode == "stdin" && argc == 2) {
        std::array<char, 8192> block{};
        while (true) {
            const auto count = ::read(0, block.data(), block.size());
            if (count < 0 && errno == EINTR) {
                continue;
            }
            if (count < 0) {
                return fail("cannot read stdin");
            }
            if (count == 0) {
                return flushed();
            }
            if (!output(block.data(), static_cast<std::size_t>(count))) {
                return fail("cannot copy stdin");
            }
        }
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
    if (mode == "ignore-sigchld" && argc >= 3) {
        if (std::signal(SIGCHLD, SIG_IGN) == SIG_ERR) {
            return fail("cannot ignore SIGCHLD");
        }
        ::execv(argv[2], argv + 2);
        std::perror("command-helper: exec with ignored SIGCHLD");
        return 2;
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
