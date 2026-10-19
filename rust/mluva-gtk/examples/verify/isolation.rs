//! Check independent kernel, process and filesystem evidence before any desktop I/O.
use super::*;

pub fn check() -> Result<PathBuf> {
    let root = path_env("OFFSCREEN_SESSION_ROOT")?.canonicalize()?;
    let evidence = path_env("OFFSCREEN_ARTIFACT_DIR")?.canonicalize()?;
    let checkout = std::env::current_dir()?.canonicalize()?;
    let mounts = fs::read_to_string("/proc/self/mountinfo")?;
    for hidden in ["/home", "/root", "/run"] {
        require(
            mounts.lines().any(|line| {
                line.split_whitespace().nth(4) == Some(hidden) && line.contains(" - tmpfs ")
            }),
            &format!("host {hidden} must be hidden by a private filesystem"),
        )?;
    }
    require(
        evidence.starts_with(checkout.join("tmp")) && root.starts_with(&evidence),
        "session must belong to this checkout tmp/",
    )?;
    for key in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "TMPDIR",
    ] {
        let path = path_env(key)?.canonicalize()?;
        require(
            path.starts_with(&root),
            &format!("{key} escapes the private session"),
        )?;
    }
    require(
        path_env("XDG_RUNTIME_DIR")?.canonicalize()? == root.join("runtime").canonicalize()?,
        "runtime is not the session's private alias",
    )?;
    for kind in ["net", "pid", "mnt"] {
        require(
            fs::read_link(format!("/proc/self/ns/{kind}"))?.to_string_lossy()
                != fs::read_to_string(evidence.join(format!("host-{kind}")))?.trim(),
            &format!("host {kind} namespace was inherited"),
        )?;
    }
    require(
        fs::read_link("/proc/1/exe")?
            .file_name()
            .is_some_and(|n| n == "bwrap"),
        "private PID namespace must be supervised by bubblewrap",
    )?;
    require(
        fs::read_to_string("/proc/net/route")?.lines().count() == 1,
        "network namespace has an external route",
    )?;
    for device in [
        "/dev/input",
        "/dev/uinput",
        "/dev/snd",
        "/dev/dri",
        "/run/dbus/system_bus_socket",
    ] {
        require(
            !Path::new(device).exists(),
            &format!("host endpoint accessible: {device}"),
        )?;
    }
    require(
        std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("x11")
            && std::env::var("GDK_BACKEND").as_deref() == Ok("x11"),
        "private X11 required",
    )?;
    require(
        ["WAYLAND_DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE"]
            .iter()
            .all(|key| std::env::var_os(key).is_none()),
        "host compositor environment inherited",
    )?;
    let display = std::env::var("DISPLAY")?;
    let number = display
        .strip_prefix(':')
        .ok_or("local Xvfb display required")?
        .split('.')
        .next()
        .unwrap();
    require(
        !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()),
        "invalid Xvfb display",
    )?;
    let pid: u32 = fs::read_to_string(format!("/tmp/.X{number}-lock"))?
        .trim()
        .parse()?;
    require(
        fs::read_link(format!("/proc/{pid}/exe"))?
            .file_name()
            .is_some_and(|n| n == "Xvfb"),
        "display is not owned by private Xvfb",
    )?;
    let authority = path_env("XAUTHORITY")?.canonicalize()?;
    require(
        authority.is_file() && authority.starts_with(&root),
        "private Xauthority required",
    )?;
    let xvfb = fs::read(format!("/proc/{pid}/cmdline"))?;
    let xvfb_environment = fs::read(format!("/proc/{pid}/environ"))?;
    let authority_environment = format!("XAUTHORITY={}", authority.display());
    require(
        xvfb.split(|b| *b == 0)
            .any(|arg| arg == authority.as_os_str().as_encoded_bytes())
            || xvfb_environment
                .split(|b| *b == 0)
                .any(|arg| arg == authority_environment.as_bytes()),
        "Xvfb does not own this authority",
    )?;
    for key in ["DBUS_SESSION_BUS_ADDRESS", "AT_SPI_BUS_ADDRESS"] {
        let address = std::env::var(key)?;
        require(
            if key == "AT_SPI_BUS_ADDRESS" {
                address.starts_with("unix:abstract=offscreen-atspi-")
            } else {
                address.starts_with("unix:path=/tmp/")
            },
            &format!("unexpected private {key}"),
        )?;
        let bus = connection(&address)?;
        let pid = bus_pid(&bus, "org.freedesktop.DBus")?;
        require(
            fs::read_link(format!("/proc/{pid}/exe"))?
                .file_name()
                .is_some_and(|n| n == "dbus-daemon"),
            "bus daemon is outside private PID namespace",
        )?;
        let environ = fs::read(format!("/proc/{pid}/environ"))?;
        let marker = format!("XDG_CONFIG_HOME={}", std::env::var("XDG_CONFIG_HOME")?);
        require(
            environ.split(|b| *b == 0).any(|s| s == marker.as_bytes()),
            "bus daemon belongs to another session",
        )?;
    }
    Ok(root)
}
