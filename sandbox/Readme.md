# Sandbox

This is a little experiment to use SECCOMP to sandbox filesystem access for apps. It basically intercepts filesystem syscalls (though I haven't got around to doing all of them), checks the path, and then allows/denies them based on blacklists/whitelists passed on the command line.

I believe Cosmipolitan libc uses the same technique to implement the `pledge()` function.

It works fairly well, except symlinks (the root of all evil) play absolute havok with it. Also, it didn't exist when I wrote this, but a much saner way to do this on modern Linux systems is with [Landlock](https://landlock.io/). I would use that if I were doing this again. Much less work to upgrade Linux than to implement this, even on janky old corporate systems.
