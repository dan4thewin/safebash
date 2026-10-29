#ifndef SASH_CPP_RUNTIME_HPP
#define SASH_CPP_RUNTIME_HPP

#include <array>
#include <functional>
#include <initializer_list>
#include <new>
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

void check(const CommandResult &result);
void check(const ProcessResult &result);
[[nodiscard]] ProcessResult try_execute(std::string_view executable,
                                        const std::vector<std::string> &arguments = {});
void execute(std::string_view executable, const std::vector<std::string> &arguments = {});
[[nodiscard]] CommandResult try_write(std::string_view bytes, int fd = 1);
void write(std::string_view bytes, int fd = 1);
[[nodiscard]] CommandResult try_echo(std::initializer_list<std::string_view> words);
void echo(std::initializer_list<std::string_view> words);
[[nodiscard]] std::regex compile_regex(std::string_view pattern);

class LineReader;

namespace detail {
ProcessResult read_from(std::string_view executable, const std::vector<std::string> &arguments,
                        const std::function<void(LineReader &)> &body);
std::string capture_stdout(const std::function<void()> &body);
int report_error(std::string_view program, std::string_view diagnostic, int status) noexcept;
} // namespace detail

class LineReader {
  public:
    LineReader(const LineReader &) = delete;
    LineReader &operator=(const LineReader &) = delete;
    [[nodiscard]] bool readline(std::string &target);

  private:
    friend ProcessResult detail::read_from(std::string_view, const std::vector<std::string> &,
                                           const std::function<void(LineReader &)> &);
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

template <typename Body> [[nodiscard]] std::string capture_stdout(Body &&body) {
    static_assert(std::is_same_v<std::invoke_result_t<Body &>, void>,
                  "stdout capture takes a void body, not an integer result");
    return detail::capture_stdout(std::forward<Body>(body));
}

template <typename Body> int run(std::string_view program, Body &&body) {
    static_assert(std::is_same_v<std::invoke_result_t<Body &>, void>,
                  "the program body must return void");
    try {
        std::invoke(body);
        return 0;
    } catch (const CommandError &error) {
        return detail::report_error(program, error.what(), error.status());
    } catch (const SetupError &error) {
        return detail::report_error(program, error.what(), 2);
    } catch (const std::regex_error &error) {
        return detail::report_error(program, error.what(), 2);
    } catch (const std::bad_alloc &) {
        return detail::report_error(program, "out of memory", 2);
    } catch (const std::length_error &) {
        return detail::report_error(program, "value exceeds string capacity", 2);
    }
}

} // namespace sash

#endif
