#include "sash/runtime.hpp"

#include <cstdint>
#include <map>
#include <string>

namespace rt = sash;

void greet(const std::string &name) { rt::write("Hello, " + name); }

void program() {
    std::map<std::int64_t, std::string> names{{2, "Ada"}, {7, "Grace"}};
    const auto wanted = rt::compile_regex("^A");

    for (const auto &entry : names) {
        const auto &name = entry.second;
        if (std::regex_search(name, wanted)) {
            const auto message = rt::capture_stdout([&] { greet(name); });
            rt::echo({message + "!"});
        }
    }
}

int main(int argc, char **argv) { return rt::run(argc > 0 ? argv[0] : "greetings", program); }
