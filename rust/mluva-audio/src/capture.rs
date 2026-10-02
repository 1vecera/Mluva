//! One recording's process and memory-backed staging, owned by a non-UI thread.

use crate::{
    AudioCaptureError, Result,
    recorder::{AudioChunkCallback, PipeWireRecorder},
    volatile::VolatileAudioStore,
};
use std::{
    path::{Component, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};
use tokio::sync::oneshot;

pub enum CaptureStorage {
    Persistent(PathBuf),
    Incognito {
        cleanup_executable: PathBuf,
        memory_root: Option<PathBuf>,
    },
}

enum Request {
    Destination(CaptureStorage, String, oneshot::Sender<Result<PathBuf>>),
    Start(
        PathBuf,
        Option<AudioChunkCallback>,
        oneshot::Sender<Result<()>>,
    ),
    Stop(oneshot::Sender<Result<PathBuf>>),
    Cancel(oneshot::Sender<()>),
    Close(Option<oneshot::Sender<()>>),
}

struct Snapshot {
    active: AtomicBool,
    level: AtomicU64,
}

/// Calls return only after their process operation has finished. Dropping an
/// awaiting caller does not strand a child: the owner still drains the request,
/// and dropping this service queues shutdown. Normal controllers await `close`.
pub struct CaptureRecorder {
    requests: Mutex<Option<mpsc::Sender<Request>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    snapshot: std::sync::Arc<Snapshot>,
}

impl CaptureRecorder {
    pub fn new(recorder: PipeWireRecorder) -> Result<Self> {
        let (requests, receiver) = mpsc::channel();
        let snapshot = std::sync::Arc::new(Snapshot {
            active: AtomicBool::new(false),
            level: AtomicU64::new(0.0_f64.to_bits()),
        });
        let state = snapshot.clone();
        let worker = thread::Builder::new()
            .name("mluva-capture-owner".into())
            .spawn(move || own(recorder, receiver, state))?;
        Ok(Self {
            requests: Mutex::new(Some(requests)),
            worker: Mutex::new(Some(worker)),
            snapshot,
        })
    }

    pub fn active(&self) -> bool {
        self.snapshot.active.load(Ordering::Acquire)
    }
    pub fn audio_level(&self) -> f64 {
        f64::from_bits(self.snapshot.level.load(Ordering::Relaxed))
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

    pub async fn start(&self, path: PathBuf, callback: Option<AudioChunkCallback>) -> Result<()> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Start(path, callback, reply))?;
        response.await.map_err(|_| stopped())?
    }

    pub async fn stop(&self) -> Result<PathBuf> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Stop(reply))?;
        response.await.map_err(|_| stopped())?
    }

    pub async fn cancel(&self) -> Result<()> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Cancel(reply))?;
        response.await.map_err(|_| stopped())
    }

    /// Reap the microphone and memory-backed janitor before releasing ownership.
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

impl Drop for CaptureRecorder {
    fn drop(&mut self) {
        if let Some(sender) = self.requests.get_mut().unwrap().take() {
            let _ = sender.send(Request::Close(None));
        }
    }
}

fn stopped() -> AudioCaptureError {
    AudioCaptureError::Message("The microphone capture service stopped.")
}

fn own(
    mut recorder: PipeWireRecorder,
    receiver: mpsc::Receiver<Request>,
    snapshot: std::sync::Arc<Snapshot>,
) {
    let mut volatile = None;
    let mut destination: Option<PathBuf> = None;
    while let Ok(request) = receiver.recv() {
        match request {
            Request::Destination(storage, filename, reply) => {
                let result = (|| {
                    if destination.is_some() {
                        return Err(AudioCaptureError::Message(
                            "This capture already has an audio destination.",
                        ));
                    }
                    let name = PathBuf::from(filename);
                    let mut components = name.components();
                    if !matches!(components.next(), Some(Component::Normal(_)))
                        || components.next().is_some()
                    {
                        return Err(AudioCaptureError::Message(
                            "The capture filename must name one private file.",
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
                            directory.join("recordings")
                        }
                    };
                    let path = root.join(name);
                    destination = Some(path.clone());
                    Ok(path)
                })();
                let _ = reply.send(result);
            }
            Request::Start(path, callback, reply) => {
                let state = snapshot.clone();
                let mut callback = callback;
                let result = if destination.as_ref() != Some(&path) {
                    Err(AudioCaptureError::Message(
                        "This audio destination does not belong to the capture.",
                    ))
                } else if let Some(store) = volatile.as_mut() {
                    store.directory().map(|_| ()).map_err(Into::into)
                } else {
                    Ok(())
                }
                .and_then(|()| {
                    recorder.start(
                        &path,
                        Some(Box::new(move |frames, level| {
                            state.level.store(level.to_bits(), Ordering::Relaxed);
                            match callback.as_mut() {
                                Some(callback) => callback(frames, level),
                                None => Ok(()),
                            }
                        })),
                    )
                });
                snapshot.active.store(recorder.active(), Ordering::Release);
                let _ = reply.send(result);
            }
            Request::Stop(reply) => {
                let result = recorder.stop();
                snapshot.active.store(false, Ordering::Release);
                snapshot.level.store(0.0_f64.to_bits(), Ordering::Relaxed);
                let _ = reply.send(result);
            }
            Request::Cancel(reply) => {
                recorder.cancel();
                snapshot.active.store(false, Ordering::Release);
                snapshot.level.store(0.0_f64.to_bits(), Ordering::Relaxed);
                if let Some(path) = &destination {
                    let _ = std::fs::remove_file(path);
                }
                let _ = reply.send(());
            }
            Request::Close(reply) => {
                recorder.cancel();
                snapshot.active.store(false, Ordering::Release);
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
    snapshot.active.store(false, Ordering::Release);
}
