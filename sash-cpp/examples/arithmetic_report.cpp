#include "sash/runtime.hpp"

#include <string>

namespace rt = sash;

std::int64_t subtotal() {
    rt::checkpoint();
    std::int64_t sum = 0;
    for (std::int64_t i = 1; i <= 3; i = rt::add(i, 1)) {
        rt::checkpoint();
        sum = rt::add(sum, rt::mul(i, 7));
    }
    return sum;
}

void program() {
    rt::checkpoint();
    const std::int64_t total = subtotal();
    const std::int64_t quote = rt::mul(rt::add(total, 1), 3);
    rt::echo({"subtotal: " + std::to_string(total) + "; quote: " + std::to_string(quote)});
    if (quote >= 60) {
        rt::echo({"tier: standard"});
    } else {
        rt::echo({"tier: small"});
    }
    std::int64_t dividend = -17;
    std::int64_t divisor = 5;
    rt::echo({"division: " + std::to_string(rt::div(dividend, divisor)) +
              "; remainder: " + std::to_string(rt::mod(dividend, divisor)) +
              "; negated: " + std::to_string(rt::neg(dividend))});
    rt::echo({"power: " + std::to_string(rt::pow(2, 10)) + "; negative: " +
              std::to_string(rt::pow(2, -3)) + "; zero: " + std::to_string(rt::pow(0, 0))});
    std::int64_t wide = 64;
    std::int64_t signed_value = -9;
    rt::echo({"left: " + std::to_string(rt::shl(1, 63)) +
              "; wide: " + std::to_string(rt::shl(1, wide))});
    rt::echo({"right: " + std::to_string(rt::shr(signed_value, 1)) +
              "; wide: " + std::to_string(rt::shr(signed_value, wide))});
    std::int64_t zero = 0;
    const std::int64_t lazy = true ? 7 : rt::div(10, zero);
    rt::echo({"lazy: " + std::to_string(lazy)});
    std::int64_t count = 3;
    while (count > 0) {
        rt::checkpoint();
        rt::echo({"countdown: " + std::to_string(count)});
        count = rt::sub(count, 1);
    }
}

int main(int argc, char **argv) {
    return rt::run(argc > 0 ? argv[0] : "arithmetic_report", program);
}
