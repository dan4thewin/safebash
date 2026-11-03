#include "sash/runtime.hpp"

#include "platform.hpp"

#include <algorithm>
#include <charconv>
#include <cstdlib>
#include <iterator>
#include <limits>

namespace sash {

CommandError::CommandError(int status, std::string diagnostic)
    : std::runtime_error(std::move(diagnostic)), status_(status) {}

SetupError::SetupError(std::error_code error, std::string_view operation)
    : std::system_error(error, std::string(operation)) {}

ArithmeticError::ArithmeticError(ArithmeticFault fault) noexcept : fault_(fault) {}

const char *ArithmeticError::what() const noexcept {
    switch (fault_) {
    case ArithmeticFault::Overflow:
        return "arithmetic overflow";
    case ArithmeticFault::DivisionByZero:
        return "division by zero";
    case ArithmeticFault::NegativeShiftCount:
        return "negative shift count";
    }
    return "invalid arithmetic fault";
}

void check(const CommandResult &result) {
    if (!result.success()) {
        throw CommandError(result.status, result.diagnostic);
    }
}

void check(const ProcessResult &result) {
    if (const auto *exit = std::get_if<ProcessExit>(&result.outcome)) {
        if (exit->status != 0) {
            throw CommandError(exit->status, result.executable + ": command failed (exit " +
                                                 std::to_string(exit->status) + ")");
        }
    } else if (const auto *signal = std::get_if<ProcessSignal>(&result.outcome)) {
        throw CommandError(128 + signal->number, result.executable + ": terminated by signal " +
                                                     std::to_string(signal->number));
    } else {
        throw std::get<SetupError>(result.outcome);
    }
}

bool PipelineResult::success() const noexcept {
    return setup_errors.empty() && !stages.empty() &&
           std::all_of(stages.begin(), stages.end(),
                       [](const auto &stage) { return stage && stage->success(); });
}

void check(const PipelineResult &result) {
    if (!result.setup_errors.empty()) {
        throw result.setup_errors.front().error;
    }
    if (result.stages.empty()) {
        throw SetupError(std::make_error_code(std::errc::invalid_argument),
                         "pipeline requires at least one stage");
    }
    const ProcessResult *failed = nullptr;
    for (const auto &stage : result.stages) {
        if (!stage) {
            throw SetupError(std::make_error_code(std::errc::protocol_error),
                             "unstarted pipeline stage without a setup failure");
        }
        if (const auto *error = std::get_if<SetupError>(&stage->outcome)) {
            throw *error;
        }
        if (!stage->success()) {
            failed = &*stage;
        }
    }
    if (failed != nullptr) {
        check(*failed);
    }
}

namespace {

using RegexMatches = std::match_results<const char *>;

[[noreturn]] void regex_failure(const std::regex_error &error) {
    throw CommandError(2, std::string("regex: ") + error.what());
}

const char *regex_begin(std::string_view subject) noexcept {
    return subject.empty() ? "" : subject.data();
}

RegexMatches search_regex(std::string_view subject, const std::regex &pattern) {
    const auto *begin = regex_begin(subject);
    RegexMatches matches;
    try {
        std::regex_search(begin, begin + subject.size(), matches, pattern);
    } catch (const std::regex_error &error) {
        regex_failure(error);
    }
    return matches;
}

void append_captures(Array<std::string> &target, const RegexMatches &matches, std::size_t first) {
    for (auto i = first; i < matches.size(); ++i) {
        target.emplace(detail::collection_count(target.size()), matches[i].str());
    }
}

std::string replace_regex(std::string_view subject, const std::regex &pattern,
                          std::string_view replacement, bool first_only) {
    const auto *begin = regex_begin(subject);
    const auto *last = begin + subject.size();
    const auto *format = regex_begin(replacement);
    const auto *unmatched = begin;
    std::string result;
    try {
        const std::regex_iterator<const char *> end;
        for (std::regex_iterator<const char *> it(begin, last, pattern); it != end; ++it) {
            const auto &matches = *it;
            result.append(unmatched, matches[0].first);
            // regex_replace's string-format overload can truncate embedded NULs.
            matches.format(std::back_inserter(result), format, format + replacement.size());
            unmatched = matches[0].second;
            if (first_only) {
                break;
            }
        }
        result.append(unmatched, last);
    } catch (const std::regex_error &error) {
        regex_failure(error);
    }
    return result;
}

std::int64_t signed_bits(std::uint64_t bits) noexcept {
    if (bits <= static_cast<std::uint64_t>(std::numeric_limits<std::int64_t>::max())) {
        return static_cast<std::int64_t>(bits);
    }
    // Avoid an implementation-defined out-of-range unsigned-to-signed cast.
    return -1 - static_cast<std::int64_t>(~bits);
}

const char *argument_pointer(int count, const char *const *arguments, std::int64_t index) {
    if (count < 0 || (count > 0 && arguments == nullptr)) {
        throw SetupError(std::make_error_code(std::errc::invalid_argument),
                         "invalid program argument vector");
    }
    if (index < 0) {
        throw SetupError(std::make_error_code(std::errc::invalid_argument),
                         "argument index must be nonnegative");
    }
    if (index >= count) {
        return nullptr;
    }
    if (arguments[index] == nullptr) {
        throw SetupError(std::make_error_code(std::errc::invalid_argument),
                         "program argument is null");
    }
    return arguments[index];
}

std::vector<std::string> command_words(std::string_view executable,
                                       const std::vector<std::string> &arguments) {
    std::vector<std::string> command;
    command.reserve(arguments.size() + 1);
    command.emplace_back(executable);
    command.insert(command.end(), arguments.begin(), arguments.end());
    for (std::size_t i = 0; i < command.size(); ++i) {
        if (command[i].find('\0') != std::string::npos) {
            const auto context = i == 0
                                     ? "execute: executable contains NUL"
                                     : "execute: argument " + std::to_string(i) + " contains NUL";
            throw SetupError(std::make_error_code(std::errc::invalid_argument), context);
        }
    }
    return command;
}

PipelineResult consume_pipeline(const std::vector<Command> &stages,
                                const std::function<void(int)> &body) {
    if (stages.empty()) {
        throw SetupError(std::make_error_code(std::errc::invalid_argument),
                         "pipeline requires at least one stage");
    }
    std::vector<std::vector<std::string>> commands;
    commands.reserve(stages.size());
    for (std::size_t i = 0; i < stages.size(); ++i) {
        try {
            commands.push_back(command_words(stages[i].executable, stages[i].arguments));
        } catch (const SetupError &error) {
            PipelineResult result;
            result.stages.resize(stages.size());
            result.setup_errors.push_back({i, error});
            return result;
        }
    }
    return platform::read_pipeline(commands, body);
}

} // namespace

PipelineCapture try_capture_pipeline(const std::vector<Command> &commands) {
    PipelineCapture captured;
    captured.pipeline = consume_pipeline(commands, [&](int fd) {
        std::array<char, 8192> buffer{};
        while (true) {
            const auto count = platform::read_some(fd, buffer.data(), buffer.size());
            if (count == 0) {
                break;
            }
            captured.output.append(buffer.data(), count);
        }
    });
    return captured;
}

std::string capture_pipeline(const std::vector<Command> &commands) {
    auto captured = try_capture_pipeline(commands);
    check(captured.pipeline);
    return std::move(captured.output);
}

ProcessResult try_execute(std::string_view executable, const std::vector<std::string> &arguments) {
    try {
        auto command = command_words(executable, arguments);
        return platform::execute(command);
    } catch (const SetupError &error) {
        return {std::string(executable), error};
    }
}

void execute(std::string_view executable, const std::vector<std::string> &arguments) {
    check(try_execute(executable, arguments));
}

namespace {

CommandResult output(std::string_view bytes, int fd, std::string_view operation) {
    if (const auto error = platform::write_all(fd, bytes)) {
        return {2, std::string(operation) + ": " + error.message()};
    }
    return {};
}

} // namespace

CommandResult try_write(std::string_view bytes, int fd) { return output(bytes, fd, "write"); }

void write(std::string_view bytes, int fd) { check(try_write(bytes, fd)); }

CommandResult try_echo(std::initializer_list<std::string_view> words) {
    std::string line;
    bool first = true;
    for (const auto word : words) {
        if (!first) {
            line += ' ';
        }
        first = false;
        if (!word.empty()) {
            line.append(word.data(), word.size());
        }
    }
    line += '\n';
    return output(line, 1, "echo");
}

void echo(std::initializer_list<std::string_view> words) { check(try_echo(words)); }

std::regex compile_regex(std::string_view pattern) {
    if (pattern.find('\0') != std::string_view::npos) {
        throw CommandError(2, "regex: literal NUL in pattern; use \\x00");
    }
    try {
        const char *begin = pattern.empty() ? "" : pattern.data();
        return std::regex(begin, begin + pattern.size(), std::regex_constants::ECMAScript);
    } catch (const std::regex_error &error) {
        regex_failure(error);
    }
}

bool match(std::string_view subject, const std::regex &pattern) {
    return !search_regex(subject, pattern).empty();
}

std::string match_text(std::string_view subject, const std::regex &pattern) {
    const auto matches = search_regex(subject, pattern);
    return matches.empty() ? std::string{} : matches[0].str();
}

bool match(std::string_view subject, const std::regex &pattern, Array<std::string> &target) {
    const auto matches = search_regex(subject, pattern);
    Array<std::string> result;
    append_captures(result, matches, 0);
    target.swap(result);
    return !matches.empty();
}

bool matchall(std::string_view subject, const std::regex &pattern, Array<std::string> &target) {
    const auto *begin = regex_begin(subject);
    Array<std::string> result;
    try {
        const std::regex_iterator<const char *> end;
        for (std::regex_iterator<const char *> it(begin, begin + subject.size(), pattern);
             it != end; ++it) {
            const auto &matches = *it;
            append_captures(result, matches, matches.size() > 1 ? 1 : 0);
        }
    } catch (const std::regex_error &error) {
        regex_failure(error);
    }
    const bool matched = !result.empty();
    target.swap(result);
    return matched;
}

std::string sub(std::string_view subject, const std::regex &pattern, std::string_view replacement) {
    return replace_regex(subject, pattern, replacement, true);
}

std::string gsub(std::string_view subject, const std::regex &pattern,
                 std::string_view replacement) {
    return replace_regex(subject, pattern, replacement, false);
}

std::int64_t add(std::int64_t left, std::int64_t right) {
    const auto minimum = std::numeric_limits<std::int64_t>::min();
    const auto maximum = std::numeric_limits<std::int64_t>::max();
    if ((right > 0 && left > maximum - right) || (right < 0 && left < minimum - right)) {
        throw ArithmeticError(ArithmeticFault::Overflow);
    }
    return left + right;
}

std::int64_t sub(std::int64_t left, std::int64_t right) {
    const auto minimum = std::numeric_limits<std::int64_t>::min();
    const auto maximum = std::numeric_limits<std::int64_t>::max();
    if ((right < 0 && left > maximum + right) || (right > 0 && left < minimum + right)) {
        throw ArithmeticError(ArithmeticFault::Overflow);
    }
    return left - right;
}

std::int64_t mul(std::int64_t left, std::int64_t right) {
    const auto minimum = std::numeric_limits<std::int64_t>::min();
    const auto maximum = std::numeric_limits<std::int64_t>::max();
    if ((left > 0 && right > 0 && left > maximum / right) ||
        (left > 0 && right < 0 && right < minimum / left) ||
        (left < 0 && right > 0 && left < minimum / right) ||
        (left < 0 && right < 0 && left < maximum / right)) {
        throw ArithmeticError(ArithmeticFault::Overflow);
    }
    return left * right;
}

std::int64_t neg(std::int64_t value) {
    if (value == std::numeric_limits<std::int64_t>::min()) {
        throw ArithmeticError(ArithmeticFault::Overflow);
    }
    return -value;
}

std::int64_t div(std::int64_t left, std::int64_t right) {
    if (right == 0) {
        throw ArithmeticError(ArithmeticFault::DivisionByZero);
    }
    if (left == std::numeric_limits<std::int64_t>::min() && right == -1) {
        throw ArithmeticError(ArithmeticFault::Overflow);
    }
    return left / right;
}

std::int64_t mod(std::int64_t left, std::int64_t right) {
    if (right == 0) {
        throw ArithmeticError(ArithmeticFault::DivisionByZero);
    }
    if (left == std::numeric_limits<std::int64_t>::min() && right == -1) {
        return 0;
    }
    return left % right;
}

std::int64_t pow(std::int64_t base, std::int64_t exponent) {
    if (exponent < 0) {
        return 0;
    }
    std::int64_t result = 1;
    while (exponent != 0) {
        if ((exponent & 1) != 0) {
            result = mul(result, base);
        }
        exponent >>= 1;
        // An unused final square can overflow even when the result fits.
        if (exponent != 0) {
            base = mul(base, base);
        }
    }
    return result;
}

std::int64_t shl(std::int64_t value, std::int64_t count) {
    if (count < 0) {
        throw ArithmeticError(ArithmeticFault::NegativeShiftCount);
    }
    if (count >= 64) {
        return 0;
    }
    return signed_bits(static_cast<std::uint64_t>(value) << count);
}

std::int64_t shr(std::int64_t value, std::int64_t count) {
    if (count < 0) {
        throw ArithmeticError(ArithmeticFault::NegativeShiftCount);
    }
    if (count >= 64) {
        return value < 0 ? -1 : 0;
    }
    if (count == 0) {
        return value;
    }
    auto bits = static_cast<std::uint64_t>(value) >> count;
    if (value < 0) {
        bits |= std::numeric_limits<std::uint64_t>::max() << (64 - count);
    }
    return signed_bits(bits);
}

std::optional<std::int64_t> parse_int(std::string_view text) noexcept {
    if (text.empty()) {
        return std::nullopt;
    }
    const bool negative = text.front() == '-';
    if (negative || text.front() == '+') {
        text.remove_prefix(1);
    }
    int base = 10;
    if (text.size() >= 2 && text.front() == '0') {
        switch (text[1]) {
        case 'x':
        case 'X':
            base = 16;
            break;
        case 'o':
        case 'O':
            base = 8;
            break;
        case 'b':
        case 'B':
            base = 2;
            break;
        }
        if (base != 10) {
            text.remove_prefix(2);
        }
    }
    if (text.empty()) {
        return std::nullopt;
    }
    std::uint64_t magnitude = 0;
    const char *end = text.data() + text.size();
    const auto parsed = std::from_chars(text.data(), end, magnitude, base);
    const auto maximum = static_cast<std::uint64_t>(std::numeric_limits<std::int64_t>::max());
    if (parsed.ec != std::errc{} || parsed.ptr != end || magnitude > maximum + negative) {
        return std::nullopt;
    }
    return negative ? signed_bits(std::uint64_t{0} - magnitude)
                    : static_cast<std::int64_t>(magnitude);
}

bool is_int(std::string_view text) noexcept { return parse_int(text).has_value(); }

std::int64_t int_or(std::string_view text, std::int64_t fallback) noexcept {
    return parse_int(text).value_or(fallback);
}

std::optional<std::string> getenv(std::string_view name) {
    if (name.find('\0') != std::string_view::npos) {
        throw SetupError(std::make_error_code(std::errc::invalid_argument),
                         "getenv: name contains NUL");
    }
    const auto key = detail::argument_text(name);
    const char *value = std::getenv(key.c_str());
    return value == nullptr ? std::nullopt : std::optional<std::string>{value};
}

Array<std::string> argv(int count, const char *const *arguments, std::int64_t start) {
    Array<std::string> result;
    (void)argument_pointer(count, arguments, start);
    for (std::int64_t index = start; index < count; ++index) {
        result.emplace(index - start, argument_pointer(count, arguments, index));
    }
    return result;
}

std::string arg(int count, const char *const *arguments, std::int64_t index) {
    const char *value = argument_pointer(count, arguments, index);
    if (value == nullptr) {
        throw SetupError(std::make_error_code(std::errc::result_out_of_range),
                         "argument index out of range");
    }
    return value;
}

std::string arg(int count, const char *const *arguments, std::int64_t index,
                std::string_view fallback) {
    const char *value = argument_pointer(count, arguments, index);
    return value == nullptr ? detail::argument_text(fallback) : std::string{value};
}

namespace {

std::size_t bounded_distance(std::int64_t value, std::size_t limit) noexcept {
    const auto bits = static_cast<std::uint64_t>(value);
    // Unsigned magnitude also represents INT64_MIN.
    const auto magnitude = value < 0 ? std::uint64_t{0} - bits : bits;
    return magnitude >= limit ? limit : static_cast<std::size_t>(magnitude);
}

std::string map_ascii_case(std::string_view text, char first, char last, char replacement) {
    auto result = detail::argument_text(text);
    for (auto &byte : result) {
        if (byte >= first && byte <= last) {
            byte = static_cast<char>(byte - first + replacement);
        }
    }
    return result;
}

bool has_ascii_shape(std::string_view text, unsigned char first) noexcept {
    return !text.empty() && std::all_of(text.begin(), text.end(), [first](unsigned char byte) {
        return byte >= first && byte <= 0x7e;
    });
}

} // namespace

std::int64_t strlen(std::string_view text) { return detail::collection_count(text.size()); }

std::string substring(std::string_view text, std::int64_t offset,
                      std::optional<std::int64_t> length) {
    const auto distance = bounded_distance(offset, text.size());
    const auto start = offset < 0 ? text.size() - distance : distance;
    const auto remaining = text.size() - start;
    auto count = remaining;
    if (length) {
        const auto bound = bounded_distance(*length, remaining);
        count = *length < 0 ? remaining - bound : bound;
    }
    return count == 0 ? std::string{} : std::string(text.data() + start, count);
}

std::string toupper(std::string_view text) { return map_ascii_case(text, 'a', 'z', 'A'); }

std::string tolower(std::string_view text) { return map_ascii_case(text, 'A', 'Z', 'a'); }

bool is_print(std::string_view text) noexcept { return has_ascii_shape(text, 0x20); }

bool is_graph(std::string_view text) noexcept { return has_ascii_shape(text, 0x21); }

Array<std::string> read(std::string_view source) {
    Array<std::string> result;
    std::size_t begin = 0;
    while (begin < source.size()) {
        const auto delimiter = source.find('\0', begin);
        const auto end = delimiter == std::string_view::npos ? source.size() : delimiter;
        result.emplace(detail::collection_count(result.size()),
                       std::string(source.substr(begin, end - begin)));
        if (delimiter == std::string_view::npos) {
            break;
        }
        begin = delimiter + 1;
    }
    return result;
}

Array<std::string> read(int fd) {
    LineReader reader(fd);
    return reader.read();
}

void LineReader::refill(std::string_view operation) {
    end_ = platform::read_some(fd_, buffer_.data(), buffer_.size(), operation);
    begin_ = 0;
    eof_ = end_ == 0;
}

bool LineReader::readline(std::string &target) {
    std::string line;
    while (true) {
        const auto *begin = buffer_.data() + begin_;
        const auto *end = buffer_.data() + end_;
        const auto *newline = std::find(begin, end, '\n');
        line.append(begin, static_cast<std::size_t>(newline - begin));
        begin_ = static_cast<std::size_t>(newline - buffer_.data());
        if (newline != end) {
            ++begin_;
            target = std::move(line);
            return true;
        }
        if (eof_) {
            const bool found = !line.empty();
            target = std::move(line);
            return found;
        }
        refill("read command output");
    }
}

Array<std::string> LineReader::read() {
    std::string bytes;
    while (true) {
        bytes.append(buffer_.data() + begin_, end_ - begin_);
        begin_ = end_;
        if (eof_) {
            return sash::read(bytes);
        }
        refill("read NUL records");
    }
}

namespace detail {

void check_key(std::int64_t index) {
    if (index < 0) {
        throw SetupError(std::make_error_code(std::errc::invalid_argument),
                         "array index must be nonnegative");
    }
}

std::int64_t collection_count(std::size_t size) {
    if (size > static_cast<std::uint64_t>(std::numeric_limits<std::int64_t>::max())) {
        throw SetupError(std::make_error_code(std::errc::value_too_large),
                         "collection length exceeds integer capacity");
    }
    return static_cast<std::int64_t>(size);
}

std::string argument_text(std::string_view value) {
    return value.empty() ? std::string{} : std::string(value.data(), value.size());
}

std::string argument_text(std::int64_t value) { return std::to_string(value); }

[[noreturn]] void required_failure(std::string_view message) {
    throw CommandError(2, "required value: " + argument_text(message));
}

void consume_lines(int fd, const std::function<void(LineReader &)> &body) {
    LineReader reader(fd);
    body(reader);
}

ProcessResult read_from(std::string_view executable, const std::vector<std::string> &arguments,
                        const std::function<void(LineReader &)> &body) {
    std::vector<std::string> command;
    try {
        command = command_words(executable, arguments);
    } catch (const SetupError &error) {
        return {std::string(executable), error};
    }
    return platform::read_from(command, [&](int fd) { consume_lines(fd, body); });
}

PipelineResult read_pipeline(const std::vector<Command> &stages,
                             const std::function<void(LineReader &)> &body) {
    return consume_pipeline(stages, [&](int fd) { consume_lines(fd, body); });
}

std::string capture_stdout(const std::function<void()> &body) {
    platform::StdoutCapture capture;
    body();
    return capture.finish();
}

int report_error(std::string_view program, std::string_view diagnostic, int status) noexcept {
    return platform::report_error(program, diagnostic, status);
}

} // namespace detail
} // namespace sash
