//! Frozen picker ownership, external editors and capture-to-conversation commits.
use super::*;
use mluva_core::{
    database::{StoreError, StoreResult},
    history::HistoryInput,
    screenshots::ScreenshotContext,
};
use mluva_workflows::{
    capture::CapturePhase,
    dictation::WorkflowResult,
    screenshot_capture::{ScreenshotCapture, ScreenshotCaptureResult},
};
use std::{
    collections::BTreeMap,
    future::Future,
    io,
    pin::Pin,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime},
};
use tokio_util::sync::CancellationToken;

struct PendingScreenshot {
    owner: String,
    capture: bool,
    offset: Option<f64>,
    requested_at: SystemTime,
    picker: ScreenshotCapture,
    attached: CancellationToken,
}
impl PendingScreenshot {
    fn cancel(&self) {
        self.picker.cancel();
        self.attached.cancel();
    }
}
struct ScreenshotEditor {
    child: Child,
    modified: SystemTime,
    recording: Option<String>,
}
#[derive(Default)]
pub(super) struct ScreenshotOwners {
    pending: RefCell<Option<Rc<PendingScreenshot>>>,
    destinations: RefCell<BTreeMap<String, Option<String>>>,
    editors: RefCell<BTreeMap<String, ScreenshotEditor>>,
    monitors: RefCell<BTreeMap<String, gio::FileMonitor>>,
}
impl Drop for ScreenshotOwners {
    fn drop(&mut self) {
        if let Some(pending) = self.pending.get_mut().take() {
            pending.cancel();
        }
        for monitor in self.monitors.get_mut().values() {
            monitor.cancel();
        }
        // Editors are independent documents. As in the release, quitting Mluva
        // cancels monitoring but leaves those external windows running.
    }
}

