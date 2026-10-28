#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif

#include "platform.hpp"

#include "sash/runtime.hpp"

#include <algorithm>
#include <array>
#include <cerrno>
#include <charconv>
#include <cstdlib>
#include <fcntl.h>
#include <limits>
#include <sys/mman.h>
#include <unistd.h>
#include <utility>

namespace sash::platform {
namespace {

std::error_code last_error() noexcept { return {errno, std::generic_category()}; }

[[noreturn]] void cleanup_failed(std::string_view operation, std::error_code error) noexcept {
    std::array<char, 32> number{};
    const auto end = std::to_chars(number.data(), number.data() + number.size(), error.value()).ptr;
    (void)write_all(2, "sash-cpp: cleanup failed: ");
    (void)write_all(2, operation);
    (void)write_all(2, " (errno ");
    (void)write_all(2, std::string_view(number.data(), end - number.data()));
    (void)write_all(2, ")\n");
    std::_Exit(2);
}

int descriptor_flags(int fd) noexcept {
    int flags;
    do {
        flags = ::fcntl(fd, F_GETFD);
    } while (flags < 0 && errno == EINTR);
    return flags;
}

int stdout_flags() {
    const int flags = descriptor_flags(1);
    if (flags < 0 && errno != EBADF) {
        throw SetupError(last_error(), "inspect stdout");
    }
    return flags;
}

FileDescriptor duplicate(int fd) {
    int copy;
    do {
        copy = ::fcntl(fd, F_DUPFD_CLOEXEC, 3);
    } while (copy < 0 && errno == EINTR);
    if (copy < 0) {
        throw SetupError(last_error(), "save descriptor");
    }
    return FileDescriptor(copy);
}

FileDescriptor capture_storage() {
    FileDescriptor raw(::memfd_create("sash-cpp-capture", MFD_CLOEXEC));
    if (raw.get() < 0) {
        throw SetupError(last_error(), "memfd_create");
    }
    if (raw.get() > 2) {
        return raw;
    }
    auto raised = duplicate(raw.get());
    raw.close();
    return raised;
}

std::error_code replace_descriptor(int source, int destination) noexcept {
    int result;
    do {
        result = ::dup2(source, destination);
    } while (result < 0 && errno == EINTR);
    return result < 0 ? last_error() : std::error_code{};
}

} // namespace

std::error_code write_all(int fd, std::string_view bytes) noexcept {
    do {
        const auto count =
            std::min(bytes.size(), static_cast<std::size_t>(std::numeric_limits<ssize_t>::max()));
        ssize_t written;
        do {
            written = ::write(fd, bytes.data(), count);
        } while (written < 0 && errno == EINTR);
        if (written < 0) {
            return last_error();
        }
        if (written == 0 && !bytes.empty()) {
            return std::make_error_code(std::errc::io_error);
        }
        bytes.remove_prefix(static_cast<std::size_t>(written));
    } while (!bytes.empty());
    return {};
}

int report_error(std::string_view program, std::string_view diagnostic, int status) noexcept {
    if (write_all(2, program) || write_all(2, ": ") || write_all(2, diagnostic) ||
        write_all(2, "\n")) {
        return 2;
    }
    return status;
}

FileDescriptor::FileDescriptor(int fd) noexcept : fd_(fd) {}

FileDescriptor::~FileDescriptor() noexcept {
    if (fd_ >= 0 && ::close(fd_) < 0) {
        cleanup_failed("close descriptor", last_error());
    }
}

FileDescriptor::FileDescriptor(FileDescriptor &&other) noexcept
    : fd_(std::exchange(other.fd_, -1)) {}

void FileDescriptor::close() {
    const int fd = std::exchange(fd_, -1);
    if (fd >= 0 && ::close(fd) < 0) {
        throw SetupError(last_error(), "close descriptor");
    }
}

StdoutCapture::StdoutCapture()
    : original_flags_(stdout_flags()), storage_(capture_storage()),
      saved_stdout_(original_flags_ < 0 ? FileDescriptor{} : duplicate(1)) {
    if (const auto error = replace_descriptor(storage_.get(), 1)) {
        throw SetupError(error, "capture stdout");
    }
    active_ = true;
}

StdoutCapture::~StdoutCapture() noexcept {
    if (const auto error = restore()) {
        cleanup_failed("restore stdout", error);
    }
}

std::error_code StdoutCapture::restore() noexcept {
    if (!active_) {
        return {};
    }
    if (original_flags_ < 0) {
        if (::close(1) < 0 && errno != EBADF) {
            return last_error();
        }
    } else {
        if (const auto error = replace_descriptor(saved_stdout_.get(), 1)) {
            return error;
        }
        int result;
        do {
            result = ::fcntl(1, F_SETFD, original_flags_);
        } while (result < 0 && errno == EINTR);
        if (result < 0) {
            return last_error();
        }
    }
    active_ = false;
    return {};
}

std::string StdoutCapture::finish() {
    if (const auto error = restore()) {
        throw SetupError(error, "restore stdout");
    }
    saved_stdout_.close();
    off_t position;
    do {
        position = ::lseek(storage_.get(), 0, SEEK_SET);
    } while (position < 0 && errno == EINTR);
    if (position < 0) {
        throw SetupError(last_error(), "rewind capture");
    }
    std::string output;
    std::array<char, 8192> buffer{};
    while (true) {
        const auto count = ::read(storage_.get(), buffer.data(), buffer.size());
        if (count < 0) {
            if (errno == EINTR) {
                continue;
            }
            throw SetupError(last_error(), "read capture");
        }
        if (count == 0) {
            break;
        }
        output.append(buffer.data(), static_cast<std::size_t>(count));
    }
    storage_.close();
    return output;
}

} // namespace sash::platform
