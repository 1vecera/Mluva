//! Managed native providers must not outlive an abruptly terminated owner.
use tokio::process::Command;

pub(crate) fn kill_with_parent(command: &mut Command) {
    let parent = std::process::id() as libc::pid_t;
    // Only async-signal-safe syscalls run between fork and exec. Recheck the
    // parent to cover its death before the kernel signal was installed.
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != parent {
                return Err(std::io::Error::from_raw_os_error(libc::ESRCH));
            }
            Ok(())
        });
    }
}
