//! Herdr-core remote agent discovery.

use mluva_core::agent_voice::{AgentStatus, Host, Target};
use serde_json::Value;

pub(crate) fn parse_list(result: &Value) -> Vec<Target> {
    result["terminals"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|terminal| {
            let address = terminal["address"].as_str()?;
            let (_, id) = address.split_once('/')?;
            if id.is_empty() || terminal["state"] != "live" {
                return None;
            }
            let status = &terminal["status"];
            let kind = crate::herdr::detect_herdr_core_agent(terminal)?;
            let status = match status["state"].as_str() {
                Some("idle") => AgentStatus::Idle,
                Some("working") => AgentStatus::Working,
                Some("done") => AgentStatus::Done,
                Some("blocked" | "error") => AgentStatus::Blocked,
                _ => AgentStatus::Unknown,
            };
            let created_ms = terminal["created_ms"].as_u64()?;
            let process = u32::try_from(
                terminal["foreground"]["pid"]
                    .as_u64()
                    .or_else(|| terminal["pid"].as_u64())?,
            )
            .ok()?;
            if created_ms == 0 || process == 0 {
                return None;
            }
            let name = terminal["name"]
                .as_str()
                .filter(|name| !name.trim().is_empty())
                .or_else(|| terminal["labels"]["where"].as_str())
                .unwrap_or(id);
            let title = terminal["status"]["title"]
                .as_str()
                .filter(|title| !title.trim().is_empty())
                .or_else(|| terminal["title"].as_str())
                .unwrap_or("");
            Some(Target {
                kind,
                name: crate::herdr::label(name, 240),
                title: crate::herdr::label(title, 240),
                project: crate::herdr::label(
                    terminal["foreground"]["cwd"].as_str().unwrap_or(""),
                    240,
                ),
                status,
                host: Host::HerdrCore {
                    address: address.into(),
                    created_ms,
                    process,
                },
                ..Target::default()
            })
        })
        .take(256)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lists_remote_agents_but_not_shells_or_local_terminals() {
        let result = json!({"terminals": [
            {"address":"claw-mini/t8", "state":"live", "created_ms":42,
             "foreground":{"pid":99,"cwd":"/work"}, "status":{"app":"codex","state":"working"},
             "title":"Remote example"},
            {"address":"claw-mini/t9", "state":"live", "created_ms":43,
             "foreground":{"pid":100}, "status":null},
            {"address":"claw-mini/ta", "state":"live", "created_ms":45,
             "foreground":{"pid":102,"name":"codex"}, "status":null},
            {"address":"t2", "state":"live", "created_ms":44,
             "foreground":{"pid":101}, "status":{"app":"codex","state":"idle"}}
        ]});
        let targets = parse_list(&result);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].display_name(), "Remote example");
        assert_eq!(targets[0].place(), "claw-mini · Herdr t8");
        assert!(targets[0].draft_only());
        assert_eq!(targets[1].status, AgentStatus::Unknown);
        assert!(targets[1].draft_only());
    }
}
