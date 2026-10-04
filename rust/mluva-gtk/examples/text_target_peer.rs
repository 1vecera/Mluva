//! A separate real GTK target for private accessibility transport comparisons; never install.

use gtk::prelude::*;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

fn require_private_session() -> PathBuf {
    let root = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("private session"))
        .canonicalize()
        .unwrap();
    assert!(
        PathBuf::from(std::env::var_os("XDG_DATA_HOME").unwrap())
            .canonicalize()
            .unwrap()
            .starts_with(&root)
    );
    assert!(
        std::env::var("AT_SPI_BUS_ADDRESS")
            .unwrap()
            .starts_with("unix:abstract=offscreen-atspi-")
    );
    assert_eq!(std::env::var("GDK_BACKEND").unwrap(), "x11");
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none());
    assert!(
        PathBuf::from(std::env::var_os("XAUTHORITY").unwrap())
            .canonicalize()
            .unwrap()
            .starts_with(&root)
    );
    root
}

fn write(path: &Path, value: &Value) {
    let temporary = path.with_extension("temporary");
    fs::write(&temporary, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    fs::rename(temporary, path).unwrap();
}

fn main() {
    let root = require_private_session();
    let directory = PathBuf::from(std::env::args_os().nth(1).unwrap())
        .canonicalize()
        .unwrap();
    assert!(directory.starts_with(&root));
    gtk::init().unwrap();
    let window = gtk::Window::builder()
        .title("Private Mluva text target")
        .default_width(680)
        .default_height(360)
        .build();
    let column = gtk::Box::new(gtk::Orientation::Vertical, 16);
    column.set_margin_top(20);
    column.set_margin_bottom(20);
    column.set_margin_start(20);
    column.set_margin_end(20);
    let entry = gtk::Entry::new();
    let text = gtk::TextView::new();
    text.set_height_request(110);
    let concealed = gtk::Entry::new();
    concealed.set_visibility(false);
    let password = gtk::PasswordEntry::new();
    let button = gtk::Button::with_label("External non-text focus");
    column.append(&entry);
    column.append(&text);
    column.append(&concealed);
    column.append(&password);
    column.append(&button);
    window.set_child(Some(&column));
    window.present();
    button.grab_focus();

    // Publish the canonical status only on the private session bus; libatspi itself uses the
    // distinct private accessibility bus selected by the runner, without a host bus launcher.
    let session = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap();
    let enabled = Rc::new(Cell::new(true));
    let value = enabled.clone();
    let info = gio::DBusNodeInfo::for_xml("<node><interface name='org.a11y.Status'><property name='IsEnabled' type='b' access='read'/></interface></node>").unwrap();
    let registration = session
        .register_object("/org/a11y/bus", &info.interfaces()[0])
        .property(move |_, _, _, _, _| value.get().to_variant())
        .build()
        .unwrap();
    session
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
            Some(&("org.a11y.Bus", 4u32).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            1_000,
            gio::Cancellable::NONE,
        )
        .unwrap();

    let serial = Rc::new(Cell::new(0u64));
    let edits = Rc::new(Cell::new(0u64));
    for editable in [
        entry.upcast_ref::<gtk::Editable>(),
        concealed.upcast_ref(),
        password.upcast_ref(),
    ] {
        let count = edits.clone();
        editable.connect_changed(move |_| count.set(count.get() + 1));
    }
    let count = edits.clone();
    text.buffer()
        .connect_changed(move |_| count.set(count.get() + 1));
    let request_path = directory.join("request.json");
    let observed_path = directory.join("observed.json");
    let loop_ = glib::MainLoop::new(None, false);
    let event_loop = loop_.clone();
    glib::timeout_add_local(Duration::from_millis(15), move || {
        if let Ok(bytes) = fs::read(&request_path) {
            let request: Value = serde_json::from_slice(&bytes).unwrap();
            if let Some(next) = request["serial"]
                .as_u64()
                .filter(|next| *next != serial.get())
            {
                let operation = request["operation"].as_str().unwrap();
                match operation {
                    "setup" => {
                        let content = request["text"].as_str().unwrap();
                        let kind = request["kind"].as_str().unwrap();
                        let start = request["start"].as_i64().unwrap() as i32;
                        let end = request["end"].as_i64().unwrap() as i32;
                        let writable = request["editable"].as_bool().unwrap_or(true);
                        window.present();
                        match kind {
                            "entry" | "concealed" => {
                                let field = if kind == "entry" { &entry } else { &concealed };
                                field.set_editable(writable);
                                field.set_text(content);
                                field.grab_focus();
                                field.select_region(start, end);
                            }
                            "password" => {
                                password.set_editable(writable);
                                password.set_text(content);
                                password.grab_focus();
                                password.select_region(start, end);
                            }
                            "textview" => {
                                text.set_editable(writable);
                                let buffer = text.buffer();
                                buffer.set_text(content);
                                text.grab_focus();
                                buffer.select_range(
                                    &buffer.iter_at_offset(end),
                                    &buffer.iter_at_offset(start),
                                );
                            }
                            _ => panic!("unknown target kind"),
                        }
                        edits.set(0);
                    }
                    "button" => {
                        button.grab_focus();
                    }
                    "entry-focus" => {
                        entry.grab_focus();
                    }
                    "caret" => {
                        entry.select_region(
                            request["offset"].as_i64().unwrap() as i32,
                            request["offset"].as_i64().unwrap() as i32,
                        );
                    }
                    "selection" => {
                        entry.select_region(
                            request["start"].as_i64().unwrap() as i32,
                            request["end"].as_i64().unwrap() as i32,
                        );
                    }
                    "enabled" => {
                        enabled.set(request["value"].as_bool().unwrap());
                    }
                    "quit" => {
                        event_loop.quit();
                    }
                    _ => panic!("unknown target operation"),
                }
                serial.set(next);
            }
        }
        let buffer = text.buffer();
        let field = |entry: &gtk::Editable| {
            json!({"text":entry.text().as_str(), "caret":entry.position(),
            "selection":entry.selection_bounds(), "editable":entry.is_editable()})
        };
        write(
            &observed_path,
            &json!({"serial":serial.get(), "pid":std::process::id(), "enabled":enabled.get(),
            "entry":field(entry.upcast_ref()), "concealed":field(concealed.upcast_ref()), "password":field(password.upcast_ref()), "textview":{
                "text":buffer.text(&buffer.start_iter(), &buffer.end_iter(), true).as_str(),
                "caret":buffer.cursor_position(), "selection":buffer.selection_bounds().map(|(start,end)| (start.offset(),end.offset())),
                "focused":text.has_focus(), "editable":text.is_editable()}, "edits":edits.get(), "active":window.is_active()}),
        );
        glib::ControlFlow::Continue
    });
    loop_.run();
    session.unregister_object(registration).unwrap();
}
