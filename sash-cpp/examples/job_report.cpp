#include "sash/runtime.hpp"

#include <cstdint>
#include <map>
#include <string>

namespace rt = sash;

void worker_label(const std::string &name) { rt::execute("/usr/bin/printf", {"worker:%s", name}); }

void program() {
    std::map<std::int64_t, std::string> people{
        {2, "Ada"}, {7, "Grace"}, {11, "Edsger"}, {20, "guest"}};
    const auto valid_name = rt::compile_regex("^[A-Z][a-z]+$");
    std::int64_t rounds = 2;
    while (rounds > 0) {
        rt::echo({"round", std::to_string(rounds)});
        for (const auto &entry : people) {
            const auto &person = entry.second;
            if (std::regex_search(person, valid_name)) {
                const auto label = rt::capture_stdout([&] { worker_label(person); });
                if (rt::try_execute("/bin/test", {person, "=", "Grace"}).success()) {
                    rt::echo({label + ": queued"});
                } else {
                    rt::echo({label + ": ready"});
                }
                (void)rt::try_execute("/bin/test", {person, "=", "Ada"});
            } else {
                rt::echo({"skip:" + person});
            }
        }
        --rounds;
    }
    rt::echo({"complete"});
}

int main(int argc, char **argv) { return rt::run(argc > 0 ? argv[0] : "job_report", program); }
