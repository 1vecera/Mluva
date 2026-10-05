//! Synthetic Responses events for real SDK clients; never supplies app callbacks.
use serde_json::{Value, json};

pub fn events(text: &str, call: Option<Value>) -> Vec<Value> {
    let mut events = vec![
        json!({"type":"response.created","response":{"id":"resp_fixture","object":"response","status":"in_progress"}}),
    ];
    if let Some(call) = call {
        events.extend([json!({"type":"response.output_item.done","output_index":0,"item":call}),json!({"type":"response.completed","response":{"id":"resp_fixture","object":"response","status":"completed","output":[call]}})]);
    } else {
        let item = json!({"id":"msg_fixture","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":text,"annotations":[]}]});
        let mut pending = item.clone();
        pending["status"] = json!("in_progress");
        pending["content"] = json!([]);
        events.extend([
            json!({"type":"response.output_item.added","output_index":0,"item":pending}),
            json!({"type":"response.content_part.added","item_id":"msg_fixture","output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
            json!({"type":"response.output_text.delta","item_id":"msg_fixture","output_index":0,"content_index":0,"delta":text}),
            json!({"type":"response.output_item.done","output_index":0,"item":item}),
            json!({"type":"response.completed","response":{"id":"resp_fixture","object":"response","status":"completed","output":[item],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14}}}),
        ]);
    }
    events
}

pub fn encode(events: &[Value]) -> Vec<u8> {
    events
        .iter()
        .map(|event| {
            format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            )
        })
        .collect::<String>()
        .into_bytes()
}
