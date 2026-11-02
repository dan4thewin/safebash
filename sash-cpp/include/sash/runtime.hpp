#ifndef SASH_CPP_RUNTIME_HPP
#define SASH_CPP_RUNTIME_HPP

#include <array>
#include <cstdint>
#include <exception>
#include <functional>
#include <initializer_list>
#include <iterator>
#include <map>
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

enum class ArithmeticFault { Overflow, DivisionByZero, NegativeShiftCount };

class ArithmeticError final : public std::exception {
  public:
    explicit ArithmeticError(ArithmeticFault fault) noexcept;
    [[nodiscard]] ArithmeticFault fault() const noexcept { return fault_; }
    [[nodiscard]] const char *what() const noexcept override;

  private:
    ArithmeticFault fault_;
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

[[nodiscard]] std::int64_t add(std::int64_t left, std::int64_t right);
[[nodiscard]] std::int64_t sub(std::int64_t left, std::int64_t right);
[[nodiscard]] std::int64_t mul(std::int64_t left, std::int64_t right);
[[nodiscard]] std::int64_t neg(std::int64_t value);
[[nodiscard]] std::int64_t div(std::int64_t left, std::int64_t right);
[[nodiscard]] std::int64_t mod(std::int64_t left, std::int64_t right);
[[nodiscard]] std::int64_t pow(std::int64_t base, std::int64_t exponent);
[[nodiscard]] std::int64_t shl(std::int64_t value, std::int64_t count);
[[nodiscard]] std::int64_t shr(std::int64_t value, std::int64_t count);

template <typename T> using Array = std::map<std::int64_t, T>;
template <typename T> using Map = std::map<std::string, T>;

namespace detail {
void check_key(std::int64_t index);
inline void check_key(const std::string &) noexcept {}
std::int64_t collection_count(std::size_t size);
std::string argument_text(std::string_view value);
std::string argument_text(std::int64_t value);
[[noreturn]] void required_failure(std::string_view message);
inline bool is_zero(const std::string &value) noexcept { return value.empty(); }
inline bool is_zero(std::int64_t value) noexcept { return value == 0; }
} // namespace detail

template <typename Key, typename T>
[[nodiscard]] T cell(const std::map<Key, T> &collection,
                     const typename std::map<Key, T>::key_type &key) {
    detail::check_key(key);
    const auto found = collection.find(key);
    return found == collection.end() ? T{} : found->second;
}

template <typename Key, typename T>
void set_cell(std::map<Key, T> &collection, const typename std::map<Key, T>::key_type &key,
              typename std::map<Key, T>::mapped_type value) {
    detail::check_key(key);
    collection.insert_or_assign(key, std::move(value));
}

template <typename Key, typename T>
void unset(std::map<Key, T> &collection, const typename std::map<Key, T>::key_type &key) {
    detail::check_key(key);
    collection.erase(key);
}

template <typename Key, typename T>
[[nodiscard]] std::int64_t len(const std::map<Key, T> &collection) {
    return detail::collection_count(collection.size());
}

namespace detail {

template <typename Key, typename T, bool Keys> class LiveRange {
    using Collection = std::map<Key, T>;

  public:
    struct Sentinel {};

    class Iterator {
      public:
        explicit Iterator(const Collection &collection) : collection_(&collection) {
            select(collection.begin());
        }

        [[nodiscard]] bool operator!=(Sentinel) const noexcept { return key_.has_value(); }

        [[nodiscard]] auto operator*() const {
            if constexpr (Keys) {
                return *key_;
            } else {
                return collection_->at(*key_);
            }
        }

        Iterator &operator++() {
            select(collection_->upper_bound(*key_));
            return *this;
        }

      private:
        void select(typename Collection::const_iterator next) {
            if (next == collection_->end()) {
                key_.reset();
            } else {
                check_key(next->first);
                key_ = next->first;
            }
        }

        const Collection *collection_;
        // A body may erase its current cell; no iterator or cell reference survives it.
        std::optional<Key> key_;
    };

    explicit LiveRange(const Collection &collection) noexcept : collection_(collection) {}
    [[nodiscard]] Iterator begin() const { return Iterator(collection_); }
    [[nodiscard]] Sentinel end() const noexcept { return {}; }

  private:
    const Collection &collection_;
};

} // namespace detail

template <typename Key, typename T> [[nodiscard]] auto values(const std::map<Key, T> &collection) {
    return detail::LiveRange<Key, T, false>(collection);
}

template <typename Key, typename T> auto values(const std::map<Key, T> &&) = delete;

template <typename T> [[nodiscard]] auto keys(const Map<T> &collection) {
    return detail::LiveRange<std::string, T, true>(collection);
}

template <typename T> auto keys(const Map<T> &&) = delete;

template <typename T> [[nodiscard]] Array<std::string> snapshot_keys(const Map<T> &collection) {
    (void)len(collection);
    Array<std::string> result;
    std::int64_t index = 0;
    for (auto key : keys(collection)) {
        result.emplace(index++, std::move(key));
    }
    return result;
}

template <typename Key, typename T>
[[nodiscard]] std::string join(const std::map<Key, T> &collection,
                               std::string_view separator = " ") {
    std::string result;
    bool first = true;
    for (const auto value : values(collection)) {
        if (!first && !separator.empty()) {
            result.append(separator.data(), separator.size());
        }
        first = false;
        result += detail::argument_text(value);
    }
    return result;
}

template <typename T, typename Fallback>
[[nodiscard]] T default_value(T value, Fallback &&fallback) {
    static_assert(std::is_same_v<T, std::string> || std::is_same_v<T, std::int64_t>,
                  "dispositions take a string or a 64-bit integer");
    return detail::is_zero(value) ? T(std::invoke(std::forward<Fallback>(fallback)))
                                  : std::move(value);
}

template <typename T, typename Message> [[nodiscard]] T required_value(T value, Message &&message) {
    static_assert(std::is_same_v<T, std::string> || std::is_same_v<T, std::int64_t>,
                  "dispositions take a string or a 64-bit integer");
    if (detail::is_zero(value)) {
        detail::required_failure(std::invoke(std::forward<Message>(message)));
    }
    return value;
}

inline void append_argument(std::vector<std::string> &arguments, std::string_view value) {
    arguments.push_back(detail::argument_text(value));
}

inline void append_argument(std::vector<std::string> &arguments, std::int64_t value) {
    arguments.push_back(detail::argument_text(value));
}

inline void append_elided(std::vector<std::string> &arguments, std::string_view value) {
    if (!value.empty()) {
        append_argument(arguments, value);
    }
}

inline void append_elided(std::vector<std::string> &arguments, std::int64_t value) {
    if (value != 0) {
        append_argument(arguments, value);
    }
}

template <typename Key, typename T>
void append_splat(std::vector<std::string> &arguments, const std::map<Key, T> &collection) {
    std::vector<std::string> words;
    words.reserve(collection.size());
    for (const auto value : values(collection)) {
        append_argument(words, value);
    }
    arguments.insert(arguments.end(), std::make_move_iterator(words.begin()),
                     std::make_move_iterator(words.end()));
}

template <typename Key, typename T, typename Fallback>
void append_splat_or(std::vector<std::string> &arguments, const std::map<Key, T> &collection,
                     Fallback &&fallback) {
    if (collection.empty()) {
        append_argument(arguments, std::invoke(std::forward<Fallback>(fallback)));
    } else {
        append_splat(arguments, collection);
    }
}

template <typename Key, typename T, typename Message>
void append_splat_required(std::vector<std::string> &arguments, const std::map<Key, T> &collection,
                           Message &&message) {
    if (collection.empty()) {
        detail::required_failure(std::invoke(std::forward<Message>(message)));
    }
    append_splat(arguments, collection);
}

namespace detail {

template <typename T, typename Odd> Map<T> pair_map(const Array<T> &collection, Odd &&odd) {
    Map<T> result;
    std::optional<T> pending;
    for (const auto value : values(collection)) {
        if (!pending) {
            pending = value;
        } else {
            result.insert_or_assign(argument_text(*pending), value);
            pending.reset();
        }
    }
    if (pending) {
        if (auto key = std::invoke(std::forward<Odd>(odd))) {
            result.insert_or_assign(std::move(*key), std::move(*pending));
        }
    }
    return result;
}

} // namespace detail

template <typename T> [[nodiscard]] Map<T> map_pairs_elided(const Array<T> &collection) {
    return detail::pair_map(collection, [] { return std::optional<std::string>{}; });
}

template <typename T, typename Fallback>
[[nodiscard]] Map<T> map_pairs_or(const Array<T> &collection, Fallback &&key) {
    return detail::pair_map(collection, [&] {
        return std::optional<std::string>(std::invoke(std::forward<Fallback>(key)));
    });
}

template <typename T, typename Message>
[[nodiscard]] Map<T> map_pairs_required(const Array<T> &collection, Message &&message) {
    return detail::pair_map(collection, [&]() -> std::optional<std::string> {
        detail::required_failure(std::invoke(std::forward<Message>(message)));
    });
}

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
