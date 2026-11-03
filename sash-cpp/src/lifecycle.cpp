#include "sash/runtime.hpp"

#include "platform.hpp"

#include <algorithm>
#include <atomic>
#include <cerrno>
#include <csignal>
#include <unistd.h>

namespace sash {
namespace {

std::atomic<int> last_signal{0};
std::atomic<int> exit_signal{0};
static_assert(std::atomic<int>::is_always_lock_free, "signal handlers must not acquire locks");

void flag_signal(int number) noexcept { last_signal.store(number, std::memory_order_relaxed); }

void request_signal_exit(int number) noexcept {
    last_signal.store(number, std::memory_order_relaxed);
    exit_signal.store(number, std::memory_order_relaxed);
}

std::error_code last_error() noexcept { return {errno, std::generic_category()}; }

class SignalMask {
  public:
    SignalMask() noexcept {
        sigset_t all;
        if ((::sigfillset)(&all) < 0 || ::sigprocmask(SIG_BLOCK, &all, &previous_) < 0) {
            error_ = last_error();
        }
    }

    ~SignalMask() noexcept {
        if (!error_ && ::sigprocmask(SIG_SETMASK, &previous_, nullptr) < 0) {
            platform::cleanup_failed("restore lifecycle signal mask", last_error());
        }
    }

    SignalMask(const SignalMask &) = delete;
    SignalMask &operator=(const SignalMask &) = delete;
    std::error_code error() const noexcept { return error_; }

  private:
    sigset_t previous_;
    std::error_code error_;
};

struct SignalRegistration {
    int number;
    struct sigaction previous;
};

class ProgramScope;
ProgramScope *active_program = nullptr;

class ProgramScope {
  public:
    ProgramScope() noexcept : owner_(::getpid()) {
        const SignalMask mask;
        if (const auto error = mask.error()) {
            platform::cleanup_failed("initialize lifecycle signal state", error);
        }
        previous_ = active_program;
        saved_signal_ = last_signal.load(std::memory_order_relaxed);
        saved_exit_ = exit_signal.load(std::memory_order_relaxed);
        last_signal.store(0, std::memory_order_relaxed);
        exit_signal.store(0, std::memory_order_relaxed);
        active_program = this;
    }

    ~ProgramScope() noexcept {
        if (!owner()) {
            return;
        }
        const SignalMask mask;
        if (const auto error = mask.error()) {
            platform::cleanup_failed("block signals during lifecycle restoration", error);
        }
        for (auto i = signals_.rbegin(); i != signals_.rend(); ++i) {
            if (::sigaction(i->number, &i->previous, nullptr) < 0) {
                platform::cleanup_failed("restore signal disposition", last_error());
            }
        }
        active_program = previous_;
        last_signal.store(saved_signal_, std::memory_order_relaxed);
        exit_signal.store(saved_exit_, std::memory_order_relaxed);
    }

    ProgramScope(const ProgramScope &) = delete;
    ProgramScope &operator=(const ProgramScope &) = delete;

    bool owner() const noexcept { return owner_ == ::getpid(); }
    bool in_end = false;

    void install(int number, const struct sigaction &action) {
        const SignalMask mask;
        if (const auto error = mask.error()) {
            throw SetupError(error, "block signals during lifecycle handoff");
        }
        const auto existing = std::find_if(
            signals_.begin(), signals_.end(),
            [number](const SignalRegistration &entry) { return entry.number == number; });
        if (existing != signals_.end()) {
            if (::sigaction(number, &action, nullptr) < 0) {
                throw SetupError(last_error(), "install signal disposition");
            }
            return;
        }
        // Reserve the snapshot before changing the kernel disposition.
        signals_.push_back({number, {}});
        if (::sigaction(number, &action, &signals_.back().previous) < 0) {
            const auto error = last_error();
            signals_.pop_back();
            throw SetupError(error, "install signal disposition");
        }
    }

