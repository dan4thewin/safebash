#include "sash/runtime.hpp"

#include <cerrno>
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

    void pipeline(const rt::PipelineResult &result) const {
        if (handling == Handling::Checked) {
            rt::check(result);
        } else if (handling == Handling::Discarded) {
            (void)result;
        } else {
            for (const auto &failure : result.setup_errors) {
                say("pipeline setup at stage " + std::to_string(failure.stage) + ": " +
                    std::to_string(failure.error.code().value()) + ": " + failure.error.what());
            }
            for (std::size_t i = 0; i < result.stages.size(); ++i) {
                const auto prefix = "stage " + std::to_string(i) + ": ";
                const auto &stage = result.stages[i];
                if (!stage) {
                    say(prefix + "not started");
                } else if (const auto *exit = std::get_if<rt::ProcessExit>(&stage->outcome)) {
                    say(prefix + "exit " + std::to_string(exit->status));
                } else if (const auto *signal = std::get_if<rt::ProcessSignal>(&stage->outcome)) {
                    say(prefix + "signal " + std::to_string(signal->number));
                } else {
                    const auto &error = std::get<rt::SetupError>(stage->outcome);
                    say(prefix + "setup " + std::to_string(error.code().value()) + ": " +
                        error.what());
                }
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

void prepare_pipeline_sigpipe() {
    sigset_t signals;
    if (std::signal(SIGPIPE, SIG_DFL) == SIG_ERR || (::sigemptyset)(&signals) < 0 ||
        (::sigaddset)(&signals, SIGPIPE) < 0 || ::sigprocmask(SIG_UNBLOCK, &signals, nullptr) < 0) {
        throw rt::SetupError({errno, std::generic_category()}, "prepare pipeline SIGPIPE");
    }
}

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
    } else if (name == "pipeline" || name == "pipeline-empty-output" || name == "pipeline-empty" ||
               name == "pipeline-upstream" || name == "pipeline-rightmost" ||
               name == "pipeline-signal" || name == "pipeline-missing" || name == "pipeline-nul" ||
               name == "pipeline-early" || name == "pipeline-record") {
        demo.operation([&] {
            prepare_pipeline_sigpipe();
            std::vector<rt::Command> commands{{demo.helper, {"records", "0"}},
                                              {demo.helper, {"relay", "0"}},
                                              {demo.helper, {"relay", "0"}}};
            if (name == "pipeline-empty") {
                commands.clear();
            } else if (name == "pipeline-empty-output" || name == "pipeline-signal") {
                commands[0].arguments = {"exit", "0"};
                if (name == "pipeline-signal") {
                    commands[1].arguments = {"signal", std::to_string(SIGTERM)};
                }
            } else if (name == "pipeline-upstream" || name == "pipeline-rightmost") {
                commands[0].arguments[1] = "7";
                if (name == "pipeline-rightmost") {
                    commands[1].arguments[1] = "3";
                    commands[2].arguments[1] = "2";
                }
            } else if (name == "pipeline-missing") {
                commands[0].arguments = {"flood"};
                commands[1].executable += ".missing";
            } else if (name == "pipeline-nul") {
                commands[1].arguments = {"args", std::string("one\0two", 7)};
            } else if (name == "pipeline-early" || name == "pipeline-record") {
                commands[0].arguments = {name == "pipeline-early" ? "flood" : "flood-records"};
            }
            const auto result = rt::try_read_pipeline(commands, [&](rt::LineReader &input) {
                if (name == "pipeline-early") {
                    return;
                }
                std::string line;
                while (input.readline(line)) {
                    if (name == "pipeline-record") {
                        return;
                    }
                }
            });
            demo.pipeline(result);
        });
    } else if (name == "pipeline-capture" || name == "pipeline-capture-empty" ||
               name == "pipeline-capture-upstream" || name == "pipeline-capture-missing" ||
               name == "pipeline-capture-read") {
        demo.operation([&] {
            prepare_pipeline_sigpipe();
            std::vector<rt::Command> commands{{demo.helper, {"records", "0"}},
                                              {demo.helper, {"relay"}},
                                              {demo.helper, {"relay"}}};
            if (name == "pipeline-capture-empty") {
                commands[0].arguments = {"exit", "0"};
            } else if (name == "pipeline-capture-upstream") {
                commands[0].arguments[1] = "7";
            } else if (name == "pipeline-capture-missing") {
                commands[0].arguments = {"flood"};
                commands[1].executable += ".missing";
            } else if (name == "pipeline-capture-read") {
                commands = {{demo.helper, {"binary"}}};
            }
            const auto captured = rt::try_capture_pipeline(commands);
            demo.pipeline(captured.pipeline);
            demo.say("captured bytes: " + std::to_string(captured.output.size()));
        });
    } else if (name == "pipeline-capture-checked") {
        std::string target = "unchanged";
        demo.operation([&] {
            target =
                rt::capture_pipeline({{demo.helper, {"records", "7"}}, {demo.helper, {"relay"}}});
        });
        demo.say("target: " + target);
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
