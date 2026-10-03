use glib::variant::ToVariant;
use std::collections::VecDeque;

/// Read the public accessibility protocol directly, independently of Mluva's
/// implementation and the reference observer's libatspi bindings.
pub struct Accessibility(gio::DBusConnection);
impl Accessibility {
    /// Public SHOWING state, accessible names/roles and list order. This never
    /// inspects Mluva objects or libatspi's asynchronously populated cache.
    pub fn visible_content(&self) -> (Vec<(String, String)>, Vec<String>) {
        let mut queue = VecDeque::from([(
            "org.a11y.atspi.Registry".into(),
            "/org/a11y/atspi/accessible/root".into(),
        )]);
        let mut names = Vec::new();
        let mut items = Vec::new();
        let mut visited = 0;
        while let Some(node) = queue.pop_front() {
            visited += 1;
            assert!(visited < 3000, "unbounded accessibility tree");
            let showing = self
                .call(&node, "org.a11y.atspi.Accessible", "GetState", None)
                .and_then(|value| value.child_value(0).get::<Vec<u32>>())
                .is_some_and(|states| states[0] & (1 << 25) != 0);
            if showing {
                let name = self
                    .call(
                        &node,
                        "org.freedesktop.DBus.Properties",
                        "Get",
                        Some(&("org.a11y.atspi.Accessible", "Name").to_variant()),
                    )
                    .and_then(|value| value.child_value(0).as_variant())
                    .and_then(|value| value.str().map(str::to_owned));
                let role = self
                    .call(&node, "org.a11y.atspi.Accessible", "GetRoleName", None)
                    .and_then(|value| value.child_value(0).str().map(str::to_owned));
                if let (Some(name), Some(role)) = (name, role)
                    && !name.is_empty()
                {
                    if role == "list item" {
                        items.push(name.clone());
                    }
                    if !["application", "frame", "panel", "filler", "text"].contains(&role.as_str())
                    {
                        names.push((role, name));
                    }
                }
            }
            if let Some(children) =
                self.call(&node, "org.a11y.atspi.Accessible", "GetChildren", None)
            {
                let children = children.child_value(0);
                for index in 0..children.n_children() {
                    let child = children.child_value(index);
                    queue.push_back((
                        child.child_value(0).str().unwrap().into(),
                        child.child_value(1).str().unwrap().into(),
                    ));
                }
            }
        }
        names.sort();
        (names, items)
    }
    pub fn open() -> Self {
        let address = std::env::var("AT_SPI_BUS_ADDRESS").unwrap();
        assert!(address.starts_with("unix:abstract=offscreen-atspi-"));
        Self(
            gio::DBusConnection::for_address_sync(
                &address,
                gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                    | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
                None,
                gio::Cancellable::NONE,
            )
            .unwrap(),
        )
    }
    pub fn call(
        &self,
        node: &(String, String),
        interface: &str,
        method: &str,
        args: Option<&glib::Variant>,
    ) -> Option<glib::Variant> {
        self.0
            .call_sync(
                Some(&node.0),
                &node.1,
                interface,
                method,
                args,
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                1000,
                gio::Cancellable::NONE,
            )
            .ok()
    }
    pub fn button(&self, label: &str) -> Option<(String, String)> {
        let mut queue = VecDeque::from([(
            "org.a11y.atspi.Registry".into(),
            "/org/a11y/atspi/accessible/root".into(),
        )]);
        let mut visited = 0;
        while let Some(node) = queue.pop_front() {
            visited += 1;
            assert!(visited < 3000);
            let name = self
                .call(
                    &node,
                    "org.freedesktop.DBus.Properties",
                    "Get",
                    Some(&("org.a11y.atspi.Accessible", "Name").to_variant()),
                )
                .and_then(|value| value.child_value(0).as_variant())
                .and_then(|value| value.str().map(str::to_owned));
            if name.as_deref() == Some(label)
                && self
                    .call(&node, "org.a11y.atspi.Accessible", "GetInterfaces", None)
                    .and_then(|value| value.child_value(0).get::<Vec<String>>())
                    .is_some_and(|interfaces| {
                        interfaces
                            .iter()
                            .any(|value| value == "org.a11y.atspi.Action")
                    })
            {
                return Some(node);
            }
            if let Some(children) =
                self.call(&node, "org.a11y.atspi.Accessible", "GetChildren", None)
            {
                let children = children.child_value(0);
                for index in 0..children.n_children() {
                    let child = children.child_value(index);
                    queue.push_back((
                        child.child_value(0).str().unwrap().into(),
                        child.child_value(1).str().unwrap().into(),
                    ));
                }
            }
        }
        None
    }
}