impl ApplicationDesktop {
    pub(super) fn request_screenshot(self: &Rc<Self>) {
        let current = self.current.borrow();
        if self.closed.get()
            || self.services.config().incognito_mode
            || current
                .as_ref()
                .is_some_and(|current| current.options.incognito)
        {
            self.shell
                .show_message("Screenshots are unavailable in Incognito.");
            return;
        }
        if self.screenshots.pending.borrow().is_some() {
            return;
        }
        if matches!(
            self.capture.phase(),
            Some(CapturePhase::Processing | CapturePhase::Cancelling)
        ) {
            self.shell
                .show_message("Wait for processing to finish, then add a screenshot.");
            return;
        }
        let capture = current.is_some();
        let owner = current
            .as_ref()
            .map(|current| current.session.identifier.clone())
            .or_else(|| self.workspace().entry().map(|entry| entry.identifier));
        let Some(owner) = owner else {
            self.shell.show_message(
                "Start recording or choose a conversation before adding a screenshot.",
            );
            return;
        };
        let offset = current
            .as_ref()
            .and_then(|current| current.session.elapsed_seconds());
        drop(current);
        if capture {
            self.screenshots
                .destinations
                .borrow_mut()
                .insert(owner.clone(), Some(owner.clone()));
        }
        let pending = Rc::new(PendingScreenshot {
            owner,
            capture,
            offset,
            requested_at: SystemTime::now(),
            picker: ScreenshotCapture::new(&self.services.paths.runtime),
            attached: CancellationToken::new(),
        });
        let run = pending.picker.run();
        self.screenshots.pending.replace(Some(pending.clone()));
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            let result = run.await;
            if let Some(owner) = weak.upgrade() {
                owner.attach_screenshot(&pending, result);
                if pending.capture {
                    owner
                        .screenshots
                        .destinations
                        .borrow_mut()
                        .remove(&pending.owner);
                }
            }
            pending.attached.cancel();
        });
    }
    fn attach_screenshot(
        self: &Rc<Self>,
        pending: &Rc<PendingScreenshot>,
        result: ScreenshotCaptureResult,
    ) {
        if !self
            .screenshots
            .pending
            .borrow()
            .as_ref()
            .is_some_and(|current| Rc::ptr_eq(current, pending))
        {
            return;
        }
        self.screenshots.pending.borrow_mut().take();
        if self.closed.get()
            || self.services.config().incognito_mode
            || pending.picker.is_cancelled()
            || (pending.capture
                && self.current.borrow().as_ref().is_some_and(|current| {
                    current.session.identifier == pending.owner
                        && current.session.phase() == CapturePhase::Cancelling
                }))
        {
            return;
        }
        let data = match result {
            Ok(Some(data)) => data,
            Ok(None) => return,
            Err(_) => {
                self.shell.show_message(
                    "Could not capture a screenshot. Check Omarchy, then press F10 to try again.",
                );
                return;
            }
        };
        let (destination, capture) = if pending.capture {
            let Some(Some(destination)) = self
                .screenshots
                .destinations
                .borrow()
                .get(&pending.owner)
                .cloned()
            else {
                return;
            };
            let capture = destination == pending.owner;
            (destination, capture)
        } else {
            (pending.owner.clone(), false)
        };
        let captured: chrono::DateTime<chrono::Utc> = data.captured_at.into();
        let requested: chrono::DateTime<chrono::Utc> = pending.requested_at.into();
        let screenshot = match self.services.screenshots.add_with_context(
            &destination,
            &data.data,
            capture,
            ScreenshotContext {
                captured_at: Some(captured.to_rfc3339()),
                captured_after_seconds: pending.offset.map(|offset| {
                    (offset + (captured - requested).num_milliseconds() as f64 / 1000.0).max(0.0)
                }),
                recording_identifier: pending.capture.then(|| pending.owner.clone()),
                narration: pending
                    .capture
                    .then(|| self.screenshot_narration(&pending.owner))
                    .flatten(),
            },
        ) {
            Ok(screenshot) => screenshot,
            Err(_) => {
                self.shell.show_message("Could not attach the screenshot. The conversation may be full or no longer available.");
                return;
            }
        };
        let exported = self
            .services
            .screenshots
            .refresh_markdown(&screenshot.identifier)
            .is_ok();
        self.refresh_screenshots();
        self.shell.show_message(if exported {
            "Screenshot attached."
        } else {
            "Image attached; the Markdown file could not be updated. Use Copy as path to retry."
        });
        self.edit_screenshot(&screenshot.identifier);
    }
    fn refresh_screenshots(&self) {
        let current = self.current.borrow();
        self.workspace().set_screenshot_context(
            current
                .as_ref()
                .map(|current| current.session.identifier.clone()),
            current
                .as_ref()
                .and_then(|current| current.continuation.clone()),
        );
    }
    pub(super) fn wait_for_screenshot(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = WorkflowOutcome<()>>>> {
        let pending = self
            .screenshots
            .pending
            .borrow()
            .as_ref()
            .filter(|pending| pending.capture && pending.owner == id)
            .cloned();
        Box::pin(async move {
            if let Some(pending) = pending {
                tokio::time::timeout(Duration::from_secs(180),pending.attached.cancelled()).await.map_err(|_|WorkflowError::Invalid("Finish or cancel the screenshot selection before processing your narration.".into()))?;
            }
            Ok(())
        })
    }
    pub(super) fn cancel_screenshot_picker(&self) {
        if let Some(pending) = self.screenshots.pending.borrow().as_ref() {
            pending.cancel();
        }
    }
    pub(super) fn close_screenshot_monitoring(&self) {
        self.cancel_screenshot_picker();
        for (_, monitor) in std::mem::take(&mut *self.screenshots.monitors.borrow_mut()) {
            monitor.cancel();
        }
    }
    pub(super) fn edit_screenshot(self: &Rc<Self>, id: &str) {
        if self.closed.get() || self.services.config().incognito_mode {
            return;
        }
        if self
            .screenshots
            .editors
            .borrow_mut()
            .get_mut(id)
            .is_some_and(|child| matches!(child.child.try_wait(), Ok(None)))
        {
            self.shell
                .show_message("This screenshot is already open in the editor.");
            return;
        }
        let opened = (|| -> Result<(), Box<dyn std::error::Error>> {
            let path = self.services.screenshots.path_for(id)?;
            if !path.is_file() {
                return Err(io::Error::new(io::ErrorKind::NotFound, "Missing screenshot").into());
            }
            let executable = if self.binaries.screenshot_editor.is_file() {
                self.binaries.screenshot_editor.as_os_str()
            } else {
                std::ffi::OsStr::new("tensaku-edit")
            };
            let child = Command::new(executable)
                .arg(&path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            let pid = child.id();
            let recording = self
                .current
                .borrow()
                .as_ref()
                .map(|current| current.session.identifier.clone());
            self.screenshots.editors.borrow_mut().insert(
                id.into(),
                ScreenshotEditor {
                    child,
                    modified: std::fs::metadata(&path)?.modified()?,
                    recording,
                },
            );
            let weak = Rc::downgrade(self);
            let editor_id = id.to_owned();
            glib::timeout_add_local(Duration::from_millis(250), move || {
                let Some(owner) = weak.upgrade().filter(|owner| !owner.closed.get()) else {
                    return glib::ControlFlow::Break;
                };
                let closed = {
                    let mut editors = owner.screenshots.editors.borrow_mut();
                    let Some(editor) = editors
                        .get_mut(&editor_id)
                        .filter(|editor| editor.child.id() == pid)
                    else {
                        return glib::ControlFlow::Break;
                    };
                    matches!(editor.child.try_wait(), Ok(Some(_)))
                };
                if owner.observe_screenshot_editor(&editor_id, closed).is_err() {
                    owner.shell.show_message(
                        "Could not update screenshot timing. The image remains attached; retrying.",
                    );
                    return glib::ControlFlow::Continue;
                }
                if closed {
                    owner.screenshots.editors.borrow_mut().remove(&editor_id);
                    owner.refresh_screenshots();
                    glib::ControlFlow::Break
                } else {
                    glib::ControlFlow::Continue
                }
            });
            if !self.screenshots.monitors.borrow().contains_key(id) {
                let monitor = gio::File::for_path(&path)
                    .monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)?;
                let weak = Rc::downgrade(self);
                let editor_id = id.to_owned();
                monitor.connect_changed(move |_, _, _, _| {
                    if let Some(owner) = weak.upgrade().filter(|owner| !owner.closed.get()) {
                        let _ = owner.observe_screenshot_editor(&editor_id, false);
                        owner.refresh_screenshots();
                    }
                });
                self.screenshots
                    .monitors
                    .borrow_mut()
                    .insert(id.into(), monitor);
            }
            Ok(())
        })();
        if opened.is_err() {
            self.shell
                .show_message("Could not open Tensaku. The screenshot is still attached.");
        }
    }
    fn screenshot_narration(&self, recording: &str) -> Option<String> {
        let current = self.current.borrow();
        let session = &current
            .as_ref()
            .filter(|current| current.session.identifier == recording)?
            .session;
        let text = session.preview()?.display_text();
        if text.trim().is_empty() {
            return None;
        }
        Some(
            text.chars()
                .rev()
                .take(1000)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect(),
        )
    }

    fn observe_screenshot_editor(&self, id: &str, closed: bool) -> StoreResult<()> {
        let path = self.services.screenshots.path_for(id)?;
        let modified = std::fs::metadata(&path)?.modified()?;
        let (changed, recording) = {
            let editors = self.screenshots.editors.borrow();
            let Some(editor) = editors.get(id) else {
                return Ok(());
            };
            (editor.modified != modified, editor.recording.clone())
        };
        if !changed && !closed {
            return Ok(());
        }
        if changed {
            let data = std::fs::read(&path)?;
            mluva_core::screenshots::validate_png(&data)?;
        }
        let saved = changed.then(|| {
            let time: chrono::DateTime<chrono::Utc> = modified.into();
            time.to_rfc3339()
        });
        let narration = recording.and_then(|id| self.screenshot_narration(&id));
        self.services.screenshots.record_editor_event(
            id,
            saved.as_deref(),
            closed,
            narration.as_deref(),
        )?;
        if let Some(editor) = self.screenshots.editors.borrow_mut().get_mut(id) {
            editor.modified = modified;
        }
        Ok(())
    }

    pub(super) fn close_screenshot_editor(&self, id: &str) -> StoreResult<()> {
        // Release the process before deleting its file: editors may save once
        // more while handling TERM. Both waits have the release's two-second cap.
        let mut editors = self.screenshots.editors.borrow_mut();
        if let Some(child) = editors.get_mut(id) {
            close_editor(&mut child.child)?;
        }
        editors.remove(id);
        if let Some(monitor) = self.screenshots.monitors.borrow_mut().remove(id) {
            monitor.cancel();
        }
        Ok(())
    }
    fn finish_screenshot_capture(
        &self,
        capture: &str,
        conversation: Option<&str>,
    ) -> StoreResult<()> {
        let waiting = self
            .screenshots
            .pending
            .borrow()
            .as_ref()
            .is_some_and(|pending| pending.capture && pending.owner == capture);
        if waiting {
            self.screenshots
                .destinations
                .borrow_mut()
                .insert(capture.into(), conversation.map(Into::into));
        } else {
            self.screenshots.destinations.borrow_mut().remove(capture);
        }
        if let Some(conversation) = conversation {
            self.services
                .screenshots
                .bind_capture(capture, conversation)?;
        } else {
            if waiting {
                self.cancel_screenshot_picker();
            }
            for screenshot in self.services.screenshots.recent(capture, true)? {
                self.close_screenshot_editor(&screenshot.identifier)?;
            }
            self.services.screenshots.delete_owner(capture, true)?;
        }
        self.workspace().set_screenshot_context(None, None);
        Ok(())
    }
    pub(super) fn discard_screenshots(&self, capture: &str) {
        if let Err(error) = self.finish_screenshot_capture(capture, None) {
            self.shell
                .show_message(&format!("Could not remove screenshots: {error}"));
        }
    }
    pub(super) fn finish_result_screenshots(
        &self,
        capture: &str,
        conversation: Option<&str>,
        incognito: bool,
    ) {
        // A failed metadata write must leave recoverable images for next launch.
        // Explicit Cancel/Incognito still discard through their separate path.
        if conversation.is_none() && !incognito {
            match self.services.screenshots.recent(capture, true) {
                Ok(images) if images.is_empty() => {}
                _ => {
                    self.workspace().set_screenshot_context(None, None);
                    return;
                }
            }
        }
        if let Err(error) = self.finish_screenshot_capture(capture, conversation) {
            self.shell
                .show_message(&format!("Could not save screenshot ownership: {error}"));
        }
    }
    pub(super) fn prepare_screenshot_result(
        &self,
        session: &CaptureSession,
        result: &mut WorkflowOutcome<WorkflowResult>,
    ) {
        if session.options.incognito {
            return;
        }
        let has_images = self
            .services
            .screenshots
            .recent(&session.identifier, true)
            .is_ok_and(|images| !images.is_empty());
        if !has_images {
            return;
        }
        let language = self.services.config().language_code;
        let policy = match session.options.audio_retention {
            mluva_core::config::AudioRetentionPolicy::Never => "never",
            mluva_core::config::AudioRetentionPolicy::Failures => "failures",
            mluva_core::config::AudioRetentionPolicy::Always => "always",
        };
        let save = match result {
            Ok(result) if result.history_entry.is_none() && !result.incognito => self
                .services
                .history
                .add(HistoryInput {
                    raw_text: result.transcription.text.clone(),
                    delivered_text: result.output_text.clone(),
                    mode: result.mode.clone(),
                    language_code: language,
                    delivery_outcome: "saved".into(),
                    ..Default::default()
                })
                .map(|entry| result.history_entry = Some(entry)),
            Err(WorkflowError::Failure(failure)) if failure.history_entry.is_none() => self
                .services
                .history
                .add(HistoryInput {
                    delivered_text: failure.output_text.clone(),
                    mode: "dictation".into(),
                    language_code: language,
                    delivery_outcome: "failed".into(),
                    retained_audio_path: failure
                        .retained_audio_path
                        .as_ref()
                        .map(|path| path.to_string_lossy().into_owned()),
                    audio_retention_policy: Some(policy.into()),
                    ..Default::default()
                })
                .map(|entry| failure.history_entry = Some(entry)),
            _ => Ok(()),
        };
        if save.is_err() {
            self.shell.show_message(
                "Could not save screenshot ownership. The images remain available for recovery.",
            );
        }
    }
    pub(super) fn preserve_interrupted_screenshots(&self, capture: &str) {
        if self.services.config().incognito_mode {
            self.discard_screenshots(capture);
            return;
        }
        let result = (|| -> StoreResult<()> {
            let mut destination = self
                .current
                .borrow()
                .as_ref()
                .and_then(|current| current.continuation.clone());
            if destination.is_none() && !self.services.screenshots.recent(capture, true)?.is_empty()
            {
                destination = Some(
                    self.services
                        .history
                        .add(HistoryInput {
                            delivered_text: "Screenshots from an interrupted narration.".into(),
                            mode: "dictation".into(),
                            language_code: self.services.config().language_code,
                            delivery_outcome: "failed".into(),
                            ..Default::default()
                        })?
                        .identifier,
                );
            }
            self.finish_screenshot_capture(capture, destination.as_deref())
        })();
        if result.is_err() {
            self.shell.show_message(
                "Could not save screenshot ownership. The images remain available for recovery.",
            );
        }
    }
}
fn close_editor(child: &mut Child) -> StoreResult<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    if unsafe { libc::kill(child.id() as i32, libc::SIGTERM) } != 0 {
        return Err(io::Error::last_os_error().into());
    }
    let wait = |child: &mut Child| -> io::Result<bool> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if child.try_wait()?.is_some() {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(5));
        }
    };
    if !wait(child)? {
        child.kill()?;
        if !wait(child)? {
            return Err(StoreError::Invalid(
                "Screenshot editor did not close. Try removing the image again.".into(),
            ));
        }
    }
    Ok(())
}
