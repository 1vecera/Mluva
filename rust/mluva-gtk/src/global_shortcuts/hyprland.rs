//! Opt-in Lua bridge for Hyprland's portal, which registers actions without keys.
use super::*;
use mluva_core::private_files::atomic_write_private;
use serde_json::Value;
use std::{path::PathBuf, time::Duration};

const HEADER: &str = "-- Mluva managed portal bindings v1\n";
const PREFIX: &str = "Mluva portal: ";
/// The session wheel appears at once, as Omarchy's own menus do, with no compositor fade. One
/// handle is kept across reloads: a live rule is reused and a rule the compositor dropped is
/// made again, so rules never stack. It runs under `pcall`, and after the bindings, so a
/// Hyprland without the layer-rule API, or an error in it, can never stop the bindings applying.
const WHEEL_RULE: &str = "pcall(function()\n  local wheel = _G.mluva_wheel_rule\n  local alive, enabled = pcall(function() return wheel:is_enabled() end)\n  if not alive then\n    _G.mluva_wheel_rule = hl.layer_rule({ name = \"mluva-session-wheel\", match = { namespace = \"^mluva-session-wheel$\" }, no_anim = true, animation = \"none\" })\n  elseif not enabled then\n    wheel:set_enabled(true)\n  end\nend)\n";

struct Binding {
    key: String,
    mask: u64,
    id: String,
}
impl Binding {
    fn trigger(&self) -> String {
        match self.mask {
            1 => format!("SHIFT + {}", self.key),
            12 => format!("CTRL + ALT + {}", self.key),
            _ => self.key.clone(),
        }
    }
}

fn bindings(key: &str, callbacks: &ShortcutCallbacks) -> Vec<Binding> {
    let mut keys = vec![
        Binding {
            key: key.into(),
            mask: 0,
            id: recording_shortcut_id(key).expect("validated key"),
        },
        Binding {
            key: "Escape".into(),
            mask: 12,
            id: CANCEL.into(),
        },
        Binding {
            key: "F9".into(),
            mask: 1,
            id: REWRITE.into(),
        },
    ];
    if let Some(agent) = &callbacks.agent {
        keys.push(Binding {
            key: agent.hold_key.clone(),
            mask: 0,
            id: AGENT_HOLD.into(),
        });
        keys.push(Binding {
            key: agent.draft_key.clone(),
            mask: 0,
            id: AGENT_DRAFT.into(),
        });
    }
    keys
}

fn conflicts(current: &[Value], desired: &[Binding]) -> bool {
    desired.iter().any(|wanted| {
        current.iter().any(|bound| {
            bound["key"]
                .as_str()
                .is_some_and(|key| key.eq_ignore_ascii_case(&wanted.key))
                && bound["modmask"].as_u64() == Some(wanted.mask)
                && !bound["description"]
                    .as_str()
                    .is_some_and(|s| s.starts_with(PREFIX))
        })
    })
}

fn script(desired: &[Binding]) -> String {
    // Retain handles, not key names: removing our handles cannot unbind another app.
    let mut lua = format!(
        "{HEADER}for _, binding in ipairs(_G.mluva_portal_bindings or {{}}) do binding:remove() end\n_G.mluva_portal_bindings = {{}}\n"
    );
    for binding in desired {
        lua.push_str(&format!(
            "table.insert(_G.mluva_portal_bindings, hl.bind(\"{}\", hl.dsp.global(\"com.mluva.Linux:{}\"), {{ description = \"{PREFIX}{}\" }}))\n",
            binding.trigger(), binding.id, binding.id,
        ));
    }
    lua.push_str(WHEEL_RULE);
    lua
}

async fn command(args: &[&str]) -> Result<String> {
    let mut command = tokio::process::Command::new("hyprctl");
    command.args(args).kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(3), command.output())
        .await
        .map_err(|_| PortalError::from("Hyprland shortcut configuration timed out."))?
        .map_err(|_| PortalError::from("Could not contact Hyprland for shortcut configuration."))?;
    if !output.status.success() || output.stdout.len() > 1_048_576 {
        return Err("Hyprland rejected shortcut configuration.".into());
    }
    String::from_utf8(output.stdout).map_err(|_| "Hyprland returned invalid shortcut data.".into())
}

async fn current() -> Result<Vec<Value>> {
    serde_json::from_str(&command(&["-j", "binds"]).await?)
        .map_err(|_| "Hyprland returned invalid shortcut data.".into())
}

