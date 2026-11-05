#include "platform.hpp"

#include "sash/runtime.hpp"

#include <algorithm>
#include <array>
#include <atomic>
#include <cerrno>
#include <charconv>
#include <cstdlib>
#include <fcntl.h>
#include <limits>
#include <new>
#include <optional>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>
#include <utility>

namespace sash::platform {

[[noreturn]] void cleanup_failed(std::string_view operation, std::error_code error) noexcept {
    std::array<char, 32> number{};
    const auto end = std::to_chars(number.data(), number.data() + number.size(), error.value()).ptr;
    const int fd = ProgramChannels::diagnostic_fd();
    (void)write_all(fd, "sash-cpp: cleanup failed: ");
    (void)write_all(fd, operation);
    (void)write_all(fd, " (errno ");
    (void)write_all(fd, std::string_view(number.data(), end - number.data()));
    (void)write_all(fd, ")\n");
    std::_Exit(2);
}

namespace {

std::error_code last_error() noexcept { return {errno, std::generic_category()}; }

int descriptor_flags(int fd) noexcept {
    int flags;
    do {
        flags = ::fcntl(fd, F_GETFD);
    } while (flags < 0 && errno == EINTR);
    return flags;
}

int checked_descriptor_flags(int fd, std::string_view operation) {
    const int flags = descriptor_flags(fd);
    if (flags < 0 && errno != EBADF) {
        throw SetupError(last_error(), operation);
    }
    return flags;
}

FileDescriptor duplicate(int fd) {
    int minimum = 3;
    while (true) {
        int copy;
        do {
            copy = ::fcntl(fd, F_DUPFD_CLOEXEC, minimum);
        } while (copy < 0 && errno == EINTR);
        if (copy < 0) {
            throw SetupError(last_error(), "save descriptor");
        }
        FileDescriptor saved(copy);
        if (!SourceFds::reserved(copy)) {
            return saved;
        }
        saved.close();
        if (copy == std::numeric_limits<int>::max()) {
            throw SetupError(std::make_error_code(std::errc::too_many_files_open),
                             "save descriptor");
        }
        minimum = copy + 1;
    }
}

FileDescriptor private_descriptor(FileDescriptor raw) {
    if (raw.get() > 2 && !SourceFds::reserved(raw.get())) {
        return raw;
    }
    auto raised = duplicate(raw.get());
    raw.close();
    return raised;
}

FileDescriptor capture_storage() { return private_descriptor(os::capture_storage()); }

Pipe create_pipe(std::string_view operation) {
    auto pipe = os::create_pipe(operation);
    auto reader = private_descriptor(std::move(pipe.reader));
    auto writer = private_descriptor(std::move(pipe.writer));
    return {std::move(reader), std::move(writer)};
}

class Child {
  public:
    explicit Child(pid_t pid) noexcept : pid_(pid) {}
    Child(const Child &) = delete;
    Child &operator=(const Child &) = delete;

    ~Child() noexcept {
        if (pending()) {
            (void)finish();
        }
    }

    int wait() {
        int status;
        if (const auto error = reap(status)) {
            throw SetupError(error, "wait for child");
        }
        return status;
    }

    bool pending() const noexcept { return pid_ > 0; }

    int finish() noexcept {
        int status;
        if (const auto error = reap(status)) {
            cleanup_failed("wait for child", error);
        }
        return status;
    }

  private:
    std::error_code reap(int &status) noexcept {
        while (true) {
            const auto waited = ::waitpid(pid_, &status, 0);
            if (waited < 0) {
                if (errno == EINTR) {
                    continue;
                }
                const auto error = last_error();
                if (error.value() == ECHILD) {
                    pid_ = -1;
                }
                return error;
            }
            if (WIFEXITED(status) || WIFSIGNALED(status)) {
                pid_ = -1;
                return {};
            }
        }
    }

    pid_t pid_;
};

enum class ChildOperation : int {
    CloseReader,
    CloseOutputReader,
    InstallOutput,
    CloseOutputWriter,
    InstallInput,
    CloseInput,
    Exec
};

struct ChildFailure {
    ChildOperation operation = ChildOperation::Exec;
    std::atomic<int> error{0};
};

static_assert(std::atomic<int>::is_always_lock_free,
              "child setup reporting must not acquire an inherited lock");

// EOF detects exec; shared storage reports setup errors without a fallible child write.
class SharedChildFailure {
  public:
    SharedChildFailure() {
        void *storage = ::mmap(nullptr, sizeof(ChildFailure), PROT_READ | PROT_WRITE,
                               MAP_SHARED | MAP_ANONYMOUS, -1, 0);
        if (storage == MAP_FAILED) {
            throw SetupError(last_error(), "allocate child setup status");
        }
        failure_ = ::new (storage) ChildFailure;
    }

