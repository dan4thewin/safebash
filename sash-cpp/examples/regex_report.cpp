#include "sash/runtime.hpp"

#include <string>

namespace rt = sash;

void show_records(std::string_view text) {
    rt::checkpoint();
    const auto assignment = rt::compile_regex("([^;=]+)=([^;]*)");
    rt::echo({"input:", "[" + std::string(text) + "]"});
    rt::Array<std::string> first;
    if (rt::match(text, assignment, first)) {
        rt::echo({"first:", "[" + rt::cell(first, 0) + "];", "key:",
                  "[" + rt::cell(first, 1) + "];", "value:", "[" + rt::cell(first, 2) + "]"});
    } else {
        rt::echo({"first: no match"});
    }
    rt::echo({"scalar:", "[" + rt::match_text(text, assignment) + "]"});
    rt::Array<std::string> pairs;
    if (rt::matchall(text, assignment, pairs)) {
        const auto options = rt::map_pairs_elided(pairs);
        const auto saved_keys = rt::snapshot_keys(options);
        rt::echo(
            {"pairs:", std::to_string(rt::len(pairs)) + ";", "keys:", rt::join(saved_keys, ",")});
        for (const auto key : rt::keys(options)) {
            rt::checkpoint();
            const auto value = rt::cell(options, key);
            const auto integer = rt::int_or(value, 0);
            rt::echo({"option:", key + "=[" + value + "];", "number:", std::to_string(integer)});
        }
    } else {
        rt::echo({"pairs: 0; keys:"});
    }
    rt::Array<std::string> missing{{99, "stale"}};
    if (rt::match(text, rt::compile_regex("NEVER"), missing)) {
        rt::echo({"missing: yes; captures:", std::to_string(rt::len(missing))});
    } else {
        rt::echo({"missing: no; captures:", std::to_string(rt::len(missing))});
    }
    rt::Array<std::string> names;
    if (rt::matchall(text, rt::compile_regex("[A-Z]+"), names)) {
        rt::echo({"group-free:", rt::join(names, ",")});
    } else {
        rt::echo({"group-free:"});
    }
}

void program(int count, const char *const *arguments) {
    rt::checkpoint();
    const auto input = rt::arg(count, arguments, 1, "COUNT=08;EMPTY=;RATE=0x7");
    if (!rt::is_print(input)) {
        throw rt::CommandError(1, "attest: input failed the gate");
    }
    show_records(input);
}

int main(int count, char **arguments) {
    return rt::run(count > 0 ? arguments[0] : "regex_report", [&] { program(count, arguments); });
}
