//! A private, stable Markdown path for dictation and its local image references.
use std::{fs, path::PathBuf};

use chrono::DateTime;

use crate::{
    annotations::AnnotationStore,
    conversation::ConversationStore,
    database::{StoreResult, invalid},
    private_files::atomic_write_private_durable,
    screenshots::{Screenshot, ScreenshotStore},
};

impl ConversationStore {
    pub fn markdown_path(&self, identifier: &str) -> StoreResult<PathBuf> {
        if !uuid::Uuid::parse_str(identifier).is_ok_and(|id| id.to_string() == identifier) {
            return Err(invalid("Invalid conversation identifier."));
        }
        let directory = self.history.database.path.with_file_name("notes");
        let path = directory.join(format!("{identifier}.md"));
        if directory.is_symlink() || path.is_symlink() {
            return Err(invalid(
                "The note path must stay in Mluva's private directory.",
            ));
        }
        Ok(path)
    }

    pub fn remove_markdown(&self, identifier: &str) -> StoreResult<()> {
        match fs::remove_file(self.markdown_path(identifier)?) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => Ok(result?),
        }
    }

    pub fn export_markdown(&self, identifier: &str) -> StoreResult<PathBuf> {
        let entry = self.history.find(identifier)?;
        let path = self.markdown_path(identifier)?;
        let working = self.source_text(&entry, false)?;
        let title = entry
            .title
            .as_deref()
            .unwrap_or("Dictation")
            .replace(['\n', '\r'], " ");
        let mut output =
            format!("# {title}\n\nNote: `{identifier}`\n\n## Dictation\n\n{working}\n");
        let segments = self.history.continuations(identifier)?;
        let store = ScreenshotStore::new(&self.history.database.path);
        let mut images = store.recent(identifier, false)?;
        for segment in &segments {
            images.extend(store.recent(&segment.identifier, false)?);
        }
        images.sort_by(|a, b| (&a.created_at, &a.identifier).cmp(&(&b.created_at, &b.identifier)));
        if !images.is_empty() {
            output.push_str("\n## Screenshots and narration\n\nTimes use UTC and elapsed time from the identified recording's start. Speech excerpts are the live preview visible at that moment, not word-level alignment. An image added after recording has no recording offset.\n");
        }
        for (index, image) in images.iter().enumerate() {
            let number = index + 1;
            output.push_str(&format!(
                "\n### Screenshot {number}\n\n> Screenshot {number} taken {}.\n",
                event_time(image, &image.created_at)
            ));
            if let Some(id) = &image.recording_identifier {
                output.push_str(&format!("\nRecording: `{id}`\n"));
            }
            output.push_str(&format!(
                "\nSaved in editor: {}\n\nEditor closed: {}\n",
                image
                    .saved_at
                    .as_deref()
                    .map(|time| event_time(image, time))
                    .unwrap_or_else(
                        || "No editor save observed; the captured image is retained.".into()
                    ),
                image
                    .closed_at
                    .as_deref()
                    .map(|time| event_time(image, time))
                    .unwrap_or_else(|| "Not observed.".into())
            ));
            for (label, excerpt) in [
                ("Speech visible at capture", &image.capture_narration),
                ("Speech visible at save", &image.save_narration),
            ] {
                if let Some(excerpt) = excerpt.as_ref().filter(|text| !text.trim().is_empty()) {
                    output.push_str(&format!("\n{label}:\n\n"));
                    for line in excerpt.lines() {
                        output.push_str(&format!("> {line}\n"));
                    }
                }
            }
            output.push_str(&format!(
                "\n![Screenshot {number}](../screenshots/{}.png)\n",
                image.identifier
            ));
        }
        let annotations = AnnotationStore::new(&self.history.database.path);
        let mut bundles = annotations.recent(identifier, false)?;
        for segment in &segments {
            bundles.extend(annotations.recent(&segment.identifier, false)?);
        }
        bundles.sort_by(|a, b| (&a.created_at, &a.identifier).cmp(&(&b.created_at, &b.identifier)));
        if !bundles.is_empty() {
            output.push_str("\n## Screen annotations\n\nKeyframes chosen from marks drawn while speaking, with the speech heard while each was on screen. Times are elapsed from that recording's start.\n");
        }
        for (index, bundle) in bundles.iter().enumerate() {
            output.push_str(&format!("\n### Annotation {}\n\n", index + 1));
            if let Some(summary) = annotations.summary(&bundle.identifier)? {
                output.push_str(summary.trim_end());
                output.push('\n');
            }
            let frames = annotations.keyframe_files(&bundle.identifier)?;
            for (number, file) in frames.iter().enumerate() {
                output.push_str(&format!(
                    "\n![Keyframe {}](../annotations/{}/{file})\n",
                    number + 1,
                    bundle.identifier
                ));
            }
        }
        output.push_str(&format!(
            "\n## Original recognition\n\n{}\n",
            entry.raw_text
        ));
        for segment in segments {
            output.push_str(&format!(
                "\n### Recording `{}`\n\n{}\n",
                segment.identifier, segment.raw_text
            ));
        }
        for reply in self.replies(identifier)? {
            output.push_str(&format!("\n## Saved reply\n\n{}\n", reply.text));
        }
        atomic_write_private_durable(&path, output.as_bytes())?;
        Ok(path)
    }
}

fn event_time(image: &Screenshot, value: &str) -> String {
    let offset = image.captured_after_seconds.and_then(|offset| {
        let captured = DateTime::parse_from_rfc3339(&image.created_at).ok()?;
        let event = DateTime::parse_from_rfc3339(value).ok()?;
        Some(offset + (event - captured).num_milliseconds() as f64 / 1000.0)
    });
    match offset {
        Some(seconds) => format!("at +{seconds:.2} s ({value})"),
        None => format!("at {value}"),
    }
}
