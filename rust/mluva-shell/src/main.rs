//! Widget bridge. No GTK initialization, local data access or application auto-start.
mod arguments;
mod projection;

use gio::prelude::*;
use glib::variant::ToVariant;
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, io::Write, rc::Rc};

const NAME: &str = "com.mluva.Linux";
const ACTION_PATH: &str = "/com/mluva/Linux";
const STATUS_PATH: &str = "/com/mluva/Linux/RecordingStatus";
const STATUS_INTERFACE: &str = "com.mluva.Linux.RecordingStatus";

fn activate(
    connection: &gio::DBusConnection,
    owner: &str,
    action: &str,
    review: Option<&(String, String, String)>,
) -> Result<(), glib::Error> {
    let parameters = review
        .into_iter()
        .map(ToVariant::to_variant)
        .collect::<Vec<_>>();
    connection.call_sync(
        Some(owner),
        ACTION_PATH,
        "org.gtk.Actions",
        "Activate",
        Some(&(action, parameters, BTreeMap::<String, glib::Variant>::new()).to_variant()),
        None,
        gio::DBusCallFlags::NO_AUTO_START,
        1500,
        gio::Cancellable::NONE,
    )?;
    Ok(())
}

fn current_owner(connection: &gio::DBusConnection) -> Result<String, glib::Error> {
    let reply = connection.call_sync(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetNameOwner",
        Some(&(NAME,).to_variant()),
        Some(glib::VariantTy::new("(s)").unwrap()),
        gio::DBusCallFlags::NO_AUTO_START,
        1500,
        gio::Cancellable::NONE,
    )?;
    Ok(reply.child_get::<String>(0))
}

struct Watch {
    connection: gio::DBusConnection,
    main_loop: glib::MainLoop,
    overlay: bool,
    owner: RefCell<Option<String>>,
    subscription: RefCell<Option<gio::SignalSubscription>>,
}

impl Watch {
    fn emit(&self, state: Value) -> bool {
        // Retain released JSON spacing and ASCII escapes, including non-BMP previews.
        let json = mluva_core::json::spaced(&state);
        let mut line = String::with_capacity(json.len() + 1);
        for character in json.chars() {
            if character.is_ascii() && character != '\u{7f}' {
                line.push(character);
            } else {
                use std::fmt::Write;
                for unit in character.encode_utf16(&mut [0; 2]) {
                    write!(line, "\\u{unit:04x}").unwrap();
                }
            }
        }
        line.push('\n');
        let mut stdout = std::io::stdout().lock();
        if stdout
            .write_all(line.as_bytes())
            .and_then(|()| stdout.flush())
            .is_err()
        {
            self.main_loop.quit();
            return false;
        }
        true
    }

    fn unsubscribe(&self) {
        self.owner.borrow_mut().take();
        self.subscription.borrow_mut().take();
    }

    fn appeared(self: &Rc<Self>, owner: &str) {
        self.unsubscribe();
        *self.owner.borrow_mut() = Some(owner.into());
        if !self.emit(json!({"phase":"unavailable", "elapsed":0})) {
            return;
        }
        let weak = Rc::downgrade(self);
        let subscription = self.connection.subscribe_to_signal(
            Some(owner),
            Some(STATUS_INTERFACE),
            Some(if self.overlay {
                "ShellStateChanged"
            } else {
                "StateChanged"
            }),
            Some(STATUS_PATH),
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                if let Some(watch) = weak.upgrade()
                    && watch.owner.borrow().as_deref() == Some(signal.sender_name)
                {
                    watch.emit(projection::state(signal.parameters, watch.overlay));
                }
            },
        );
        *self.subscription.borrow_mut() = Some(subscription);
        // Subscribe first: the owner may publish synchronously while handling replay.
        if activate(&self.connection, owner, "status", None).is_err() {
            self.emit(json!({"phase":"unavailable", "elapsed":0}));
        }
    }
}

fn run(args: arguments::Arguments) -> Result<(), glib::Error> {
    let connection = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)?;
    if args.action != "watch" {
        return activate(
            &connection,
            &current_owner(&connection)?,
            &args.action,
            args.review.as_ref(),
        );
    }
    let main_loop = glib::MainLoop::new(None, false);
    let watch = Rc::new(Watch {
        connection: connection.clone(),
        main_loop: main_loop.clone(),
        overlay: args.overlay,
        owner: RefCell::new(None),
        subscription: RefCell::new(None),
    });
    let appeared = Rc::downgrade(&watch);
    let vanished = Rc::downgrade(&watch);
    let name_watch = gio::bus_watch_name_on_connection(
        &connection,
        NAME,
        gio::BusNameWatcherFlags::NONE,
        move |_, _, owner| {
            if let Some(watch) = appeared.upgrade() {
                watch.appeared(owner);
            }
        },
        move |_, _| {
            if let Some(watch) = vanished.upgrade() {
                watch.unsubscribe();
                watch.emit(json!({"phase":"stopped","elapsed":0}));
            }
        },
    );
    let closed_loop = main_loop.clone();
    let closed = connection.connect_closed(move |_, _, _| closed_loop.quit());
    main_loop.run();
    gio::bus_unwatch_name(name_watch);
    watch.unsubscribe();
    connection.disconnect(closed);
    Ok(())
}

fn main() -> std::process::ExitCode {
    let args = match arguments::parse(std::env::args().skip(1)) {
        Ok(Some(args)) => args,
        Ok(None) => return std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{}mluva-shell: error: {message}", arguments::USAGE);
            return std::process::ExitCode::from(2);
        }
    };
    if run(args).is_err() {
        eprintln!("Mluva unavailable. Start the configured Mluva application first.");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}
