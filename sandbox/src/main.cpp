#include <cstdio>
#include <string>
#include <cstring>
#include <cstdlib>
#include <iostream>
#include <cstddef>

#include <errno.h>

#include <sys/ptrace.h>
#include <sys/reg.h>
#include <signal.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>
#include <sys/user.h>
#include <sys/prctl.h>
#include <fcntl.h>
#include <linux/limits.h>
#include <linux/filter.h>
#include <linux/seccomp.h>
#include <linux/unistd.h>

#ifndef __x86_64__
#error This code relies on the x86_64 calling convention! It wont work on other architectures
#endif

// OMG GNU
#define PTRACE_EVENT_SECCOMP PTRAVE_EVENT_SECCOMP

namespace {

int main_child(int argc, char* argv[]) {
  // TODO: You're meant to check offsetof(struct seccomp_data, arch) too.
  // See pitfalls here: https://www.kernel.org/doc/Documentation/prctl/seccomp_filter.txt

  // This creates a cBPF program that just checks if the syscall is open() and
  // starts ptrace if it is, and allows it if it isn't.
  sock_filter filter[] = {
    BPF_STMT(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, nr)),
    BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, __NR_open, 0, 1),
    BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_TRACE),
    BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ALLOW),
  };
  sock_fprog prog = {
    .len = static_cast<unsigned short>(sizeof(filter)/sizeof(filter[0])),
    .filter = filter,
  };

  // Request that this process be traced.
  ptrace(PTRACE_TRACEME, 0, 0, 0);

  // To avoid the need for CAP_SYS_ADMIN.
  if (prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) == -1) {
    perror("prctl(PR_SET_NO_NEW_PRIVS)");
    return 1;
  }

  // Install the seccomp filter.
  if (prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &prog) == -1) {
    perror("when setting seccomp filter");
    return 1;
  }

  // Send a sigstop signal to this process so it stops. It will be restarted
  // by the parent.
  kill(getpid(), SIGSTOP);

  // Replace this process with the one given on the command line.
  return execvp(argv[1], argv + 1);
}

enum class WaitResult {
  OpenSyscall,
  ProcessExited,
};

#define IS_SECCOMP_EVENT(status) ((status >> 16) == PTRACE_EVENT_SECCOMP)

WaitResult wait_for_open_syscall(pid_t child)
{
  for (;;) {
    std::cout << "Telling child to continue\n";
    // Tell the process to continue execution.
    ptrace(PTRACE_CONT, child, 0, 0);

    std::cout << "Waiting for child event\n";

    // Wait for the child to be stopped by PTRACE_EVENT_SECCOMP.
    int status;
    int r = waitpid(child, &status, __WALL);
    // printf("[waitpid status: 0x%08x]\n", status);

    // Check if child is dead.
    if (WIFSIGNALED(status) || WIFEXITED(status)) {
      return WaitResult::ProcessExited;
    }

    if (r > 0 && WIFSTOPPED(status)) {
      std::cout << "It's stopped\n";
    }

    // Is it our filter for the open syscall?
    if (
      IS_SECCOMP_EVENT(status) &&
      ptrace(PTRACE_PEEKUSER, child, sizeof(long) * ORIG_RAX, 0) == __NR_open
    ) {
      return WaitResult::OpenSyscall;
    } else {
      std::cout << "Got uninteresting event: " << status << "\n";
    }
    // Return 1 if the process exited.

  }
}

// If the `child` process is stopped at an `open()` syscall, get the filename.
void get_open_filename(pid_t child, std::string& filename)
{
  // Read the RDI register. This is the address of the filename string.
  // TODO: Can possibly use PTRACE_GETREGSET here. Might be better?
  const char* child_addr = reinterpret_cast<const char*>(
    ptrace(PTRACE_PEEKUSER, child, sizeof(long) * RDI, 0)
  );

  // Read the string one word at a time.
  filename.clear();
  // filename.reserve(500);

  for (;;) {
    // Read a `long` at `child_addr`.
    long val = ptrace(PTRACE_PEEKTEXT, child, child_addr, NULL);
    if (val == -1) {
      fprintf(stderr, "PTRACE_PEEKTEXT error: %s", strerror(errno));
      exit(1);
    }

    const char* val_str = reinterpret_cast<const char*>(&val);

    for (int i = 0; i < sizeof(long); ++i) {
      if (val_str[i] == '\0') {
        return;
      }
      filename.push_back(val_str[i]);
    }

    child_addr += sizeof(long);
  }
}


