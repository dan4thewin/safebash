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

enum class TerminalPhase { Main, Exit, Error };

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
    TerminalPhase phase = TerminalPhase::Main;

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

int exit_byte(std::int64_t status) noexcept {
    return static_cast<int>(static_cast<std::uint64_t>(status) & 255);
}

Shutdown failed(std::string_view program, std::string_view message, int status = 2) noexcept {
    return {ShutdownReason::Failure, detail::report_error(program, message, status), 0};
}

template <typename Body> Shutdown checked(std::string_view program, Body &&body) {
    try {
        return body();
    } catch (const ExitRequest &request) {
        return {request.signal == 0 ? ShutdownReason::ExplicitExit : ShutdownReason::Signal,
                request.status, request.signal};
    } catch (const CommandError &error) {
        return failed(program, error.what(), error.status());
    } catch (const ArithmeticError &error) {
        return failed(program, error.what());
    } catch (const SetupError &error) {
        return failed(program, error.what());
    } catch (const std::regex_error &error) {
        return failed(program, error.what());
    } catch (const std::bad_alloc &) {
        return failed(program, "out of memory");
    } catch (const std::length_error &) {
        return failed(program, "value exceeds string capacity");
    }
}

bool selects_error(const Shutdown &shutdown) noexcept {
    return shutdown.reason == ShutdownReason::Failure ||
           (shutdown.reason == ShutdownReason::ExplicitExit && shutdown.status != 0);
}

std::int64_t finish_error(std::string_view program,
                          const std::function<void(const Shutdown &)> &on_error,
                          const Shutdown &shutdown) {
    if (!on_error) {
        return shutdown.status;
    }
    try {
        on_error(shutdown);
        return shutdown.status;
    } catch (const ExitRequest &request) {
        return request.status;
    } catch (const ArithmeticError &error) {
        return detail::report_error(program, error.what(), 2);
    } catch (const std::bad_alloc &) {
        return detail::report_error(program, "out of memory", 2);
    }
}

} // namespace

[[noreturn]] void exit_program(std::int64_t status) {
    if (active_program != nullptr && !active_program->owner()) {
        ::_exit(exit_byte(status));
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

void checkpoint(bool deferred) {
    if (deferred || active_program == nullptr || active_program->phase != TerminalPhase::Main) {
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
                const std::function<std::int64_t()> &body,
                const std::function<void(const Shutdown &)> &on_exit,
                const std::function<void(const Shutdown &)> &on_error) {
    ProgramScope scope;
    std::optional<platform::SourceFds> sources;
    std::optional<platform::ProgramChannels> channels;
    try {
        sources.emplace(source_fds);
        channels.emplace();
    } catch (const SetupError &error) {
        return report_error(program, error.what(), 2);
    } catch (const std::bad_alloc &) {
        if (!sources) {
            return report_error(program, "out of memory", 2);
        }
        scope.phase = TerminalPhase::Error;
        const auto shutdown = failed(program, "out of memory");
        return exit_byte(finish_error(program, on_error, shutdown));
    } catch (const std::length_error &) {
        return report_error(program, "source-fd set exceeds capacity", 2);
    }
    auto shutdown = checked(program, [&] {
        checkpoint();
        const auto status = body();
        checkpoint();
        return Shutdown{ShutdownReason::Completed, status, 0};
    });
    if (!scope.owner()) {
        ::_exit(exit_byte(shutdown.status));
    }
    const bool error = selects_error(shutdown);
    scope.phase = error ? TerminalPhase::Error : TerminalPhase::Exit;
    try {
        channels->restore();
    } catch (const SetupError &error) {
        return report_error(program, error.what(), 2);
    }
    if (!error && on_exit) {
        shutdown = checked(program, [&] {
            on_exit(shutdown);
            return Shutdown{ShutdownReason::Completed, shutdown.status, shutdown.signal};
        });
        if (!scope.owner()) {
            ::_exit(exit_byte(shutdown.status));
        }
        if (selects_error(shutdown)) {
            scope.phase = TerminalPhase::Error;
            try {
                channels->restore();
            } catch (const SetupError &failure) {
                return report_error(program, failure.what(), 2);
            }
        }
    }
    const auto status = scope.phase == TerminalPhase::Error
                            ? finish_error(program, on_error, shutdown)
                            : shutdown.status;
    if (!scope.owner()) {
        ::_exit(exit_byte(status));
    }
    return exit_byte(status);
}

} // namespace detail
} // namespace sash
