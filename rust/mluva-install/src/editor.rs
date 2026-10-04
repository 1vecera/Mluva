//! Keep the released single-image editor contract beside the native workers.
use std::{
    env,
    ffi::CString,
    os::unix::{ffi::OsStrExt, process::CommandExt},
    path::Path,
    process::{Command, ExitCode},
};

fn executable(path: &Path) -> bool {
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // Match the caller's effective permissions, including a managed symlink.
    unsafe { libc::faccessat(libc::AT_FDCWD, path.as_ptr(), libc::X_OK, libc::AT_EACCESS) == 0 }
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 1 {
        eprintln!("Usage: mluva-screenshot-editor image.png");
        return ExitCode::from(2);
    }
    let result = (|| -> std::io::Result<u8> {
        let own = env::current_exe()?;
        let bin = own
            .parent()
            .ok_or_else(|| std::io::Error::other("Missing application directory"))?;
        let data = bin
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| std::io::Error::other("Missing application directory"))?;
        let editor = data.join("tensaku/tensaku");
        let mut command = if executable(&editor) {
            let mut command = Command::new(editor);
            command
                .arg("--filename")
                .arg(&args[0])
                .arg("--output-filename")
                .arg(&args[0])
                .arg("--narration-command")
                .arg(bin.join("mluva-narrate"))
                .args([
                    "--actions-on-enter",
                    "save-to-clipboard",
                    "--save-after-copy",
                    "--copy-command",
                    "wl-copy",
                ]);
            command
        } else {
            let mut command = Command::new("/usr/bin/tensaku-edit");
            command.arg(&args[0]);
            command
        };
        let program = command.get_program().to_owned();
        let error = command.exec();
        // A present script with a missing interpreter is an execution failure,
        // distinct from an absent stock editor, as in the released launcher.
        Ok(
            if error.kind() == std::io::ErrorKind::NotFound && !Path::new(&program).exists() {
                127
            } else {
                126
            },
        )
    })();
    eprintln!("Mluva screenshot editor could not start. Check its installed editor and try again.");
    ExitCode::from(result.unwrap_or(126))
}
