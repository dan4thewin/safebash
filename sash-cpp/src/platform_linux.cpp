#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif

#include "platform.hpp"

#include "sash/runtime.hpp"

#include <cerrno>
#include <fcntl.h>
#include <sys/mman.h>
#include <unistd.h>

namespace sash::platform::os {
namespace {

std::error_code last_error() noexcept { return {errno, std::generic_category()}; }

} // namespace

FileDescriptor capture_storage() {
    FileDescriptor storage(::memfd_create("sash-cpp-capture", MFD_CLOEXEC));
    if (storage.get() < 0) {
        throw SetupError(last_error(), "memfd_create");
    }
    return storage;
}

Pipe create_pipe(std::string_view operation) {
    int descriptors[2];
    int result;
    do {
        result = ::pipe2(descriptors, O_CLOEXEC);
    } while (result < 0 && errno == EINTR);
    if (result < 0) {
        throw SetupError(last_error(), operation);
    }
    return {FileDescriptor(descriptors[0]), FileDescriptor(descriptors[1])};
}

char **environment() noexcept { return ::environ; }

} // namespace sash::platform::os
