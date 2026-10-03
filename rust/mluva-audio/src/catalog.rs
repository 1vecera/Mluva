use crate::process;
use caseless::Caseless;
use mluva_core::text::trim;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::process::Command;

const MAX_PIPEWIRE_DUMP_BYTES: usize = 20_000_000;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct PipeWireCatalogError(pub &'static str);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PipeWireDeviceKind {
    Microphone,
    SystemOutput,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipeWireDevice {
    pub target: String,
    pub name: String,
    pub kind: PipeWireDeviceKind,
}

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipeWireDeviceCatalog {
    pub microphones: Vec<PipeWireDevice>,
    pub system_outputs: Vec<PipeWireDevice>,
}

impl PipeWireDeviceCatalog {
    pub fn from_system(executable: Option<&Path>) -> Result<Self, PipeWireCatalogError> {
        let executable = executable
            .map(Path::to_path_buf)
            .or_else(|| process::find_executable("pw-dump"))
            .ok_or(PipeWireCatalogError(
                "pw-dump is required to list PipeWire audio devices.",
            ))?;
        let mut command = Command::new(executable);
        command.arg("--no-colors");
        let (status, output) = process::bounded_stdout(
            command,
            MAX_PIPEWIRE_DUMP_BYTES,
            process::FINALIZATION_TIMEOUT,
        )
        .map_err(|_| PipeWireCatalogError("PipeWire audio devices could not be listed."))?;
        if !status.success() {
            return Err(PipeWireCatalogError(
                "PipeWire audio devices could not be listed.",
            ));
        }
        if output.len() > MAX_PIPEWIRE_DUMP_BYTES {
            return Err(PipeWireCatalogError(
                "PipeWire returned an unexpectedly large device graph.",
            ));
        }
        parse_pipewire_devices(&output)
    }

    pub fn devices(&self, kind: PipeWireDeviceKind) -> &[PipeWireDevice] {
        match kind {
            PipeWireDeviceKind::Microphone => &self.microphones,
            PipeWireDeviceKind::SystemOutput => &self.system_outputs,
        }
    }

    pub fn find(&self, kind: PipeWireDeviceKind, target: Option<&str>) -> Option<&PipeWireDevice> {
        target.and_then(|target| {
            self.devices(kind)
                .iter()
                .find(|device| device.target == target)
        })
    }

    pub fn display_name(&self, kind: PipeWireDeviceKind, target: Option<&str>) -> String {
        match target {
            None => "Default (automatic)".into(),
            Some(target) => self.find(kind, Some(target)).map_or_else(
                || format!("Unavailable target: {target}"),
                |device| device.name.clone(),
            ),
        }
    }
}

fn fold(value: &str) -> String {
    value.chars().default_case_fold().collect()
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
        Value::Number(value) => value.as_f64() != Some(0.0),
    }
}

pub fn parse_pipewire_devices(
    payload: &[u8],
) -> Result<PipeWireDeviceCatalog, PipeWireCatalogError> {
    let payload: Value = serde_json::from_slice(payload)
        .map_err(|_| PipeWireCatalogError("PipeWire returned invalid device metadata."))?;
    let nodes = payload.as_array().ok_or(PipeWireCatalogError(
        "PipeWire device metadata must be a JSON array.",
    ))?;
    let mut microphones = BTreeMap::new();
    let mut system_outputs = BTreeMap::new();
    for node in nodes {
        if !node
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind.ends_with(":Node"))
        {
            continue;
        }
        let Some(props) = node
            .get("info")
            .and_then(|info| info.get("props"))
            .and_then(Value::as_object)
        else {
            continue;
        };
        if props.get("node.disabled") == Some(&Value::Bool(true)) {
            continue;
        }
        let Some(target) = props.get("node.name").and_then(Value::as_str) else {
            continue;
        };
        if target.is_empty()
            || trim(target) != target
            || target.chars().count() > 512
            || target.chars().any(|character| u32::from(character) < 32)
        {
            continue;
        }
        let name = ["node.description", "node.nick"]
            .iter()
            .find_map(|key| props.get(*key).filter(|value| truthy(value)));
        let name = name
            .and_then(Value::as_str)
            .map(trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(target)
            .to_owned();
        let kind = match props.get("media.class").and_then(Value::as_str) {
            Some("Audio/Source")
                if props.get("device.class").and_then(Value::as_str) != Some("monitor")
                    && !fold(target).ends_with(".monitor") =>
            {
                PipeWireDeviceKind::Microphone
            }
            Some("Audio/Sink") => PipeWireDeviceKind::SystemOutput,
            _ => continue,
        };
        let devices = match kind {
            PipeWireDeviceKind::Microphone => &mut microphones,
            PipeWireDeviceKind::SystemOutput => &mut system_outputs,
        };
        devices
            .entry(target.to_owned())
            .or_insert_with(|| PipeWireDevice {
                target: target.into(),
                name,
                kind,
            });
    }
    Ok(PipeWireDeviceCatalog {
        microphones: labels(microphones),
        system_outputs: labels(system_outputs),
    })
}

fn labels(devices: BTreeMap<String, PipeWireDevice>) -> Vec<PipeWireDevice> {
    let mut counts = HashMap::<String, usize>::new();
    for device in devices.values() {
        *counts.entry(fold(&device.name)).or_default() += 1;
    }
    let mut devices: Vec<_> = devices
        .into_values()
        .map(|mut device| {
            if counts[&fold(&device.name)] > 1 {
                device.name = format!("{} ({})", device.name, device.target);
            }
            device
        })
        .collect();
    devices.sort_by_cached_key(|device| (fold(&device.name), device.target.clone()));
    devices
}
