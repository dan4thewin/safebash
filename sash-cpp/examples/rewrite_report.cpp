#include "sash/runtime.hpp"

#include <string>

namespace rt = sash;

void show_rewrites(std::string_view text) {
    rt::checkpoint();
    const auto assignment = rt::compile_regex("([A-Za-z]+)=([0-9]+)");
    rt::echo({"input:", "[" + std::string(text) + "]"});
    if (rt::match(text, assignment)) {
        rt::echo({"found: yes"});
    } else {
        rt::echo({"found: no"});
    }
    rt::echo({"first:", "[" + rt::sub(text, assignment, "$1:[$2]") + "]"});
    rt::echo({"all:", "[" + rt::gsub(text, assignment, "$1:[$2]") + "]"});
    rt::echo({"whole:", "[" + rt::sub(text, assignment, "<$&>") + "]"});
    rt::echo(
        {"literal:", "[" + rt::sub(text, rt::compile_regex("^[A-Za-z]+"), R"(cost=$$\)") + "]"});
    rt::echo({"digits:", "[" + rt::gsub(text, rt::compile_regex("[0-9]"), "*") + "]"});
    rt::echo({"missing:", "[" + rt::gsub(text, rt::compile_regex("NEVER"), "gone") + "]"});
    const rt::Array<std::string> samples{{2, "port=02"}, {9, "RATE=0"}};
    for (const auto value : rt::values(samples)) {
        rt::checkpoint();
        rt::echo({"cell:", "[" + rt::gsub(value, assignment, "$2/$1") + "]"});
    }
}

void program(int count, const char *const *arguments) {
    rt::checkpoint();
    const auto input = rt::arg(count, arguments, 1, "COUNT=08;RATE=7");
    if (!rt::is_print(input)) {
        throw rt::CommandError(1, "attest: input failed the gate");
    }
    show_rewrites(input);
}

int main(int count, char **arguments) {
    return rt::run(count > 0 ? arguments[0] : "rewrite_report", [&] { program(count, arguments); });
}
