#include "platform.hpp"

#include "sash/runtime.hpp"

#include <cerrno>
#include <crt_externs.h>
#include <cstdlib>
#include <fcntl.h>
#include <filesystem>
#include <unistd.h>

namespace sash::platform::os {
namespace {

std::error_code last_error() noexcept { return {errno, std::generic_category()}; }

void close_on_exec(int fd, std::string_view operation) {
    int result;
    do {
        result = ::fcntl(fd, F_SETFD, FD_CLOEXEC);
    } while (result < 0 && errno == EINTR);
    if (result < 0) {
        throw SetupError(last_error(), operation);
    }
}

} // namespace

FileDescriptor capture_storage() {
    std::error_code error;
    const auto directory = std::filesystem::temp_directory_path(error);
    if (error) {
        throw SetupError(error, "locate capture directory");
    }
    auto name = (directory / "sash-cpp-capture.XXXXXX").string();
    FileDescriptor storage(::mkstemp(name.data()));
    if (storage.get() < 0) {
        throw SetupError(last_error(), "create capture file");
    }
    int result;
    do {
        result = ::unlink(name.c_str());
    } while (result < 0 && errno == EINTR);
    if (result < 0) {
        throw SetupError(last_error(), "unlink capture file");
    }
    close_on_exec(storage.get(), "mark capture file close-on-exec");
    return storage;
}

Pipe create_pipe(std::string_view operation) {
    int descriptors[2];
    int result;
    do {
        result = ::pipe(descriptors);
    } while (result < 0 && errno == EINTR);
    if (result < 0) {
        throw SetupError(last_error(), operation);
    }
    Pipe pipe{FileDescriptor(descriptors[0]), FileDescriptor(descriptors[1])};
    close_on_exec(pipe.reader.get(), "mark pipe reader close-on-exec");
    close_on_exec(pipe.writer.get(), "mark pipe writer close-on-exec");
    return pipe;
}

char **environment() noexcept { return *_NSGetEnviron(); }

} // namespace sash::platform::os
