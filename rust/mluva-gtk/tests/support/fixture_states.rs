//! Store invariant observation fields once; assertions still receive complete release states.
use serde_json::Value;

fn union(target: &mut Value, shared: &Value) {
    let target = target.as_object_mut().expect("observation object");
    for (key, value) in shared.as_object().expect("shared observation object") {
        match target.get_mut(key) {
            Some(existing) => {
                assert!(
                    existing.is_object() && value.is_object(),
                    "shared observation overlaps a case field: {key}"
                );
                union(existing, value);
            }
            None => {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}

pub fn load(source: &str, groups: &[&str], observation: &str) -> Value {
    let mut document: Value = serde_json::from_str(source).unwrap();
    let mut shared = document
        .as_object_mut()
        .unwrap()
        .remove("common_observations")
        .unwrap();
    for group in groups {
        let common = shared.as_object_mut().unwrap().remove(*group).unwrap();
        for case in document[*group].as_array_mut().unwrap() {
            let mut case_common = case
                .as_object_mut()
                .unwrap()
                .remove("common_observation")
                .unwrap();
            union(&mut case_common, &common);
            for stage in case["stages"].as_array_mut().unwrap() {
                union(&mut stage[observation], &case_common);
            }
        }
    }
    assert!(
        shared.as_object().unwrap().is_empty(),
        "unread observation group"
    );
    document
}
