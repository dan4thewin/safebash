#include "sash/runtime.hpp"

#include <csignal>
#include <cstdlib>
#include <filesystem>
#include <string>
#include <string_view>

namespace rt = sash;

enum class Handling { Checked, Caught, Discarded };

Handling parse_handling(std::string_view value) {
    if (value == "checked") {
        return Handling::Checked;
    }
    if (value == "caught") {
        return Handling::Caught;
    }
    if (value == "discarded") {
        return Handling::Discarded;
    }
    throw rt::CommandError(2, "unknown handling: " + std::string(value));
}

struct Demo {
    Handling handling;
    std::string helper;
    int report_fd;

    void say(std::string_view text) const {
        rt::write(text, report_fd);
        rt::write("\n", report_fd);
    }

    void failure(std::string_view kind, std::string_view message) const {
        if (handling == Handling::Caught) {
            say("caught " + std::string(kind) + ": " + std::string(message));
        }
    }

    void command(const rt::CommandResult &result) const {
        if (handling == Handling::Checked) {
            rt::check(result);
        } else if (handling == Handling::Discarded) {
            (void)result;
        } else if (!result.success()) {
            failure("command " + std::to_string(result.status), result.diagnostic);
        }
    }

    void process(const rt::ProcessResult &result) const {
        if (handling == Handling::Checked) {
            rt::check(result);
        } else if (handling == Handling::Discarded) {
            (void)result;
        } else if (!result.success()) {
            if (const auto *exit = std::get_if<rt::ProcessExit>(&result.outcome)) {
                failure("exit", std::to_string(exit->status));
            } else if (const auto *signal = std::get_if<rt::ProcessSignal>(&result.outcome)) {
                failure("signal", std::to_string(signal->number));
            } else {
                const auto &error = std::get<rt::SetupError>(result.outcome);
                failure("setup " + std::to_string(error.code().value()), error.what());
            }
        }
    }

    template <typename Body> void operation(Body body) const {
        if (handling == Handling::Checked) {
            body();
            return;
        }
        try {
            body();
        } catch (const rt::CommandError &error) {
            failure("command " + std::to_string(error.status()), error.what());
        } catch (const rt::SetupError &error) {
            failure("setup " + std::to_string(error.code().value()), error.what());
        } catch (const std::regex_error &error) {
            failure("regex", error.what());
        } catch (const std::bad_alloc &error) {
            failure("allocation", error.what());
        } catch (const std::length_error &error) {
            failure("length", error.what());
        }
    }
};

void scenario(const Demo &demo, std::string_view name) {
    if (name == "process" || name == "process-closed") {
        demo.process(rt::try_execute(demo.helper, {"exit", "0"}));
    } else if (name == "stream") {
        demo.operation([&] {
            demo.process(rt::try_read_from(demo.helper, {"binary"}, [](rt::LineReader &input) {
                std::string line;
                while (input.readline(line)) {
                }
            }));
        });
    } else if (name == "exit7") {
        demo.process(rt::try_execute(demo.helper, {"exit", "7"}));
    } else if (name == "exit127") {
        demo.process(rt::try_execute(demo.helper, {"exit", "127"}));
    } else if (name == "signal") {
        demo.process(rt::try_execute(demo.helper, {"signal", std::to_string(SIGTERM)}));
    } else if (name == "missing") {
        demo.process(rt::try_execute(demo.helper + ".missing"));
    } else if (name == "permission" || name == "format") {
        const auto directory = std::filesystem::path(demo.helper).parent_path();
        const auto path = directory / (name == "permission" ? "not-executable" : "no-shebang");
        demo.process(rt::try_execute(path.string()));
    } else if (name == "nul-path") {
        demo.process(rt::try_execute(demo.helper + std::string("\0bad", 4), {"exit", "0"}));
    } else if (name == "nul-argument") {
        demo.process(rt::try_execute(demo.helper, {"args", std::string("one\0two", 7)}));
    } else if (name == "bad-write") {
        demo.command(rt::try_write("not written", -1));
    } else if (name == "write") {
        demo.command(rt::try_write("written\n"));
    } else if (name == "echo-closed") {
        demo.command(rt::try_echo({"not written"}));
    } else if (name == "regex") {
        demo.operation([] { (void)rt::compile_regex("["); });
    } else if (name == "regex-raw") {
        demo.operation([] { (void)std::regex("[", std::regex_constants::ECMAScript); });
    } else if (name == "regex-no-match") {
        const auto regex = rt::compile_regex("^Ada$");
        if (!std::regex_search(std::string("guest"), regex)) {
            demo.say("no match is not an error");
        }
    } else if (name == "length") {
        demo.operation([] {
            std::string value;
            value.resize(value.max_size() + 1);
        });
    } else if (name == "bad-alloc") {
        // Exercise the boundary without exhausting the shared development host.
        demo.operation([] { throw std::bad_alloc{}; });
    } else if (name == "capture" || name == "capture-closed" || name == "capture-command" ||
               name == "capture-command-closed") {
        const bool command_fails = name == "capture-command" || name == "capture-command-closed";
        std::string target = "unchanged";
        demo.operation([&] {
            target = rt::capture_stdout([&] {
                rt::write(command_fails ? "partial" : "captured");
                if (command_fails) {
                    rt::execute(demo.helper, {"exit", "7"});
                }
            });
        });
        demo.say("target: " + target);
    } else {
        throw rt::CommandError(2, "unknown scenario: " + std::string(name));
    }
}

int main(int argc, char **argv) {
    return rt::run("error_paths", [&] {
        if (argc != 4) {
            throw rt::CommandError(
                2, "usage: error_paths SCENARIO checked|caught|discarded COMMAND_HELPER");
        }
        const std::string_view name(argv[1]);
        const bool closed_stdout = name == "echo-closed" || name == "capture-closed" ||
                                   name == "capture-command-closed" || name == "process-closed";
        const Demo demo{parse_handling(argv[2]), argv[3], closed_stdout ? 2 : 1};
        if (std::getenv("SASH_CPP_ERROR_TRACE") != nullptr) {
            rt::write("__SASH_CPP_DEMO_BEGIN__\n", 2);
        }
        scenario(demo, name);
        demo.say("continued");
    });
}
