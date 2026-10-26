//! Screen annotation for shells that capture and draw natively: the shared planner chooses
//! keyframes and crops, the shell renders them into the bundle, and `finish` writes the summary.
use mluva_annotate::{
    bundle::{self, Plan},
    model::{Capture, Transcript, Word},
    timing,
};
use mluva_core::{
    annotations::{AnnotationSettings, AnnotationStore, CONTACT_SHEET, keyframe_file},
    config::AppConfig,
    history::{HistoryInput, HistoryStore},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// A planned bundle waiting for the shell's images.
pub struct Pending {
    capture: Capture,
    transcript: Transcript,
    plan: Plan,
    directory: PathBuf,
}

/// One recording's annotation as the shell captured it. `lumas` holds each frame's grey
/// thumbnail in `frames` order, since the shared model never serializes them.
#[derive(Deserialize)]
pub struct Request {
    pub owner: String,
    pub capture: Capture,
    #[serde(default)]
    pub lumas: Vec<Vec<f32>>,
    pub text: String,
    #[serde(default)]
    pub words: Vec<Word>,
}

pub fn settings_path(config: &Path) -> PathBuf {
    config.join(AnnotationSettings::FILE)
}

/// Reserve the recording's bundle and choose its keyframes. The reply tells the shell what to
/// draw: each keyframe's source frame, crop and marks, and the files to write.
pub fn plan(
    store: &AnnotationStore,
    pending: &mut HashMap<String, Pending>,
    request: Request,
) -> Result<Value, String> {
    let Request {
        owner,
        mut capture,
        lumas,
        text,
        words,
    } = request;
    if lumas.len() == capture.frames.len() {
        for (frame, luma) in capture.frames.iter_mut().zip(lumas) {
            frame.luma = luma;
        }
    }
    let transcript = timing::transcript(&text, Some(words), &capture.preview, capture.duration_ms);
    let plan = bundle::plan(&capture, &transcript).map_err(|error| error.to_string())?;
    let annotation = store.create(&owner).map_err(|error| error.to_string())?;
    let keyframes: Vec<Value> = plan
        .keyframes
        .iter()
        .zip(&plan.crops)
        .map(|(keyframe, crop)| {
            json!({
                "keyframe": keyframe,
                "crop": crop,
                "file": annotation.directory.join(keyframe_file(keyframe.index)),
            })
        })
        .collect();
    let reply = json!({
        "identifier": annotation.identifier,
        "marks": plan.marks,
        "keyframes": keyframes,
        "contact_sheet": annotation.directory.join(CONTACT_SHEET),
    });
    pending.insert(
        annotation.identifier.clone(),
        Pending {
            capture,
            transcript,
            plan,
            directory: annotation.directory,
        },
    );
    Ok(reply)
}

/// Write the summary and data beside the images the shell drew, and return the summary.
pub fn finish(
    store: &AnnotationStore,
    pending: &mut HashMap<String, Pending>,
    identifier: &str,
) -> Result<Value, String> {
    let Some(planned) = pending.remove(identifier) else {
        return Err("This screen annotation is no longer being written.".into());
    };
    let frames: Vec<PathBuf> = planned
        .plan
        .keyframes
        .iter()
        .map(|keyframe| planned.directory.join(keyframe_file(keyframe.index)))
        .collect();
    let contact_sheet = planned.directory.join(CONTACT_SHEET);
    let written = bundle::finish(
        &planned.directory,
        &planned.capture,
        &planned.transcript,
        &planned.plan,
        frames,
        contact_sheet,
    )
    .and_then(|written| {
        if store.is_complete(identifier) {
            Ok(written)
        } else {
            Err(bundle::BundleError::NoFrames)
        }
    });
    match written {
        Ok(written) => {
            let _ = store.set_keyframes(identifier, written.frames.len());
            Ok(json!({"summary": written.summary, "directory": written.directory}))
        }
        Err(error) => {
            let _ = store.delete(identifier);
            Err(error.to_string())
        }
    }
}

/// Drop a planned or unfinished bundle.
pub fn discard(
    store: &AnnotationStore,
    pending: &mut HashMap<String, Pending>,
    identifier: &str,
) -> Result<Value, String> {
    pending.remove(identifier);
    store
        .delete(identifier)
        .map(|()| Value::Null)
        .map_err(|error| error.to_string())
}

/// A conversation's bundles for History, newest first, with their keyframe images.
pub fn recent(store: &AnnotationStore, conversation: &str) -> Result<Value, String> {
    let bundles = store
        .recent(conversation, false)
        .map_err(|error| error.to_string())?;
    Ok(Value::Array(
        bundles
            .into_iter()
            .filter(|bundle| store.is_complete(&bundle.identifier))
            .map(|bundle| {
                let files = store
                    .keyframe_files(&bundle.identifier)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|file| bundle.directory.join(file))
                    .collect::<Vec<_>>();
                json!({
                    "identifier": bundle.identifier,
                    "created_at": bundle.created_at,
                    "directory": bundle.directory,
                    "keyframes": files,
                    "contact_sheet": bundle.directory.join(CONTACT_SHEET),
                })
            })
            .collect(),
    ))
}

/// At open: bundles a quit left with their recording are kept when complete, since their
/// summary may already have been delivered, and given to a recovered conversation.
pub fn recover(history: &HistoryStore, config: &AppConfig) -> Result<(), String> {
    if config.incognito_mode {
        return Ok(());
    }
    let store = AnnotationStore::new(&history.database.path);
    let text = |error: mluva_core::database::StoreError| error.to_string();
    for capture in store.pending_captures().map_err(text)? {
        for bundle in store.recent(&capture, true).map_err(text)? {
            if !store.is_complete(&bundle.identifier) {
                store.delete(&bundle.identifier).map_err(text)?;
            }
        }
        if store.recent(&capture, true).map_err(text)?.is_empty() {
            continue;
        }
        let recovered = history
            .add(HistoryInput {
                language_code: config.language_code.clone(),
                delivery_outcome: "failed".into(),
                ..HistoryInput::dictation("", "Screen annotation from an interrupted recording.")
            })
            .map_err(text)?;
        store
            .bind_capture(&capture, &recovered.identifier)
            .map_err(text)?;
    }
    Ok(())
}
