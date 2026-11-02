#include "sash/runtime.hpp"

#include <string>
#include <vector>

namespace rt = sash;

void amend_options(rt::Array<std::string> &first, rt::Array<std::string> &second) {
    rt::checkpoint();
    rt::set_cell(second, 5, rt::cell(first, 5) + "!");
    rt::unset(first, 9);
    rt::set_cell(second, 7, "--verbose");
}

void prepare_options(rt::Array<std::string> &options) {
    rt::checkpoint();
    rt::set_cell(options, 2, "");
    rt::set_cell(options, 5, "Ada Lovelace");
    rt::set_cell(options, 9, "--color");
    amend_options(options, options);
}

void argument_report(const std::vector<std::string> &arguments) {
    rt::checkpoint();
    rt::echo({"arguments: " + std::to_string(arguments.size())});
    std::vector<std::string> command{"argument: <%s>\n"};
    command.insert(command.end(), arguments.begin(), arguments.end());
    rt::checkpoint();
    rt::execute("/usr/bin/printf", command);
}

void program() {
    rt::Array<std::string> options;
    prepare_options(options);
    rt::echo({"options: " + std::to_string(rt::len(options))});
    rt::echo(
        {"missing: <" + rt::default_value(rt::cell(options, 4), [] { return "unset"; }) + ">"});
    rt::echo({"joined: " + rt::join(options, "|")});
    rt::Array<std::int64_t> counts{{2, 0}, {5, 7}};
    rt::echo({"counts: " + std::to_string(rt::len(counts)) +
              "; absent: " + std::to_string(rt::cell(counts, 8))});
    rt::echo({"count values: " + rt::join(counts, ",")});

    rt::Array<std::string> pairs{{0, "color"}, {1, "blue"}, {2, "label"}, {3, "Ada"}, {4, "spare"}};
    auto config = rt::map_pairs_or(pairs, [] { return "z-extra"; });
    const auto saved_keys = rt::snapshot_keys(config);
    rt::unset(config, "label");
    rt::echo({"snapshot: " + rt::join(saved_keys, ",")});
    for (const auto &key : rt::keys(config)) {
        rt::checkpoint();
        rt::echo({"config: " + key + "=" + rt::cell(config, key)});
    }

    std::vector<std::string> arguments{"--name"};
    rt::append_argument(arguments, rt::default_value(rt::cell(options, 5), [] { return "guest"; }));
    rt::append_argument(arguments, rt::default_value(rt::cell(options, 2), [] { return ""; }));
    rt::append_elided(arguments, rt::cell(options, 2));
    rt::append_splat_required(arguments, options, [] { return "expected options"; });
    argument_report(arguments);
    rt::Array<std::string> empty;
    std::vector<std::string> fallback;
    rt::append_splat_or(fallback, empty, [] { return "fallback"; });
    argument_report(fallback);
}

int main(int argc, char **argv) { return rt::run(argc > 0 ? argv[0] : "option_report", program); }
