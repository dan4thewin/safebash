#include "sash/runtime.hpp"

#include <string>

namespace rt = sash;

std::int64_t charge(std::string_view units_text, std::string_view rate_text) {
    rt::checkpoint();
    const auto units = rt::int_required(units_text, [] { return "bad units"; });
    const auto rate = rt::int_required(rate_text, [] { return "bad rate"; });
    return rt::mul(units, rate);
}

void attest_option(std::string_view value, const std::regex &shape) {
    if (!std::regex_match(value.begin(), value.end(), shape)) {
        throw rt::CommandError(1, "attest: numeric option failed the gate");
    }
}

void program(int count, const char *const *arguments) {
    rt::checkpoint();
    const auto shape = rt::compile_regex(R"(^[A-Za-z0-9+\-]*$)");
    const auto quantity = rt::arg(count, arguments, 1, "03");
    attest_option(quantity, shape);
    const auto rate_text = rt::getenv("SASH_CPP_NUMERIC_RATE").value_or("07");
    attest_option(rate_text, shape);
    const auto zero_text = rt::arg(count, arguments, 2, "0");
    attest_option(zero_text, shape);
    const auto spare = rt::arg(count, arguments, 3, "spare");
    attest_option(spare, shape);
    const auto options = rt::argv(count, arguments, 1);
    for (const auto option : rt::values(options)) {
        rt::checkpoint();
        attest_option(option, shape);
    }

    const auto quantity_value = rt::int_or(quantity, 3);
    const auto rate_value = rt::int_or(rate_text, 7);
    const auto amount = charge(std::to_string(quantity_value), std::to_string(rate_value));
    rt::echo(
        {"argc: " + std::to_string(count) + "; user args: " + std::to_string(rt::len(options))});
    rt::echo({"quantity: " + std::to_string(quantity_value) +
              "; rate: " + std::to_string(rate_value) + "; total: " + std::to_string(amount)});
    rt::echo({"optional: " + spare});
    if (rt::is_int(zero_text)) {
        const auto zero = rt::int_required(zero_text, [] { return "bad zero"; });
        rt::echo({"zero: " + std::to_string(zero) + "; valid: yes"});
    } else {
        rt::echo({"zero: invalid; valid: no"});
    }

    const rt::Array<std::string> hints{
        {0, "0xff"}, {1, "bad"}, {2, ""}, {3, "0"}, {4, "9223372036854775808"}};
    rt::echo({"cells: " +
              std::to_string(rt::int_required(rt::cell(hints, 0), [] { return "bad hex"; })) + "," +
              std::to_string(rt::int_or(rt::cell(hints, 1), 11)) + "," +
              std::to_string(rt::int_or(rt::cell(hints, 2), 13)) + "," +
              std::to_string(rt::int_or(rt::cell(hints, 3), 17)) + "," +
              std::to_string(rt::int_or(rt::cell(hints, 4), 19))});
    const std::string partial = "12tail";
    rt::echo({"string: " + rt::default_value(partial, [] { return "unused"; }) +
              "; numeric: " + std::to_string(rt::int_or(partial, 9))});
    for (const auto option : rt::values(options)) {
        rt::checkpoint();
        rt::echo({"option: " + option});
    }
}

int main(int count, char **arguments) {
    return rt::run(count > 0 ? arguments[0] : "numeric_report", [&] { program(count, arguments); });
}