    SharedChildFailure(const SharedChildFailure &) = delete;
    SharedChildFailure &operator=(const SharedChildFailure &) = delete;

    ~SharedChildFailure() noexcept {
        failure_->~ChildFailure();
        if (::munmap(failure_, sizeof(ChildFailure)) < 0) {
            cleanup_failed("unmap child setup status", last_error());
        }
    }

    ChildFailure *get() noexcept { return failure_; }

  private:
    ChildFailure *failure_;
};

[[noreturn]] void child_failed(ChildFailure *failure, ChildOperation operation,
                               int error) noexcept {
    failure->operation = operation;
    failure->error.store(error, std::memory_order_release);
    ::_exit(127);
}

void wait_for_exec(int channel) {
    char unexpected;
    while (true) {
        const auto count = ::read(channel, &unexpected, 1);
        if (count < 0) {
            if (errno == EINTR) {
                continue;
            }
            throw SetupError(last_error(), "read exec completion pipe");
        }
        if (count > 0) {
            throw SetupError(std::make_error_code(std::errc::protocol_error),
                             "unexpected exec completion data");
        }
        return;
    }
}

std::error_code replace_descriptor(int source, int destination) noexcept {
    int result;
    do {
        result = ::dup2(source, destination);
    } while (result < 0 && errno == EINTR);
    return result < 0 ? last_error() : std::error_code{};
}

std::error_code restore_descriptor(int fd, int flags, int saved) noexcept {
    if (flags < 0) {
        return ::close(fd) < 0 && errno != EBADF ? last_error() : std::error_code{};
    }
    if (const auto error = replace_descriptor(saved, fd)) {
        return error;
    }
    int result;
    do {
        result = ::fcntl(fd, F_SETFD, flags);
    } while (result < 0 && errno == EINTR);
    return result < 0 ? last_error() : std::error_code{};
}

void validate_redirect(const Redirect &redirect) {
    const auto require_source_fd = [](int fd) {
        if (fd < 0 || !SourceFds::reserved(fd)) {
            throw SetupError(std::make_error_code(std::errc::invalid_argument),
                             "redirect descriptor is not in the source-fd set");
        }
    };
    require_source_fd(redirect.fd);
    switch (redirect.op) {
    case RedirectOp::Duplicate:
        require_source_fd(redirect.source);
        break;
    case RedirectOp::Read:
    case RedirectOp::Write:
    case RedirectOp::Append:
    case RedirectOp::ReadWrite:
        if (redirect.path.find('\0') != std::string::npos) {
            throw SetupError(std::make_error_code(std::errc::invalid_argument),
                             "redirect path contains NUL");
        }
        break;
    default:
        throw SetupError(std::make_error_code(std::errc::invalid_argument),
                         "invalid redirection operation");
    }
}

void apply_redirect(const Redirect &redirect) {
    if (redirect.op == RedirectOp::Duplicate) {
        if (const auto error = replace_descriptor(redirect.source, redirect.fd)) {
            throw SetupError(error, "duplicate redirect descriptor");
        }
        return;
    }
    int flags = O_CLOEXEC;
    switch (redirect.op) {
    case RedirectOp::Read:
        flags |= O_RDONLY;
        break;
    case RedirectOp::Write:
        flags |= O_WRONLY | O_CREAT | O_TRUNC;
        break;
    case RedirectOp::Append:
        flags |= O_WRONLY | O_CREAT | O_APPEND;
        break;
    case RedirectOp::ReadWrite:
        flags |= O_RDWR | O_CREAT;
        break;
    case RedirectOp::Duplicate:
        throw std::logic_error("duplicate redirection bypassed its dispatch");
    }
    int fd;
    do {
        fd = ::open(redirect.path.c_str(), flags, 0666);
    } while (fd < 0 && errno == EINTR);
    if (fd < 0) {
        throw SetupError(last_error(), "open redirect " + redirect.path);
    }
    auto source = private_descriptor(FileDescriptor(fd));
    if (const auto error = replace_descriptor(source.get(), redirect.fd)) {
        throw SetupError(error, "install redirect descriptor");
    }
    source.close();
}

class RedirectFrame {
  public:
    explicit RedirectFrame(std::initializer_list<Redirect> redirects) {
        if (!ProgramChannels::active()) {
            throw SetupError(std::make_error_code(std::errc::operation_not_permitted),
                             "redirection requires program channel setup");
        }
        for (const auto &redirect : redirects) {
            validate_redirect(redirect);
        }
        saved_.reserve(redirects.size());
        for (const auto &redirect : redirects) {
            if (std::none_of(saved_.begin(), saved_.end(), [&](const SavedDescriptor &saved) {
                    return saved.fd == redirect.fd;
                })) {
                saved_.emplace_back(redirect.fd);
            }
        }
    }

