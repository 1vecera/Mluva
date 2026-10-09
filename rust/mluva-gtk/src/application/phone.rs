//! A same-user socket hands authenticated phone audio to the ordinary capture owner.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use mluva_core::phone::{self, PhoneReply, PhoneRequest};
use mluva_workflows::{capture::CapturePhase, dictation::WorkflowResult};
use std::{
    collections::BTreeMap,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::{Semaphore, mpsc, oneshot},
};
use tokio_util::sync::CancellationToken;

struct Recording {
    session: Rc<CaptureSession>,
    sequence: u64,
    bytes: usize,
    last_audio: Instant,
    reply: PhoneReply,
}
#[derive(Default)]
pub(super) struct PhoneState {
    recordings: RefCell<BTreeMap<String, Recording>>,
    bridge: RefCell<Option<Bridge>>,
}
struct Bridge {
    path: PathBuf,
    cancellation: CancellationToken,
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.cancellation.cancel();
        let _ = std::fs::remove_file(&self.path);
    }
}
type Call = (PhoneRequest, oneshot::Sender<PhoneReply>);

async fn exchange(mut stream: UnixStream, sender: mpsc::Sender<Call>) -> std::io::Result<()> {
    if stream.peer_cred()?.uid() != unsafe { libc::getuid() } {
        return Err(std::io::Error::other("Wrong peer."));
    }
    let count = stream.read_u32().await? as usize;
    if count > phone::MAX_MESSAGE_BYTES {
        return Err(std::io::Error::other("Oversized request."));
    }
    let mut bytes = vec![0; count];
    stream.read_exact(&mut bytes).await?;
    let request: PhoneRequest = serde_json::from_slice(&bytes)?;
    let (reply, response) = oneshot::channel();
    sender
        .send((request, reply))
        .await
        .map_err(|_| std::io::Error::other("Closed."))?;
    let reply = response
        .await
        .map_err(|_| std::io::Error::other("Closed."))?;
    let bytes = serde_json::to_vec(&reply)?;
    stream.write_u32(bytes.len() as u32).await?;
    stream.write_all(&bytes).await
}

