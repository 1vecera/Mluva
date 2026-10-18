//! Real process startup, single-instance actions and orderly exit after upgrading preferences.
use glib::variant::ToVariant;
use mluva_core::config::AppConfig;
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[path = "support/dictation_ui.rs"]
mod ui;
#[test]
#[ignore = "requires a private display, session bus, HOME/XDG and device namespace"]
fn actual_binary_opens_one_instance_keeps_old_preferences_and_quits() {
    let root = ui::isolated();
    let config =
        PathBuf::from(std::env::var_os("XDG_CONFIG_HOME").unwrap()).join("mluva/config.json");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    let bytes=br#"{"transcription_provider":"local","local_model":"parakeet-v3","incognito_mode":true,"welcome_completed":true,"default_mode":"command","rewrite_provider":"codex","auto_paste":false}"#;
    fs::write(&config, bytes).unwrap();
    let binary = std::env::var_os("MLUVA_BOOTSTRAP_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_mluva")));
    let start = || {
        Command::new(&binary)
            .env("MLUVA_DISABLE_GLOBAL_SHORTCUT", "1")
            .env("GSETTINGS_BACKEND", "memory")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(fs::File::create(root.join("binary-startup.log")).unwrap())
            .spawn()
            .unwrap()
    };
    let mut child = start();
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap();
    let owner = || {
        bus.call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetNameOwner",
            Some(&("com.mluva.Linux",).to_variant()),
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            500,
            gio::Cancellable::NONE,
        )
        .ok()
        .map(|v| v.child_value(0).str().unwrap().to_owned())
    };
    ui::until(|| owner().is_some());
    let first = owner().unwrap();
    let action = |name: &str| {
        bus.call_sync(
            Some(&first),
            "/com/mluva/Linux",
            "org.gtk.Actions",
            "Activate",
            Some(
                &(
                    name,
                    Vec::<glib::Variant>::new(),
                    BTreeMap::<String, glib::Variant>::new(),
                )
                    .to_variant(),
            ),
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            1500,
            gio::Cancellable::NONE,
        )
        .unwrap()
    };
    let actions = bus
        .call_sync(
            Some(&first),
            "/com/mluva/Linux",
            "org.gtk.Actions",
            "List",
            None,
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            1500,
            gio::Cancellable::NONE,
        )
        .unwrap()
        .child_value(0)
        .get::<Vec<String>>()
        .unwrap();
    for name in ["record", "cancel", "latest", "settings", "account", "quit"] {
        assert!(actions.iter().any(|v| v == name));
    }
    assert!(!actions.iter().any(|v| v == "review"));
    let mut second = start();
    ui::until(|| second.try_wait().unwrap().is_some());
    assert!(second.wait().unwrap().success());
    assert_eq!(owner().as_ref(), Some(&first));
    action("latest");
    action("settings");
    action("latest");
    action("quit");
    let deadline = Instant::now() + Duration::from_secs(12);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("Mluva did not quit");
        }
        ui::settle(0.02);
    }
    assert!(child.wait().unwrap().success());
    assert!(owner().is_none());
    assert_eq!(fs::read(&config).unwrap(), bytes);
    let saved = AppConfig::load(&config).unwrap();
    assert_eq!(saved.transcription_provider, "local");
    assert_eq!(saved.local_model, "parakeet-v3");
    assert!(saved.incognito_mode);
}
