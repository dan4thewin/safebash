#ifndef SASH_CPP_RUNTIME_HPP
#define SASH_CPP_RUNTIME_HPP

#include <array>
#include <functional>
#include <initializer_list>
#include <new>
#include <optional>
#include <regex>
#include <stdexcept>
#include <string>
#include <string_view>
#include <system_error>
#include <type_traits>
#include <utility>
#include <variant>
#include <vector>

namespace sash {

struct CommandResult {
    int status = 0;
    std::string diagnostic;

    [[nodiscard]] bool success() const noexcept { return status == 0; }
};

class CommandError final : public std::runtime_error {
  public:
    CommandError(int status, std::string diagnostic);
    [[nodiscard]] int status() const noexcept { return status_; }

  private:
    int status_;
};

class SetupError final : public std::system_error {
  public:
    SetupError(std::error_code error, std::string_view operation);
};

struct ProcessExit {
    int status = 0;
};

struct ProcessSignal {
    int number = 0;
};

struct ProcessResult {
    std::string executable;
    std::variant<ProcessExit, ProcessSignal, SetupError> outcome;

    [[nodiscard]] bool success() const noexcept {
        const auto *exit = std::get_if<ProcessExit>(&outcome);
        return exit != nullptr && exit->status == 0;
    }
};

struct Command {
    std::string executable;
    std::vector<std::string> arguments;
};

struct PipelineSetupError {
    std::size_t stage;
    SetupError error;
};

struct PipelineResult {
    // Empty slots are unstarted stages, not successful exits.
    std::vector<std::optional<ProcessResult>> stages;
    std::vector<PipelineSetupError> setup_errors;

    [[nodiscard]] bool success() const noexcept;
};

struct PipelineCapture {
    // Exact bytes, including NUL and final newlines; failed outcomes do not erase them.
    std::string output;
    PipelineResult pipeline;
};

enum class ShutdownReason { Completed, ExplicitExit, Failure, Signal };

struct Shutdown {
    ShutdownReason reason = ShutdownReason::Completed;
    int status = 0;
    int signal = 0;
};

struct ExitRequest {
    int status;
    int signal = 0;
};

enum class SignalDisposition { Ignore, Default, Flag, Exit };

[[noreturn]] void exit_program(int status = 0);
void signal(int number, SignalDisposition disposition);
[[nodiscard]] int signum() noexcept;
void clear_signum() noexcept;
void checkpoint();

void check(const CommandResult &result);
void check(const ProcessResult &result);
void check(const PipelineResult &result);
[[nodiscard]] ProcessResult try_execute(std::string_view executable,
                                        const std::vector<std::string> &arguments = {});
void execute(std::string_view executable, const std::vector<std::string> &arguments = {});
[[nodiscard]] PipelineCapture try_capture_pipeline(const std::vector<Command> &commands);
[[nodiscard]] std::string capture_pipeline(const std::vector<Command> &commands);
[[nodiscard]] CommandResult try_write(std::string_view bytes, int fd = 1);
void write(std::string_view bytes, int fd = 1);
[[nodiscard]] CommandResult try_echo(std::initializer_list<std::string_view> words);
void echo(std::initializer_list<std::string_view> words);
[[nodiscard]] std::regex compile_regex(std::string_view pattern);

class LineReader;

namespace detail {
void consume_lines(int fd, const std::function<void(LineReader &)> &body);
ProcessResult read_from(std::string_view executable, const std::vector<std::string> &arguments,
                        const std::function<void(LineReader &)> &body);
PipelineResult read_pipeline(const std::vector<Command> &commands,
                             const std::function<void(LineReader &)> &body);
std::string capture_stdout(const std::function<void()> &body);
int report_error(std::string_view program, std::string_view diagnostic, int status) noexcept;
int run_program(std::string_view program, const std::function<void()> &body,
                const std::function<void(const Shutdown &)> &end);
} // namespace detail

class LineReader {
  public:
    LineReader(const LineReader &) = delete;
    LineReader &operator=(const LineReader &) = delete;
    [[nodiscard]] bool readline(std::string &target);

  private:
    friend void detail::consume_lines(int, const std::function<void(LineReader &)> &);
    explicit LineReader(int fd) noexcept : fd_(fd) {}

    int fd_;
    std::array<char, 8192> buffer_{};
    std::size_t begin_ = 0;
    std::size_t end_ = 0;
    bool eof_ = false;
};

template <typename Body>
[[nodiscard]] ProcessResult try_read_from(std::string_view executable,
                                          const std::vector<std::string> &arguments, Body &&body) {
    static_assert(std::is_same_v<std::invoke_result_t<Body &, LineReader &>, void>,
                  "the reader body must return void");
    return detail::read_from(executable, arguments, std::forward<Body>(body));
}

template <typename Body>
void read_from(std::string_view executable, const std::vector<std::string> &arguments,
               Body &&body) {
    check(try_read_from(executable, arguments, std::forward<Body>(body)));
}

template <typename Body>
[[nodiscard]] PipelineResult try_read_pipeline(const std::vector<Command> &commands, Body &&body) {
    static_assert(std::is_same_v<std::invoke_result_t<Body &, LineReader &>, void>,
                  "the reader body must return void");
    return detail::read_pipeline(commands, std::forward<Body>(body));
}

template <typename Body> void read_pipeline(const std::vector<Command> &commands, Body &&body) {
    check(try_read_pipeline(commands, std::forward<Body>(body)));
}

template <typename Body> [[nodiscard]] std::string capture_stdout(Body &&body) {
    static_assert(std::is_same_v<std::invoke_result_t<Body &>, void>,
                  "stdout capture takes a void body, not an integer result");
    return detail::capture_stdout(std::forward<Body>(body));
}

template <typename Body, typename End> int run(std::string_view program, Body &&body, End &&end) {
    static_assert(std::is_same_v<std::invoke_result_t<Body &>, void>,
                  "the program body must return void");
    static_assert(std::is_same_v<std::invoke_result_t<End &, const Shutdown &>, void>,
                  "the END body must return void");
    return detail::run_program(program, std::ref(body), std::ref(end));
}

template <typename Body> int run(std::string_view program, Body &&body) {
    return run(program, std::forward<Body>(body), [](const Shutdown &) {});
}

} // namespace sash

#endif
