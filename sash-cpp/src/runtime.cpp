#include "sash/runtime.hpp"

#include "platform.hpp"

namespace sash {

CommandError::CommandError(int status, std::string diagnostic)
    : std::runtime_error(std::move(diagnostic)), status_(status) {}

SetupError::SetupError(std::error_code error, std::string_view operation)
    : std::system_error(error, std::string(operation)) {}

void check(const CommandResult &result) {
    if (!result.success()) {
        throw CommandError(result.status, result.diagnostic);
    }
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
    try {
        const char *begin = pattern.empty() ? "" : pattern.data();
        return std::regex(begin, begin + pattern.size(), std::regex_constants::ECMAScript);
    } catch (const std::regex_error &error) {
        throw CommandError(2, std::string("regex: ") + error.what());
    }
}

namespace detail {

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