  private:
    pid_t owner_;
    ProgramScope *previous_ = nullptr;
    int saved_signal_ = 0;
    int saved_exit_ = 0;
    std::vector<SignalRegistration> signals_;
};

Shutdown finish_body(std::string_view program, const std::function<void()> &body) {
    try {
        checkpoint();
        body();
        checkpoint();
        return {};
    } catch (const ExitRequest &request) {
        return {request.signal == 0 ? ShutdownReason::ExplicitExit : ShutdownReason::Signal,
                request.status, request.signal};
    } catch (const CommandError &error) {
        return {ShutdownReason::Failure,
                detail::report_error(program, error.what(), error.status()), 0};
    } catch (const ArithmeticError &error) {
        return {ShutdownReason::Failure, detail::report_error(program, error.what(), 2), 0};
    } catch (const SetupError &error) {
        return {ShutdownReason::Failure, detail::report_error(program, error.what(), 2), 0};
    } catch (const std::regex_error &error) {
        return {ShutdownReason::Failure, detail::report_error(program, error.what(), 2), 0};
    } catch (const std::bad_alloc &) {
        return {ShutdownReason::Failure, detail::report_error(program, "out of memory", 2), 0};
    } catch (const std::length_error &) {
        return {ShutdownReason::Failure,
                detail::report_error(program, "value exceeds string capacity", 2), 0};
    }
}

} // namespace

[[noreturn]] void exit_program(int status) {
    if (active_program != nullptr && !active_program->owner()) {
        ::_exit(status);
    }
    throw ExitRequest{status};
}

void signal(int number, SignalDisposition disposition) {
    if (active_program == nullptr || !active_program->owner()) {
        throw SetupError(std::make_error_code(std::errc::operation_not_permitted),
                         "signal registration requires the program owner");
    }
    struct sigaction action{};
    if ((::sigfillset)(&action.sa_mask) < 0) {
        throw SetupError(last_error(), "initialize signal mask");
    }
    switch (disposition) {
    case SignalDisposition::Ignore:
        action.sa_handler = SIG_IGN;
        break;
    case SignalDisposition::Default:
        action.sa_handler = SIG_DFL;
        break;
    case SignalDisposition::Flag:
        action.sa_handler = flag_signal;
        break;
    case SignalDisposition::Exit:
        action.sa_handler = request_signal_exit;
        break;
    default:
        throw SetupError(std::make_error_code(std::errc::invalid_argument),
                         "invalid signal disposition");
    }
    active_program->install(number, action);
}

int signum() noexcept { return last_signal.load(std::memory_order_relaxed); }

void clear_signum() noexcept { last_signal.store(0, std::memory_order_relaxed); }

void checkpoint() {
    if (active_program == nullptr || active_program->in_end) {
        return;
    }
    const int number = exit_signal.load(std::memory_order_relaxed);
    if (number != 0) {
        if (!active_program->owner()) {
            ::_exit(128 + number);
        }
        throw ExitRequest{128 + number, number};
    }
}

namespace detail {

int run_program(std::string_view program, std::initializer_list<int> source_fds,
                const std::function<void()> &body,
                const std::function<void(const Shutdown &)> &end) {
    ProgramScope scope;
    std::optional<platform::ProgramChannels> channels;
    try {
        channels.emplace(source_fds);
    } catch (const SetupError &error) {
        return report_error(program, error.what(), 2);
    } catch (const std::bad_alloc &) {
        return report_error(program, "out of memory", 2);
    } catch (const std::length_error &) {
        return report_error(program, "source-fd set exceeds capacity", 2);
    }
    const auto shutdown = finish_body(program, body);
    if (!scope.owner()) {
        ::_exit(shutdown.status);
    }
    try {
        channels->restore();
    } catch (const SetupError &error) {
        return report_error(program, error.what(), 2);
    }
    scope.in_end = true;
    int status = shutdown.status;
    try {
        end(shutdown);
    } catch (const ExitRequest &request) {
        status = request.status;
    }
    if (!scope.owner()) {
        ::_exit(status);
    }
    return status;
}

} // namespace detail
} // namespace sash
