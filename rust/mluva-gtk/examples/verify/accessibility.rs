//! External AT-SPI client; only the verified Mluva process is traversed.
use super::*;
use std::collections::VecDeque;

fn children(
    bus: &gio::DBusConnection,
    name: &str,
    path: &str,
) -> Result<Vec<(String, glib::variant::ObjectPath)>> {
    let reply = call(
        bus,
        name,
        path,
        "org.a11y.atspi.Accessible",
        "GetChildren",
        None,
    )?;
    reply
        .child_value(0)
        .get()
        .ok_or_else(|| "invalid accessible children".into())
}

pub fn nodes() -> Result<Value> {
    let (_, pid) = app_owner()?;
    let bus = connection(&std::env::var("AT_SPI_BUS_ADDRESS")?)?;
    let mut queue = VecDeque::new();
    for (name, path) in children(
        &bus,
        "org.a11y.atspi.Registry",
        "/org/a11y/atspi/accessible/root",
    )? {
        if bus_pid(&bus, &name).ok() == Some(pid) {
            queue.push_back((name, path));
        }
    }
    require(
        !queue.is_empty(),
        "Mluva is absent from the private accessibility registry",
    )?;
    let mut nodes = vec![];
    while let Some((name, path)) = queue.pop_front() {
        require(
            nodes.len() < 400,
            "accessible tree exceeds the bounded discovery limit",
        )?;
        let interfaces = call(
            &bus,
            &name,
            &path,
            "org.a11y.atspi.Accessible",
            "GetInterfaces",
            None,
        )?
        .child_value(0)
        .get::<Vec<String>>()
        .ok_or("invalid accessible interfaces")?;
        let properties = call(
            &bus,
            &name,
            &path,
            "org.freedesktop.DBus.Properties",
            "GetAll",
            Some(&("org.a11y.atspi.Accessible",).to_variant()),
        )?
        .child_value(0)
        .get::<BTreeMap<String, glib::Variant>>()
        .ok_or("invalid accessible properties")?;
        let label = properties.get("Name").and_then(|v| v.str()).unwrap_or("");
        let role = call(
            &bus,
            &name,
            &path,
            "org.a11y.atspi.Accessible",
            "GetRoleName",
            None,
        )?
        .child_get::<String>(0);
        let states = call(
            &bus,
            &name,
            &path,
            "org.a11y.atspi.Accessible",
            "GetState",
            None,
        )?
        .child_value(0)
        .get::<Vec<u32>>()
        .ok_or("invalid accessible states")?;
        // AT-SPI EDITABLE=7, SENSITIVE=24, SHOWING=25. GTK publishes
        // SENSITIVE rather than ENABLED. Hidden/read-only
        // text controls must not become mutation targets during navigation.
        let editable = states
            .first()
            .is_some_and(|bits| [7, 24, 25].iter().all(|bit| bits & (1 << bit) != 0));
        let text = if interfaces.iter().any(|v| v == "org.a11y.atspi.Text") {
            Some(
                call(
                    &bus,
                    &name,
                    &path,
                    "org.a11y.atspi.Text",
                    "GetText",
                    Some(&(0_i32, -1_i32).to_variant()),
                )?
                .child_get::<String>(0),
            )
        } else {
            None
        };
        nodes.push(json!({"bus":name,"path":path.as_str(),"name":label,"role":role,"interfaces":interfaces,"text":text,"editable_showing":editable,"states":states}));
        queue.extend(children(&bus, &name, &path)?);
    }
    Ok(json!(nodes))
}

pub fn editor(nodes: &Value) -> Result<&Value> {
    let candidates = nodes
        .as_array()
        .ok_or("invalid tree")?
        .iter()
        .filter(|node| {
            node["interfaces"].as_array().is_some_and(|interfaces| {
                interfaces
                    .iter()
                    .any(|v| v == "org.a11y.atspi.EditableText")
            }) && node["role"] == "text box"
                && node["editable_showing"] == true
        })
        .collect::<Vec<_>>();
    require(
        candidates.len() == 1,
        &format!(
            "expected one editable document; found {} (inspect discover)",
            candidates.len()
        ),
    )?;
    Ok(candidates[0])
}

pub fn edit(text: &str) -> Result<Value> {
    let nodes = nodes()?;
    let editor = editor(&nodes)?;
    let bus = connection(&std::env::var("AT_SPI_BUS_ADDRESS")?)?;
    let result = call(
        &bus,
        editor["bus"].as_str().unwrap(),
        editor["path"].as_str().unwrap(),
        "org.a11y.atspi.EditableText",
        "SetTextContents",
        Some(&(text,).to_variant()),
    )?;
    require(
        result.child_get::<bool>(0),
        "AT-SPI editor refused SetTextContents",
    )?;
    Ok(json!({"transport":"AT-SPI EditableText.SetTextContents","text":text,"path":editor["path"]}))
}