    ~RedirectFrame() noexcept {
        if (const auto error = restore()) {
            cleanup_failed("restore redirected descriptors", error);
        }
    }

    RedirectFrame(const RedirectFrame &) = delete;
    RedirectFrame &operator=(const RedirectFrame &) = delete;

    void apply(std::initializer_list<Redirect> redirects) {
        active_ = true;
        for (const auto &redirect : redirects) {
            apply_redirect(redirect);
        }
    }

    void finish() {
        if (const auto error = restore()) {
            throw SetupError(error, "restore redirected descriptors");
        }
        for (auto &saved : saved_) {
            saved.saved.close();
        }
    }

  private:
    std::error_code restore() noexcept {
        if (!active_) {
            return {};
        }
        std::error_code first;
        for (auto saved = saved_.rbegin(); saved != saved_.rend(); ++saved) {
            const auto error = saved->restore();
            if (!first) {
                first = error;
            }
        }
        active_ = static_cast<bool>(first);
        return first;
    }

    std::vector<SavedDescriptor> saved_;
    bool active_ = false;
};

[[nodiscard]] std::optional<SetupError> launch(std::vector<std::string> &command,
                                               std::optional<Child> &child, Pipe *output = nullptr,
                                               int input = -1) {
    std::vector<char *> arguments;
    arguments.reserve(command.size() + 1);
    for (auto &word : command) {
        arguments.push_back(word.data());
    }
    arguments.push_back(nullptr);
    const auto *executable = command[0].c_str();
    auto *const *argv = arguments.data();
    char *empty_environment[] = {nullptr};
    auto *const *inherited_environment = os::environment();
    auto *const *environment =
        inherited_environment != nullptr ? inherited_environment : empty_environment;
    auto channel = create_pipe("create exec error pipe");
    SharedChildFailure shared_failure;
    auto *const failure = shared_failure.get();
    const int reader = channel.reader.get();
    const int output_reader = output != nullptr ? output->reader.get() : -1;
    const int output_writer = output != nullptr ? output->writer.get() : -1;
    const auto pid = ::fork();
    if (pid < 0) {
        const auto error = last_error();
        throw SetupError(error, "fork " + command[0]);
    }
    if (pid == 0) {
        if (::close(reader) < 0) {
            child_failed(failure, ChildOperation::CloseReader, errno);
        }
        if (output_writer >= 0) {
            if (::close(output_reader) < 0) {
                child_failed(failure, ChildOperation::CloseOutputReader, errno);
            }
            if (const auto error = replace_descriptor(output_writer, 1)) {
                child_failed(failure, ChildOperation::InstallOutput, error.value());
            }
            if (::close(output_writer) < 0) {
                child_failed(failure, ChildOperation::CloseOutputWriter, errno);
            }
        }
        if (input >= 0) {
            if (const auto error = replace_descriptor(input, 0)) {
                child_failed(failure, ChildOperation::InstallInput, error.value());
            }
            if (::close(input) < 0) {
                child_failed(failure, ChildOperation::CloseInput, errno);
            }
        }
        ::execve(executable, argv, environment);
        child_failed(failure, ChildOperation::Exec, errno);
    }
    child.emplace(pid);
    if (output != nullptr) {
        output->writer.close();
    }
    channel.writer.close();
    wait_for_exec(channel.reader.get());
    channel.reader.close();
    const int error = failure->error.load(std::memory_order_acquire);
    if (error != 0) {
        if (error < 0) {
            throw SetupError(std::make_error_code(std::errc::protocol_error),
                             "invalid child setup failure");
        }
        switch (failure->operation) {
        case ChildOperation::CloseReader:
            return SetupError({error, std::generic_category()}, "close child exec error reader");
        case ChildOperation::CloseOutputReader:
            return SetupError({error, std::generic_category()}, "close child output reader");
        case ChildOperation::InstallOutput:
            return SetupError({error, std::generic_category()}, "connect child stdout");
        case ChildOperation::CloseOutputWriter:
            return SetupError({error, std::generic_category()}, "close child output writer");
        case ChildOperation::InstallInput:
            return SetupError({error, std::generic_category()}, "connect child stdin");
        case ChildOperation::CloseInput:
            return SetupError({error, std::generic_category()}, "close child input reader");
        case ChildOperation::Exec:
            return SetupError({error, std::generic_category()}, "exec " + command[0]);
        }
        return SetupError(std::make_error_code(std::errc::protocol_error),
                          "unknown child setup operation");
    }
    return std::nullopt;
}

ProcessResult completed(std::vector<std::string> &command, int status) {
    if (WIFEXITED(status)) {
        return {std::move(command[0]), ProcessExit{WEXITSTATUS(status)}};
    }
    return {std::move(command[0]), ProcessSignal{WTERMSIG(status)}};
}

ProcessResult completed(std::vector<std::string> &command, Child &child) {
    return completed(command, child.wait());
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
    const int fd = ProgramChannels::diagnostic_fd();
    if (write_all(fd, program) || write_all(fd, ": ") || write_all(fd, diagnostic) ||
        write_all(fd, "\n")) {
        return 2;
    }
    return status;
}

ProcessResult execute(std::vector<std::string> &command) {
    std::optional<Child> child;
    if (const auto error = launch(command, child)) {
        throw *error;
    }
    return completed(command, *child);
}

ProcessResult read_from(std::vector<std::string> &command, const std::function<void(int)> &body) {
    // Close the pipe before the child owner waits, including during unwinding.
    std::optional<Child> child;
    std::optional<Pipe> output;
    try {
        output.emplace(create_pipe("create command output pipe"));
        if (const auto error = launch(command, child, &*output)) {
            throw *error;
        }
    } catch (const SetupError &error) {
        return {std::move(command[0]), error};
    }
    body(output->reader.get());
    try {
        output->reader.close();
        return completed(command, *child);
    } catch (const SetupError &error) {
        return {std::move(command[0]), error};
    }
}

PipelineResult read_pipeline(std::vector<std::vector<std::string>> &commands,
                             const std::function<void(int)> &body) {
    PipelineResult result;
    result.stages.resize(commands.size());
    // All parent endpoints must close before any child owner waits.
    std::vector<std::optional<Child>> children(commands.size());
    std::vector<Pipe> pipes;
    pipes.reserve(commands.size());
    const auto failure = [&](std::size_t stage, const SetupError &error) {
        result.setup_errors.push_back({stage, error});
    };
    const auto retain = [&](std::size_t stage, ProcessResult outcome) {
        if (!result.stages[stage]) {
            result.stages[stage] = std::move(outcome);
        }
    };
    std::size_t stage = 0;
    bool ready = false;
    try {
        for (; stage < commands.size(); ++stage) {
            pipes.push_back(create_pipe("create pipeline pipe"));
        }
        for (stage = 0; stage < commands.size(); ++stage) {
            const int input = stage > 0 ? pipes[stage - 1].reader.get() : -1;
            if (const auto error = launch(commands[stage], children[stage], &pipes[stage], input)) {
                retain(stage, {commands[stage][0], *error});
                throw *error;
            }
            if (stage > 0) {
                pipes[stage - 1].reader.close();
            }
        }
        ready = true;
    } catch (const SetupError &error) {
        failure(stage, error);
    }
    if (ready) {
        body(pipes.back().reader.get());
    }
    const auto close = [&](FileDescriptor &fd, std::size_t index) {
        try {
            fd.close();
        } catch (const SetupError &error) {
            failure(index, error);
        }
    };
    for (std::size_t i = 0; i < pipes.size(); ++i) {
        close(pipes[i].reader, i);
        close(pipes[i].writer, i);
    }
    for (std::size_t i = 0; i < children.size(); ++i) {
        if (children[i]) {
            try {
                retain(i, completed(commands[i], *children[i]));
            } catch (const SetupError &error) {
                failure(i, error);
                if (children[i]->pending()) {
                    retain(i, completed(commands[i], children[i]->finish()));
                } else {
                    retain(i, {commands[i][0], error});
                }
            }
        }
    }
    return result;
}

std::size_t read_some(int fd, char *buffer, std::size_t capacity, std::string_view operation) {
    ssize_t count;
    do {
        count = ::read(fd, buffer, capacity);
    } while (count < 0 && errno == EINTR);
    if (count < 0) {
        throw SetupError(last_error(), operation);
    }
    return static_cast<std::size_t>(count);
}

FileDescriptor *FileDescriptor::head_ = nullptr;

FileDescriptor::FileDescriptor(int fd) noexcept : fd_(fd), next_(head_) { head_ = this; }

FileDescriptor::~FileDescriptor() noexcept {
    if (fd_ >= 0 && ::close(fd_) < 0) {
        cleanup_failed("close descriptor", last_error());
    }
    auto **entry = &head_;
    while (*entry != this) {
        entry = &(*entry)->next_;
    }
    *entry = next_;
}

FileDescriptor::FileDescriptor(FileDescriptor &&other) noexcept
    : FileDescriptor(std::exchange(other.fd_, -1)) {}

bool FileDescriptor::owns(int fd) noexcept {
    for (auto *entry = head_; entry != nullptr; entry = entry->next_) {
        if (entry->fd_ == fd) {
            return true;
        }
    }
    return false;
}

void FileDescriptor::close() {
    // Linux and Darwin release ordinary fds before reporting file-close errors; do not retry.
    const int fd = std::exchange(fd_, -1);
    if (fd >= 0 && ::close(fd) < 0) {
        throw SetupError(last_error(), "close descriptor");
    }
}

SourceFds *SourceFds::active_ = nullptr;

SourceFds::SourceFds(std::initializer_list<int> fds) : fds_(fds), previous_(active_) {
    for (const int fd : fds_) {
        if (fd < 0) {
            throw SetupError(std::make_error_code(std::errc::invalid_argument),
                             "source descriptor must be nonnegative");
        }
        if (FileDescriptor::owns(fd)) {
            throw SetupError(std::make_error_code(std::errc::device_or_resource_busy),
                             "source descriptor is already owned by the runtime");
        }
    }
    active_ = this;
}

SourceFds::~SourceFds() noexcept { active_ = previous_; }

bool SourceFds::reserved(int fd) noexcept {
    if (fd >= 0 && fd <= 2) {
        return true;
    }
    for (auto *scope = active_; scope != nullptr; scope = scope->previous_) {
        if (std::find(scope->fds_.begin(), scope->fds_.end(), fd) != scope->fds_.end()) {
            return true;
        }
    }
    return false;
}

SavedDescriptor::SavedDescriptor(int target)
    : fd(target), flags(checked_descriptor_flags(target, "inspect redirect descriptor")),
      saved(flags < 0 ? FileDescriptor{} : duplicate(target)) {}

std::error_code SavedDescriptor::restore() noexcept {
    return restore_descriptor(fd, flags, saved.get());
}

ProgramChannels *ProgramChannels::active_ = nullptr;

ProgramChannels::ProgramChannels() : input_(0), output_(1), error_(2), previous_(active_) {
    active_ = this;
}

ProgramChannels::~ProgramChannels() noexcept {
    if (!restored_) {
        if (const auto error = restore_all()) {
            cleanup_failed("restore invoker channels", error);
        }
    }
    active_ = previous_;
}

bool ProgramChannels::active() noexcept { return active_ != nullptr; }

int ProgramChannels::diagnostic_fd() noexcept {
    return active_ != nullptr ? active_->error_.saved.get() : 2;
}

std::error_code ProgramChannels::restore_all() noexcept {
    std::error_code first;
    for (auto *saved : {&input_, &output_, &error_}) {
        const auto error = saved->restore();
        if (!first) {
            first = error;
        }
    }
    restored_ = !first;
    return first;
}

void ProgramChannels::restore() {
    if (const auto error = restore_all()) {
        throw SetupError(error, "restore invoker channels");
    }
}

void redirected(std::initializer_list<Redirect> redirects, const std::function<void()> &body) {
    RedirectFrame frame(redirects);
    frame.apply(redirects);
    body();
    frame.finish();
}

StdoutCapture::StdoutCapture()
    : original_flags_(checked_descriptor_flags(1, "inspect stdout")), storage_(capture_storage()),
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
    if (const auto error = restore_descriptor(1, original_flags_, saved_stdout_.get())) {
        return error;
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
