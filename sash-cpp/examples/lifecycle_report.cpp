#include "sash/runtime.hpp"

#include <cstdint>
#include <map>
#include <string>

namespace rt = sash;

using Workers = std::map<std::int64_t, std::string>;

static Workers workers;
static std::string summary;

void collect_workers(Workers &workers) {
    const auto valid_name = rt::compile_regex("^[A-Za-z]+$");
    std::int64_t seen = 0;
    rt::read_from("/usr/bin/printf", {"Ada\nGrace\n"}, [&](rt::LineReader &input) {
        std::string person;
        while (input.readline(person)) {
            if (!std::regex_search(person, valid_name)) {
                throw rt::CommandError(2, "worker record must contain only ASCII letters");
            }
            workers[++seen] = person;
        }
    });
    rt::execute("/usr/bin/printf", {"collected:%s", std::to_string(seen)});
}

std::int64_t finished_status() {
    rt::echo({"result discarded"});
    return 7;
}

void exit_handler(const rt::Shutdown &shutdown) {
    rt::echo({"EXIT: incoming " + std::to_string(shutdown.status)});
    try {
        rt::execute("/usr/bin/false");
    } catch (const rt::CommandError &error) {
        rt::echo({"EXIT: caught " + std::to_string(error.status())});
    }
    (void)rt::try_execute("/usr/bin/false");
    rt::echo({"EXIT: discarded"});
    for (const auto &entry : workers) {
        rt::echo({"EXIT: saved " + entry.second});
    }
}

void error_handler(const rt::Shutdown &shutdown) {
    (void)rt::try_echo({"ERR: incoming " + std::to_string(shutdown.status)});
}

void program() {
    summary = rt::capture_stdout([] { collect_workers(workers); });
    rt::echo({"capture: " + summary});
    (void)finished_status();
    rt::echo({"body complete"});
    rt::exit_program();
}

int main(int argc, char **argv) {
    return rt::run(argc > 0 ? argv[0] : "lifecycle_report", program, exit_handler, error_handler);
}
