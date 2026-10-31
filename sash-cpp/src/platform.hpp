#ifndef SASH_CPP_PLATFORM_HPP
#define SASH_CPP_PLATFORM_HPP

#include <functional>
#include <string>
#include <string_view>
#include <system_error>
#include <vector>

namespace sash {
struct ProcessResult;
struct PipelineResult;
} // namespace sash

namespace sash::platform {

std::error_code write_all(int fd, std::string_view bytes) noexcept;
int report_error(std::string_view program, std::string_view diagnostic, int status) noexcept;
ProcessResult execute(std::vector<std::string> &command);
ProcessResult read_from(std::vector<std::string> &command, const std::function<void(int)> &body);
PipelineResult read_pipeline(std::vector<std::vector<std::string>> &commands,
                             const std::function<void(int)> &body);
std::size_t read_some(int fd, char *buffer, std::size_t capacity);

class FileDescriptor {
  public:
    explicit FileDescriptor(int fd = -1) noexcept;
    ~FileDescriptor() noexcept;
    FileDescriptor(const FileDescriptor &) = delete;
    FileDescriptor &operator=(const FileDescriptor &) = delete;
    FileDescriptor(FileDescriptor &&other) noexcept;
    FileDescriptor &operator=(FileDescriptor &&) = delete;

    [[nodiscard]] int get() const noexcept { return fd_; }
    void close();

  private:
    int fd_;
};

struct Pipe {
    FileDescriptor reader;
    FileDescriptor writer;
};

namespace os {
FileDescriptor capture_storage();
Pipe create_pipe(std::string_view operation);
char **environment() noexcept;
} // namespace os

class StdoutCapture {
  public:
    StdoutCapture();
    ~StdoutCapture() noexcept;
    StdoutCapture(const StdoutCapture &) = delete;
    StdoutCapture &operator=(const StdoutCapture &) = delete;

    std::string finish();

  private:
    std::error_code restore() noexcept;

    int original_flags_;
    FileDescriptor storage_;
    FileDescriptor saved_stdout_;
    bool active_ = false;
};

} // namespace sash::platform

#endif
