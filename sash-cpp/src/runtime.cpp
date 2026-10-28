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

ProcessResult try_execute(std::string_view executable, const std::vector<std::string> &arguments) {
    std::vector<std::string> command;
    command.reserve(arguments.size() + 1);
    command.emplace_back(executable);
    command.insert(command.end(), arguments.begin(), arguments.end());
    for (std::size_t i = 0; i < command.size(); ++i) {
        if (command[i].find('\0') != std::string::npos) {
            const auto context = i == 0
                                     ? "execute: executable contains NUL"
                                     : "execute: argument " + std::to_string(i) + " contains NUL";
            return {std::move(command[0]),
                    SetupError(std::make_error_code(std::errc::invalid_argument), context)};
        }
    }
    try {
        return platform::execute(command);
    } catch (const SetupError &error) {
        return {std::move(command[0]), error};
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
