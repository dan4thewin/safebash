#include "sash/runtime.hpp"

#include <cstdint>
#include <map>
#include <string>

namespace rt = sash;

std::string worker_summary() {
    return rt::capture_pipeline({{"/usr/bin/printf", {"Grace\nAda\nGrace\nEdsger\nAda\n"}},
                                 {"/usr/bin/env", {"LC_ALL=C", "/usr/bin/sort"}},
                                 {"/usr/bin/uniq", {}},
                                 {"/usr/bin/paste", {"-sd", ","}},
                                 {"/usr/bin/tr", {"-d", "\\n"}}});
}

void framed_summary() {
    rt::write("[");
    rt::write(worker_summary());
    rt::write("]");
}

void program() {
    std::map<std::int64_t, std::string> summaries;
    std::int64_t round = 0;
    while (round < 2) {
        ++round;
        const auto summary = worker_summary();
        summaries[round] = summary;
        const auto framed = rt::capture_stdout(framed_summary);
        rt::echo({"round " + std::to_string(round) + ": " + summary});
        rt::echo({"framed " + std::to_string(round) + ": " + framed});
    }
    for (const auto &entry : summaries) {
        rt::echo({"saved: " + entry.second});
    }
    const auto probe = rt::try_capture_pipeline({{"/bin/false", {}}, {"/bin/cat", {}}});
    if (!probe.output.empty()) {
        throw rt::CommandError(1, "failed probe must not produce output");
    }
    if (probe.pipeline.success()) {
        rt::echo({"unexpected capture success"});
    } else {
        rt::echo({"failed capture detected"});
    }
    rt::echo({"complete"});
}

int main(int argc, char **argv) { return rt::run(argc > 0 ? argv[0] : "capture_report", program); }
