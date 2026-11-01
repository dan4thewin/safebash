#include "sash/runtime.hpp"

#include <cstdint>
#include <cstdio>
#include <map>
#include <string>

namespace rt = sash;

using Workers = std::map<std::int64_t, std::string>;

void collect_workers(Workers &workers) {
    rt::checkpoint();
    const auto valid_name = rt::compile_regex("^[A-Za-z]+$");
    std::int64_t seen = 0;
    rt::checkpoint();
    rt::read_from("/usr/bin/printf", {"Ada\nGrace\n"}, [&](rt::LineReader &input) {
        std::string person;
        while (rt::checkpoint(), input.readline(person)) {
            if (!std::regex_search(person, valid_name)) {
                throw rt::CommandError(2, "worker record must contain only ASCII letters");
            }
            workers[++seen] = person;
        }
    });
    rt::checkpoint();
    rt::execute("/usr/bin/printf", {"collected:%s", std::to_string(seen)});
    rt::checkpoint();
}

std::int64_t finished_status() {
    rt::checkpoint();
    rt::echo({"result discarded"});
    rt::checkpoint();
    return 7;
}

void finish_program(const Workers &workers, const rt::Shutdown &shutdown) {
    try {
        (void)rt::try_echo({"END: incoming " + std::to_string(shutdown.status)});
        try {
            rt::execute("/usr/bin/false");
        } catch (const rt::CommandError &error) {
            (void)rt::try_echo({"END: caught " + std::to_string(error.status())});
        }
        (void)rt::try_execute("/usr/bin/false");
        (void)rt::try_echo({"END: discarded"});
        for (const auto &entry : workers) {
            (void)rt::try_echo({"END: saved " + entry.second});
        }
    } catch (const rt::SetupError &error) {
        (void)std::fprintf(stderr, "lifecycle_report: END: %s\n", error.what());
        rt::exit_program(2);
    } catch (const std::bad_alloc &) {
        (void)std::fputs("lifecycle_report: END: out of memory\n", stderr);
        rt::exit_program(2);
    } catch (const std::length_error &) {
        (void)std::fputs("lifecycle_report: END: value exceeds string capacity\n", stderr);
        rt::exit_program(2);
    }
}

int main(int argc, char **argv) {
    Workers workers;
    return rt::run(
        argc > 0 ? argv[0] : "lifecycle_report",
        [&] {
            const auto summary = rt::capture_stdout([&] { collect_workers(workers); });
            rt::echo({"capture: " + summary});
            (void)finished_status();
            rt::echo({"body complete"});
            rt::exit_program(0);
        },
        [&](const rt::Shutdown &shutdown) { finish_program(workers, shutdown); });
}