/// Enabled only by an existing managed file loaded from the user's Lua config.
/// Other desktops and manually configured Hyprland sessions keep portal behavior.
pub(super) async fn apply(
    key: &str,
    callbacks: &ShortcutCallbacks,
) -> Result<Option<Vec<BoundShortcut>>> {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return Ok(None);
    }
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    let Some(path) = config.map(|p| p.join("mluva/hyprland-shortcuts.lua")) else {
        return Ok(None);
    };
    if !std::fs::read_to_string(&path).is_ok_and(|value| value.starts_with(HEADER)) {
        return Ok(None);
    }
    let desired = bindings(key, callbacks);
    // Keys are settings, never arbitrary Lua or shell text.
    for binding in desired.iter().filter(|b| b.mask == 0) {
        recording_shortcut_id(&binding.key)?;
    }
    if conflicts(&current().await?, &desired) {
        return Err("A selected key already belongs to another Hyprland binding. Choose a free key in Settings → Capture.".into());
    }
    let lua = script(&desired);
    // A leading Lua comment looks like an option to hyprctl's argument parser.
    let response = command(&["eval", &format!("do\n{lua}\nend")]).await?;
    if response.trim() != "ok" {
        return Err("Hyprland could not apply Mluva's shortcut bindings.".into());
    }
    let actual = current().await?;
    if !desired.iter().all(|wanted| {
        actual.iter().any(|bound| {
            bound["key"]
                .as_str()
                .is_some_and(|k| k.eq_ignore_ascii_case(&wanted.key))
                && bound["modmask"].as_u64() == Some(wanted.mask)
                && bound["description"] == format!("{PREFIX}{}", wanted.id)
        })
    }) {
        return Err("Hyprland did not confirm all Mluva shortcut bindings.".into());
    }
    if (callbacks.persist_bindings)() {
        atomic_write_private(&path, lua.as_bytes()).map_err(|_| {
            PortalError::from("Could not save Hyprland shortcuts for the next login.")
        })?;
    }
    Ok(Some(
        desired
            .iter()
            .map(|b| BoundShortcut {
                id: b.id.clone(),
                trigger: b.trigger(),
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_collisions_with_unrelated_bindings() {
        let wanted = vec![Binding {
            key: "F12".into(),
            mask: 0,
            id: AGENT_HOLD.into(),
        }];
        assert!(conflicts(
            &[serde_json::json!({"key":"F12","modmask":0,"description":"Other application"})],
            &wanted
        ));
        assert!(!conflicts(
            &[serde_json::json!({"key":"F12","modmask":4,"description":"Other application"})],
            &wanted
        ));
        assert!(!conflicts(
            &[
                serde_json::json!({"key":"F12","modmask":0,"description":"Mluva portal: agent-hold"})
            ],
            &wanted
        ));
    }
    #[test]
    #[ignore = "requires the Lua interpreter"]
    fn lua_rebinds_owned_handles_without_removing_other_bindings() {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let mut lua = String::from(
            "local active = { other = true }; local removed = 0; local rules = 0; hl = { dsp = { global = function(id) return id end }, bind = function(key, target, options) assert(not active[key]); active[key] = target; return { remove = function() active[key] = nil; removed = removed + 1 end } end, layer_rule = function(spec) assert(spec.match.namespace == '^mluva-session-wheel$' and spec.no_anim == true); rules = rules + 1; local on = true; return { is_enabled = function() return on end, set_enabled = function(_, value) on = value end } end };\n",
        );
        lua.push_str(&script(&[Binding {
            key: "F12".into(),
            mask: 0,
            id: AGENT_HOLD.into(),
        }]));
        lua.push_str("assert(active.F12 == 'com.mluva.Linux:agent-hold');\n");
        lua.push_str(&script(&[Binding {
            key: "F11".into(),
            mask: 0,
            id: AGENT_HOLD.into(),
        }]));
        lua.push_str("assert(active.F12 == nil and active.F11 == 'com.mluva.Linux:agent-hold' and active.other == true and removed == 1)\n");
        // Two applies leave one rule; a rule that was switched off comes back; a dropped one is remade.
        lua.push_str("assert(rules == 1)\n_G.mluva_wheel_rule:set_enabled(false)\n");
        lua.push_str(&script(&[]));
        lua.push_str("assert(rules == 1 and _G.mluva_wheel_rule:is_enabled())\n_G.mluva_wheel_rule = { is_enabled = function() error('dropped') end }\n");
        lua.push_str(&script(&[]));
        lua.push_str("assert(rules == 2)\n");
        let mut child = Command::new("lua")
            .arg("-")
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(lua.as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
        // A layer rule that throws, or no layer-rule API at all, still leaves the bindings applied.
        for rule in [
            "layer_rule = function() error('no such api') end",
            "layer_rule = nil",
        ] {
            let mut lua = format!(
                "local active = {{}}; hl = {{ dsp = {{ global = function(id) return id end }}, bind = function(key, target) active[key] = target; return {{ remove = function() end }} end, {rule} }};\n"
            );
            lua.push_str(&script(&[Binding {
                key: "F12".into(),
                mask: 0,
                id: AGENT_HOLD.into(),
            }]));
            lua.push_str("assert(active.F12 == 'com.mluva.Linux:agent-hold')\n");
            let mut child = Command::new("lua")
                .arg("-")
                .stdin(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(lua.as_bytes())
                .unwrap();
            assert!(child.wait().unwrap().success(), "{rule}");
        }
    }
}