void return_error_from_syscall(pid_t child, int errno_) {
  std::cout << "Returning error\n";
  // We need to set EAX to -errno_ and skip the current instruction, which should
  // be `syscall`.
  user_regs_struct regs;
  if (ptrace(PTRACE_GETREGS, child, 0, &regs) != 0) {
    std::cout << "Error with PTRACE_GETREGS\n";
    return;
  }
  std::cout << "Got regs, rax = " << regs.rax << " rip = " << regs.rip << "\n";
  regs.rax = -errno_;
  // The syscall instruction is 0f 05 according to Godbolt (just put in `asm("syscall");`.
  // So if we increment the instruction pointer by 2 bytes it will skip the syscall.
  // Note that on x86_64 there are still other ways to do syscalls, e.g. using `int`
  // but let's hope nobody does that.
  regs.rip += 2;
  if (ptrace(PTRACE_SETREGS, child, 0, &regs) != 0) {
    std::cout << "Error with PTRACE_SETREGS\n";
    return;
  }
  std::cout << "Updated regs\n";
  if (ptrace(PTRACE_GETREGS, child, 0, &regs) != 0) {
    std::cout << "Error with PTRACE_GETREGS\n";
    return;
  }
  std::cout << "Got regs, rax = " << regs.rax << " rip = " << regs.rip << "\n";

}

void process_signals(pid_t child)
{
  const char *file_to_redirect = "ONE.txt";
  const char *file_to_avoid = "TWO.txt";

  std::string filename;

  for (;;) {
    // Start the child process and wait for the start of the open() syscall.
    if (wait_for_open_syscall(child) != WaitResult::OpenSyscall) {
      break;
    }

    // Get the filename of the `open()` call by peeking into the child's memory.
    get_open_filename(child, filename);
    std::cout << "Opening " << filename << "\n";

    if (filename.find("zzz") != std::string::npos) {
      std::cout << "Child process tried to access a file containing a forbidden 'z'. Naughty child!\n";
      return_error_from_syscall(child, EPERM);
    }
  }
}

} // anonymous namespace

int main(int argc, char* argv[])
{
  if (argc < 2) {
    // fprintf(stderr, "Usage: %s [-blacklist] [+whitelist] ... -- <prog> <arg1> ... <argN>\n", argv[0]);
    return 1;
  }

  // Fork a child process.
  pid_t pid = fork();
  if (pid == 0) {
    // Child process.
    return main_child(argc, argv);
  }

  // Wait for the child to be stopped by SIGSTOP.
  int status;
  waitpid(pid, &status, 0);

  ptrace(
    PTRACE_SETOPTIONS,
    pid,
    0,
    // Stop the child when SECCOMP_RET_TRACE is returned from seccomp.
    PTRACE_O_TRACESECCOMP |
    // Kill the child when the parent exits. Just for good luck.
    PTRACE_O_EXITKILL |
    // Set bit 7 (0x80) on the signal number for SIGTRAP so we can distinguish
    // it from non-ptrace SIGTRAPs. This basically fixes a design flaw so you
    // always want it.
    PTRACE_O_TRACESYSGOOD,
    // TODO: Trace creation of new processes so we can ptrace them too.
    // PTRACE_O_TRACECLONE | PTRACE_O_TRACEEXEC | PTRACE_O_TRACEFORK | PTRACE_O_TRACEVFORK
  );

  // Loop waiting for the ptrace signals. This also restarts the child process.
  process_signals(pid);
  return 0;
}
