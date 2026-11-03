#include "sash/runtime.hpp"

#include <string>

namespace rt = sash;

void write_records(std::string_view directory) {
    rt::checkpoint();
    rt::execute("/usr/bin/printf", {"%s\\0", "Ada"});
    rt::checkpoint();
    rt::redirected({rt::Redirect::write(std::string(directory) + "/nested")},
                   [] { rt::execute("/usr/bin/printf", {"%s\\0", "Grace"}); });
    rt::checkpoint();
    rt::execute("/usr/bin/printf", {"%s\\0", "Edsger"});
}

void write_merged() {
    rt::checkpoint();
    rt::execute("/usr/bin/printf", {"%s\\0", "out"});
    rt::checkpoint();
    rt::redirected({rt::Redirect::duplicate(1, 2)},
                   [] { rt::execute("/usr/bin/printf", {"%s\\0", "err"}); });
}

void write_unmerged() {
    rt::checkpoint();
    rt::execute("/usr/bin/printf", {"%s\\0", "out"});
    rt::checkpoint();
    rt::redirected({rt::Redirect::duplicate(1, 2)}, [] { rt::echo({"unmerged:", "stderr"}); });
}

void write_reserved() {
    rt::checkpoint();
    rt::redirected({rt::Redirect::duplicate(1, 3)},
                   [] { rt::execute("/usr/bin/printf", {"%s\\0", "reserved"}); });
}

void read_records(std::string_view path, rt::Array<std::string> &records) {
    rt::checkpoint();
    rt::redirected({rt::Redirect::read(path)}, [&] { records = rt::read(); });
    const auto names = rt::compile_regex(R"(^[A-Za-z\-]+$)");
    for (const auto value : rt::values(records)) {
        rt::checkpoint();
        if (!rt::match(value, names)) {
            throw rt::CommandError(1, "attest: records failed the gate");
        }
    }
}

void collect_files(std::string_view directory, rt::Array<std::string> &records) {
    rt::checkpoint();
    const std::string prefix = std::string(directory) + "/";
    rt::redirected({rt::Redirect::write(prefix + "records")}, [&] { write_records(directory); });
    rt::checkpoint();
    rt::redirected({rt::Redirect::append(prefix + "records")},
                   [] { rt::execute("/usr/bin/printf", {"%s\\0", "Barbara"}); });
    rt::checkpoint();
    rt::redirected({rt::Redirect::write(prefix + "merged"), rt::Redirect::duplicate(2, 1)},
                   write_merged);
    rt::checkpoint();
    rt::redirected({rt::Redirect::duplicate(2, 1), rt::Redirect::write(prefix + "unmerged")},
                   write_unmerged);
    rt::checkpoint();
    rt::redirected({rt::Redirect::write(prefix + "descriptor", 3)}, write_reserved);
    read_records(prefix + "records", records);
    rt::echo({"records:", std::to_string(rt::len(records))});
    for (const auto value : rt::values(records)) {
        rt::checkpoint();
        rt::echo({"record:", value});
    }
    rt::Array<std::string> other;
    read_records(prefix + "nested", other);
    rt::echo({"nested:", rt::join(other, ",")});
    read_records(prefix + "merged", other);
    rt::echo({"merged:", rt::join(other, ",")});
    read_records(prefix + "unmerged", other);
    rt::echo({"unmerged:", rt::join(other, ",")});
    read_records(prefix + "descriptor", other);
    rt::echo({"descriptor:", rt::join(other, ",")});
    bool caught = false;
    rt::checkpoint();
    try {
        rt::redirected(
            {rt::Redirect::write(prefix + "failure", 2), rt::Redirect::read(prefix + "missing")},
            [] { rt::execute("/usr/bin/printf", {"unreachable\n"}); });
    } catch (const rt::SetupError &error) {
        if (error.code() != std::errc::no_such_file_or_directory) {
            throw;
        }
        caught = true;
    }
    if (caught) {
        rt::echo({"caught:", "missing input"});
    } else {
        rt::echo({"unexpected:", "input"});
    }
    rt::echo({"restored:", "stdout"});
}

void program(int count, const char *const *arguments) {
    rt::checkpoint();
    const auto workdir = rt::arg(count, arguments, 1);
    if (!rt::match(workdir, rt::compile_regex(R"(^[/A-Za-z0-9._\-]+$)"))) {
        throw rt::CommandError(1, "attest: directory failed the gate");
    }
    rt::Array<std::string> records;
    collect_files(workdir, records);
}

int main(int count, char **arguments) {
    return rt::run(count > 0 ? arguments[0] : "file_report", {3},
                   [&] { program(count, arguments); });
}
