#include <cerrno>
#include <csignal>
#include <cstdio>
#include <cstdlib>
#include <sys/prctl.h>
#include <sys/wait.h>
#include <unistd.h>

int main(int argc, char **argv) {
    if (argc < 2) {
        std::fprintf(stderr, "usage: error-supervisor COMMAND [ARGUMENT...]\n");
        return 2;
    }
    if (std::signal(SIGCHLD, SIG_DFL) == SIG_ERR || ::prctl(PR_SET_CHILD_SUBREAPER, 1) < 0) {
        std::perror("error-supervisor: enable child reaping");
        return 2;
    }
    const auto child = ::fork();
    if (child < 0) {
        std::perror("error-supervisor: fork");
        return 2;
    }
    if (child == 0) {
        ::execvp(argv[1], argv + 1);
        std::perror("error-supervisor: exec");
        std::_Exit(127);
    }
    int result = 2;
    bool completed = false;
    while (true) {
        int status;
        const auto pid = ::waitpid(-1, &status, 0);
        if (pid < 0) {
            if (errno == EINTR) {
                continue;
            }
            if (errno == ECHILD) {
                break;
            }
            std::perror("error-supervisor: wait");
            return 2;
        }
        if (pid == child) {
            result = WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
            completed = true;
        }
    }
    if (!completed) {
        std::fprintf(stderr, "error-supervisor: missing command status\n");
        return 2;
    }
    return result;
}