impl ApplicationDesktop {
    pub(super) fn start_phone_bridge(self: &Rc<Self>) -> std::io::Result<()> {
        let path = phone::socket_path()?;
        let root = path.parent().expect("socket parent");
        std::fs::create_dir_all(root)?;
        let metadata = std::fs::symlink_metadata(root)?;
        if !metadata.is_dir() || metadata.uid() != unsafe { libc::getuid() } {
            return Err(std::io::Error::other("Private runtime required."));
        }
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            if !metadata.file_type().is_socket() || metadata.uid() != unsafe { libc::getuid() } {
                return Err(std::io::Error::other("Unowned socket."));
            }
            std::fs::remove_file(&path)?;
        }
        let listener = std::os::unix::net::UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let cancellation = CancellationToken::new();
        let stop = cancellation.clone();
        let (sender, mut calls) = mpsc::channel::<Call>(8);
        self.runtime.spawn_background(async move {
            let Ok(listener) = tokio::net::UnixListener::from_std(listener) else { return; };
            let slots = std::sync::Arc::new(Semaphore::new(8));
            let mut children = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _ = stop.cancelled() => break,
                    Some(_) = children.join_next(), if !children.is_empty() => {},
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { break; };
                        let Ok(permit) = slots.clone().try_acquire_owned() else { continue; };
                        let sender = sender.clone();
                        children.spawn(async move { let _permit = permit; let _ = tokio::time::timeout(Duration::from_secs(10), exchange(stream, sender)).await; });
                    }
                }
            }
            children.abort_all();
            while children.join_next().await.is_some() {}
        });
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            while let Some((request, response)) = calls.recv().await {
                let Some(owner) = weak.upgrade().filter(|owner| !owner.closed.get()) else {
                    break;
                };
                let reply = owner.phone_request(request).await;
                let _ = response.send(reply);
            }
        });
        self.phone
            .bridge
            .replace(Some(Bridge { path, cancellation }));
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_secs(1), move || {
            let Some(owner) = weak.upgrade().filter(|owner| !owner.closed.get()) else {
                return glib::ControlFlow::Break;
            };
            let expired = owner.phone.recordings.borrow().values().any(|recording| {
                owner.capture.session_identifier().as_deref() == Some(&recording.session.identifier)
                    && matches!(
                        recording.session.phase(),
                        CapturePhase::Preparing | CapturePhase::Recording
                    )
                    && recording.last_audio.elapsed()
                        > Duration::from_secs(
                            if recording.session.phase() == CapturePhase::Preparing {
                                120
                            } else {
                                20
                            },
                        )
            });
            if expired {
                owner.capture.cancel();
            }
            glib::ControlFlow::Continue
        });
        Ok(())
    }

    async fn phone_request(self: &Rc<Self>, request: PhoneRequest) -> PhoneReply {
        let id = request.identifier().to_owned();
        if !mluva_core::phone::valid_identifier(&id) {
            return PhoneReply::error(&id, "Invalid recording ID.");
        }
        if matches!(request, PhoneRequest::Start { .. })
            && !self.phone.recordings.borrow().contains_key(&id)
        {
            if self.initialization_pending.get() {
                return PhoneReply {
                    identifier: id,
                    phase: "starting".into(),
                    ..Default::default()
                };
            }
            if self.capture.phase().is_some() || self.review.rewriting() || self.live.finalizing() {
                return PhoneReply::error(&id, "Finish the current PC recording or rewrite first.");
            }
            self.toggle_recording(CaptureOrigin::Phone(id.clone()));
            let Some(session) = self
                .current
                .borrow()
                .as_ref()
                .map(|capture| capture.session.clone())
            else {
                return PhoneReply::error(
                    &id,
                    "Mluva is not ready. Open Mluva on the PC and finish its setup or pending work.",
                );
            };
            let mut recordings = self.phone.recordings.borrow_mut();
            if recordings.len() >= 20 {
                recordings.retain(|_, r| {
                    matches!(
                        r.session.phase(),
                        CapturePhase::Preparing
                            | CapturePhase::Recording
                            | CapturePhase::Processing
                    )
                });
            }
            recordings.insert(
                id.clone(),
                Recording {
                    session: session.clone(),
                    sequence: 0,
                    bytes: 0,
                    last_audio: Instant::now(),
                    reply: PhoneReply {
                        identifier: id.clone(),
                        incognito: session.options.incognito,
                        ..Default::default()
                    },
                },
            );
        }
        let Some(session) = self
            .phone
            .recordings
            .borrow()
            .get(&id)
            .map(|r| r.session.clone())
        else {
            return PhoneReply::error(
                &id,
                "This desktop session is unavailable. Keep the phone audio and retry its transfer.",
            );
        };
        match request {
            PhoneRequest::Audio { sequence, pcm, .. }
                if session.phase() == CapturePhase::Recording =>
            {
                let expected = self.phone.recordings.borrow()[&id].sequence;
                if sequence > expected {
                    return PhoneReply::error(
                        &id,
                        "An audio chunk is missing. Keep the phone recording.",
                    );
                }
                if sequence == expected {
                    let Ok(frames) = STANDARD.decode(pcm) else {
                        return PhoneReply::error(&id, "Invalid PCM audio.");
                    };
                    let count = frames.len();
                    if session.append_phone_audio(frames).await.is_err() {
                        return PhoneReply::error(
                            &id,
                            "Phone audio could not be captured. Keep the phone recording.",
                        );
                    }
                    let mut recordings = self.phone.recordings.borrow_mut();
                    let recording = recordings.get_mut(&id).expect("recording owner");
                    recording.sequence += 1;
                    recording.bytes += count;
                    recording.last_audio = Instant::now();
                    if recording.bytes == phone::MAX_PCM_BYTES {
                        drop(recordings);
                        self.capture.stop();
                    }
                }
            }
            PhoneRequest::Stop { sequence, .. } if session.phase() == CapturePhase::Recording => {
                if sequence != self.phone.recordings.borrow()[&id].sequence {
                    return PhoneReply::error(
                        &id,
                        "Some audio has not arrived. Keep the phone recording.",
                    );
                }
                self.capture.stop();
            }
            PhoneRequest::Cancel { .. }
                if self.capture.session_identifier().as_deref() == Some(&session.identifier) =>
            {
                self.capture.cancel();
            }
            _ => {}
        }
        let mut reply = self.phone.recordings.borrow()[&id].reply.clone();
        reply.sequence = self.phone.recordings.borrow()[&id].sequence;
        if reply.phase.is_empty() {
            reply.phase = match session.phase() {
                CapturePhase::Preparing => "preparing",
                CapturePhase::Recording => "recording",
                CapturePhase::Processing => "processing",
                CapturePhase::Cancelled | CapturePhase::Cancelling => "cancelled",
                CapturePhase::Failed => "failed",
                CapturePhase::Completed => "processing",
            }
            .into();
            reply.text = session
                .preview()
                .map(|preview| preview.display_text())
                .unwrap_or_default()
                .chars()
                .take(4096)
                .collect();
            if reply.phase == "cancelled" || reply.phase == "failed" {
                reply.message =
                    "Desktop recording ended. Keep the audio on your phone to retry or download."
                        .into();
            }
        }
        reply
    }
    pub(super) fn phone_completed(&self, session: &str, result: &WorkflowResult) {
        for recording in self
            .phone
            .recordings
            .borrow_mut()
            .values_mut()
            .filter(|r| r.session.identifier == session)
        {
            recording.reply.phase = "completed".into();
            recording.reply.text = result.output_text.clone();
            recording.reply.copied = result.delivery.copied;
            recording.reply.message = result.delivery.guidance.clone();
        }
    }
    pub(super) fn close_phone_bridge(&self) {
        self.phone.bridge.borrow_mut().take();
    }
}
