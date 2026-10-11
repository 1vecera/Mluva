//! Cloudflare-authenticated PCM requests cross only the same-user desktop socket.
use crate::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use mluva_core::phone::{self, PhoneReply, PhoneRequest};
use serde::Deserialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

pub(super) async fn call(
    state: &AppState,
    request: PhoneRequest,
) -> Result<PhoneReply, &'static str> {
    let path = state
        .phone_socket
        .as_ref()
        .ok_or("Open Mluva on the PC to use its recording widget.")?;
    tokio::time::timeout(Duration::from_secs(12), async {
        let mut stream = UnixStream::connect(path).await?;
        let bytes = serde_json::to_vec(&request)?;
        stream.write_u32(bytes.len() as u32).await?;
        stream.write_all(&bytes).await?;
        let count = stream.read_u32().await? as usize;
        if count > 2 * 1024 * 1024 {
            return Err(std::io::Error::other("Oversized response."));
        }
        let mut bytes = vec![0; count];
        stream.read_exact(&mut bytes).await?;
        serde_json::from_slice(&bytes).map_err(std::io::Error::from)
    })
    .await
    .map_err(|_| "Mluva on the PC did not respond. Keep the phone recording.")?
    .map_err(|_| "Open or update Mluva on the PC to use live microphone mode.")
}
fn response(result: Result<PhoneReply, &'static str>) -> Response {
    match result {
        Ok(reply) if reply.phase == "error" => error(StatusCode::CONFLICT, &reply.message),
        Ok(reply) => Json(reply).into_response(),
        Err(message) => error(StatusCode::SERVICE_UNAVAILABLE, message),
    }
}
pub async fn start(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let id = match recording_id(&id) {
        Ok(id) => id,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    match state.completed(&id) {
        Ok(Some(entry)) => return Json(receipt(id, entry)).into_response(),
        Err(message) => return error(StatusCode::INTERNAL_SERVER_ERROR, &message),
        _ => {}
    }
    let mut result = call(
        &state,
        PhoneRequest::Start {
            identifier: id.clone(),
        },
    )
    .await;
    if result.is_err() {
        // This flag initializes a resident desktop without presenting its window.
        if let Some(executable) = mluva_core::executables::find_executable("mluva")
            && Command::new(executable)
                .arg("--phone-background")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .is_ok()
        {
            for _ in 0..40 {
                tokio::time::sleep(Duration::from_millis(250)).await;
                result = call(
                    &state,
                    PhoneRequest::Start {
                        identifier: id.clone(),
                    },
                )
                .await;
                if result.is_ok() {
                    break;
                }
            }
        }
    }
    response(result)
}
pub async fn audio(
    State(state): State<Arc<AppState>>,
    Path((id, sequence)): Path<(String, u64)>,
    audio: Bytes,
) -> Response {
    let id = match recording_id(&id) {
        Ok(id) => id,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    if audio.is_empty() || audio.len() > phone::MAX_CHUNK_BYTES || !audio.len().is_multiple_of(2) {
        return error(StatusCode::BAD_REQUEST, "Expected 16 kHz mono PCM16 audio.");
    }
    response(
        call(
            &state,
            PhoneRequest::Audio {
                identifier: id,
                sequence,
                pcm: STANDARD.encode(audio),
            },
        )
        .await,
    )
}
#[derive(Deserialize)]
pub struct Stop {
    sequence: u64,
}
pub async fn stop(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(stop): Json<Stop>,
) -> Response {
    let id = match recording_id(&id) {
        Ok(id) => id,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    response(
        call(
            &state,
            PhoneRequest::Stop {
                identifier: id,
                sequence: stop.sequence,
            },
        )
        .await,
    )
}
pub async fn cancel(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let id = match recording_id(&id) {
        Ok(id) => id,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    response(call(&state, PhoneRequest::Cancel { identifier: id }).await)
}
pub async fn status(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let id = match recording_id(&id) {
        Ok(id) => id,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    // Survives restarts for completed non-Incognito native recordings.
    match state.completed(&id) {
        Ok(Some(entry)) => return Json(receipt(id, entry)).into_response(),
        Err(message) => return error(StatusCode::INTERNAL_SERVER_ERROR, &message),
        _ => {}
    }
    let result = call(&state, PhoneRequest::Status { identifier: id }).await;
    if let Ok(reply) = &result
        && reply.phase == "completed"
    {
        let mut jobs = state.jobs.lock().await;
        if jobs.len() >= 50 {
            jobs.retain(|_, job| job.phase == "processing");
        }
        jobs.insert(
            reply.identifier.clone(),
            Job {
                identifier: reply.identifier.clone(),
                phase: reply.phase.clone(),
                text: reply.text.clone(),
                copied: reply.copied,
                message: reply.message.clone(),
            },
        );
    }
    response(result)
}
