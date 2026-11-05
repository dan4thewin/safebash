#include "sash/runtime.hpp"

#include <cstdint>
#include <string>

namespace rt = sash;

static std::int64_t saved;
static std::string pending;

void interrupt_cleanup() {
    saved = 23;
    rt::echo({"private"});
    rt::exit_program(256);
}

void exit_handler(const rt::Shutdown &shutdown) {
    rt::echo({"EXIT: incoming " + std::to_string(shutdown.status)});
    pending = rt::capture_stdout(interrupt_cleanup);
}

void error_handler(const rt::Shutdown &shutdown) {
    (void)rt::try_echo({"ERR: incoming " + std::to_string(shutdown.status)});
    (void)rt::try_echo({"ERR: saved " + std::to_string(saved) + ", pending " + pending});
}

void program() {
    pending = "initial";
    saved = 17;
    rt::echo({"main: saved " + std::to_string(saved)});
}

int main(int argc, char **argv) {
    return rt::run(argc > 0 ? argv[0] : "terminal_report", program, exit_handler, error_handler);
}
