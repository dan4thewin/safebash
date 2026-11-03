#include "sash/runtime.hpp"

#include <string>

namespace rt = sash;

void collect_records(std::string_view text, rt::Array<std::string> &saved) {
    rt::checkpoint();
    const auto encoded = rt::gsub(text, rt::compile_regex("[|]"), std::string_view("\0", 1));
    const auto fields = rt::read(encoded);
    rt::echo({"split:", std::to_string(rt::len(fields))});
    for (const auto value : rt::values(fields)) {
        rt::checkpoint();
        rt::echo({"field:", "<" + value + ">"});
    }
    rt::set_cell(saved, 27, "stale");
    rt::read_from("/usr/bin/printf", {"%s\\0", "Edsger", "Barbara"}, [&](rt::LineReader &input) {
        rt::checkpoint();
        saved = input.read();
    });
    const auto letters = rt::compile_regex("^[A-Za-z]+$");
    for (const auto value : rt::values(saved)) {
        rt::checkpoint();
        if (!rt::match(value, letters)) {
            throw rt::CommandError(1, "attest: saved failed the gate");
        }
    }
    rt::echo({"stream:", std::to_string(rt::len(saved))});
    for (const auto value : rt::values(saved)) {
        rt::checkpoint();
        rt::echo({"saved:", "<" + value + ">"});
    }
    rt::echo({"joined:", "[" + rt::join(saved, ",") + "]"});
    saved = rt::read(std::string_view{});
    rt::echo({"cleared:", std::to_string(rt::len(saved))});
}

void program(int count, const char *const *arguments) {
    rt::checkpoint();
    const auto input = rt::arg(count, arguments, 1, "Ada||Grace|Linus|");
    if (!rt::is_print(input)) {
        throw rt::CommandError(1, "attest: input failed the gate");
    }
    rt::Array<std::string> records;
    collect_records(input, records);
}

int main(int count, char **arguments) {
    return rt::run(count > 0 ? arguments[0] : "record_report", [&] { program(count, arguments); });
}
