#include "sash/runtime.hpp"

#include <string>

namespace rt = sash;

void show_option(std::string_view text) {
    rt::checkpoint();
    const auto bytes = rt::strlen(text);
    const auto raw_key = rt::substring(text, 0, 5);
    const auto key = rt::tolower(raw_key);
    const auto separator = rt::substring(text, 5, 1);
    if (key != "count" || separator != "=") {
        rt::write("expected COUNT=VALUE\n", 2);
        rt::exit_program(2);
    }
    const auto value = rt::substring(text, 6);
    const auto integer = rt::int_required(value, [] { return "count must be an integer"; });
    rt::echo({"input: " + std::string(text) + "; bytes: " + std::to_string(bytes)});
    rt::echo({"key: " + key + "; upper: " + rt::toupper(key)});
    rt::echo({"value: [" + value + "]; number: " + std::to_string(integer)});
    rt::echo(
        {"head: [" + rt::substring(text, 0, 6) + "]; empty: [" + rt::substring(text, 6, 0) + "]"});
}

void program(int count, const char *const *arguments) {
    rt::checkpoint();
    const auto option = rt::arg(count, arguments, 1, "COUNT=08");
    if (!rt::is_graph(option)) {
        throw rt::CommandError(1, "attest: option failed the gate");
    }
    show_option(option);
    const rt::Array<std::string> words{{0, "MiXeD"}, {1, "two words"}, {2, ""}};
    for (const auto word : rt::values(words)) {
        rt::checkpoint();
        const std::string printable = rt::is_print(word) ? "yes" : "no";
        const std::string graphical = rt::is_graph(word) ? "yes" : "no";
        rt::echo({"cell: [" + word + "]; lower: [" + rt::tolower(word) + "]; upper: [" +
                  rt::toupper(word) + "]; bytes: " + std::to_string(rt::strlen(word)) +
                  "; print: " + printable + "; graph: " + graphical});
    }
}

int main(int count, char **arguments) {
    return rt::run(count > 0 ? arguments[0] : "string_report", [&] { program(count, arguments); });
}
