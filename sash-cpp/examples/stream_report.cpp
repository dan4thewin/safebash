#include "sash/runtime.hpp"

#include <cstdint>
#include <map>
#include <string>

namespace rt = sash;

void worker_label(const std::string &name) { rt::execute("/usr/bin/printf", {"worker:%s", name}); }

void collect_workers(std::map<std::int64_t, std::string> &workers) {
    const auto valid_record = rt::compile_regex("^[A-Za-z]+$");
    const auto worker_name = rt::compile_regex("^[A-Z][a-z]+$");
    std::int64_t seen = 0;
    std::int64_t kept = 0;
    rt::read_from("/usr/bin/printf", {"Ada\nGrace\nguest\nEdsger\n"}, [&](rt::LineReader &input) {
        std::string person;
        while (input.readline(person)) {
            if (!std::regex_search(person, valid_record)) {
                throw rt::CommandError(2, "worker record must contain only ASCII letters");
            }
            ++seen;
            if (std::regex_search(person, worker_name)) {
                workers[seen] = person;
                ++kept;
                const auto label = rt::capture_stdout([&] { worker_label(person); });
                rt::echo({"read " + std::to_string(seen) + ": " + label});
            } else {
                rt::echo({"skip " + std::to_string(seen) + ": " + person});
            }
        }
    });
    rt::echo({"seen " + std::to_string(seen) + "; kept " + std::to_string(kept)});
}

void program() {
    std::map<std::int64_t, std::string> workers;
    collect_workers(workers);
    for (const auto &entry : workers) {
        rt::echo({"saved: " + entry.second});
    }
    rt::echo({"complete"});
}

int main(int argc, char **argv) { return rt::run(argc > 0 ? argv[0] : "stream_report", program); }
