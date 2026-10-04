//! One Meeting's two processes and staging, finalized away from the GTK thread.

use crate::{
    AudioCaptureError, Result,
    capture::CaptureStorage,
    meeting::{MeetingCaptureResult, PipeWireMeetingRecorder},
    volatile::VolatileAudioStore,
};
use std::{
    path::{Component, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};
use tokio::sync::oneshot;

enum Request {
    Destination(CaptureStorage, String, oneshot::Sender<Result<PathBuf>>),
    Start(PathBuf, oneshot::Sender<Result<()>>),
    Stop(oneshot::Sender<Result<MeetingCaptureResult>>),
    Retain(oneshot::Sender<Result<()>>),
    Cancel(oneshot::Sender<()>),
    Close(Option<oneshot::Sender<()>>),
}

/// Destination ownership stays here after Stop. A persisted result or an
/// explicitly retained recovery failure must call `retain_audio` before close.
/// Dropping a caller during Stop cannot strand a newly finalized recording.
pub struct MeetingRecorder {
    requests: Mutex<Option<mpsc::Sender<Request>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    active: Arc<AtomicBool>,
}
impl MeetingRecorder {
    pub fn new(recorder: PipeWireMeetingRecorder) -> Result<Self> {
        let (requests, receiver) = mpsc::channel();
        let active = Arc::new(AtomicBool::new(false));
        let state = active.clone();
        let worker = thread::Builder::new()
            .name("mluva-meeting-owner".into())
            .spawn(move || own(recorder, receiver, state))?;
        Ok(Self {
            requests: Mutex::new(Some(requests)),
            worker: Mutex::new(Some(worker)),
            active,
        })
    }
    pub fn active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
    pub async fn prepare_destination(
        &self,
        storage: CaptureStorage,
        filename: String,
    ) -> Result<PathBuf> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Destination(storage, filename, reply))?;
        response.await.map_err(|_| stopped())?
    }
    pub async fn start(&self, path: PathBuf) -> Result<()> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Start(path, reply))?;
        response.await.map_err(|_| stopped())?
    }
    pub async fn stop(&self) -> Result<MeetingCaptureResult> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Stop(reply))?;
        response.await.map_err(|_| stopped())?
    }
    /// Transfer finalized persistent audio to the archive/recovery owner. The
    /// Incognito staging boundary never accepts persistent retention.
    pub async fn retain_audio(&self) -> Result<()> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Retain(reply))?;
        response.await.map_err(|_| stopped())?
    }
    pub async fn cancel(&self) -> Result<()> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Cancel(reply))?;
        response.await.map_err(|_| stopped())
    }
    pub async fn close(&self) -> Result<()> {
        let (reply, response) = oneshot::channel();
        let sender = self.requests.lock().unwrap().take();
        if let Some(sender) = sender {
            let _ = sender.send(Request::Close(Some(reply)));
            let _ = response.await;
        }
        let worker = self.worker.lock().unwrap().take();
        if let Some(worker) = worker {
            tokio::task::spawn_blocking(move || worker.join())
                .await
                .map_err(|_| stopped())?
                .map_err(|_| stopped())?;
        }
        Ok(())
    }
    fn send(&self, request: Request) -> Result<()> {
        self.requests
            .lock()
            .unwrap()
            .as_ref()
            .ok_or_else(stopped)?
            .send(request)
            .map_err(|_| stopped())
    }
}
impl Drop for MeetingRecorder {
    fn drop(&mut self) {
        if let Some(sender) = self.requests.get_mut().unwrap().take() {
            let _ = sender.send(Request::Close(None));
        }
    }
}
fn stopped() -> AudioCaptureError {
    AudioCaptureError::Message("The Meeting recorder owner is closed.")
}
fn own(
    mut recorder: PipeWireMeetingRecorder,
    receiver: mpsc::Receiver<Request>,
    active: Arc<AtomicBool>,
) {
    let mut destination: Option<PathBuf> = None;
    let mut volatile: Option<VolatileAudioStore> = None;
    let mut output_owned = false;
    let mut finalized = false;
    let mut retained = false;
    let erase = |path: &Option<PathBuf>, owned: bool| {
        if owned && let Some(path) = path {
            let _ = std::fs::remove_file(path);
        }
    };
    while let Ok(request) = receiver.recv() {
        match request {
            Request::Destination(storage, filename, reply) => {
                let result = (|| {
                    if destination.is_some() {
                        return Err(AudioCaptureError::Message(
                            "This Meeting already has an audio destination.",
                        ));
                    }
                    let name = PathBuf::from(filename);
                    let mut components = name.components();
                    if !matches!(components.next(), Some(Component::Normal(_)))
                        || components.next().is_some()
                    {
                        return Err(AudioCaptureError::Message(
                            "The Meeting filename must name one private file.",
                        ));
                    }
                    let root = match storage {
                        CaptureStorage::Persistent(directory) => directory,
                        CaptureStorage::Incognito {
                            cleanup_executable,
                            memory_root,
                        } => {
                            let mut store = match memory_root {
                                Some(root) => {
                                    VolatileAudioStore::open_in(&root, &cleanup_executable)?
                                }
                                None => VolatileAudioStore::open(&cleanup_executable)?,
                            };
                            let directory = store.directory()?.to_path_buf();
                            volatile = Some(store);
                            directory.join("meetings/recordings")
                        }
                    };
                    let path = root.join(name);
                    destination = Some(path.clone());
                    Ok(path)
                })();
                let _ = reply.send(result);
            }
            Request::Start(path, reply) => {
                let result = if destination.as_ref() != Some(&path) {
                    Err(AudioCaptureError::Message(
                        "This audio destination does not belong to the Meeting.",
                    ))
                } else if output_owned && !recorder.active() {
                    Err(AudioCaptureError::Message(
                        "This Meeting capture has already finished.",
                    ))
                } else if let Some(store) = volatile.as_mut() {
                    store.directory().map(|_| ()).map_err(Into::into)
                } else {
                    Ok(())
                }
                .and_then(|()| recorder.start(&path));
                if result.is_ok() {
                    output_owned = true;
                }
                active.store(recorder.active(), Ordering::Release);
                let _ = reply.send(result);
            }
            Request::Stop(reply) => {
                let result = recorder.stop();
                finalized = result.is_ok();
                active.store(false, Ordering::Release);
                let _ = reply.send(result);
            }
            Request::Retain(reply) => {
                let result = if volatile.is_some() {
                    Err(AudioCaptureError::Message(
                        "Incognito Meeting audio cannot be retained.",
                    ))
                } else if !finalized {
                    Err(AudioCaptureError::Message(
                        "Meeting audio has not finalized.",
                    ))
                } else {
                    retained = true;
                    Ok(())
                };
                let _ = reply.send(result);
            }
            Request::Cancel(reply) => {
                recorder.cancel();
                active.store(false, Ordering::Release);
                erase(&destination, output_owned && !retained);
                finalized = false;
                let _ = reply.send(());
            }
            Request::Close(reply) => {
                recorder.cancel();
                active.store(false, Ordering::Release);
                erase(&destination, output_owned && !retained);
                if let Some(mut store) = volatile.take() {
                    store.close();
                }
                if let Some(reply) = reply {
                    let _ = reply.send(());
                }
                return;
            }
        }
    }
    recorder.cancel();
    active.store(false, Ordering::Release);
    erase(&destination, output_owned && !retained);
}
