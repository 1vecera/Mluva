use mluva_audio::{
    capture::{CaptureRecorder, CaptureStorage},
    wav::WaveReader,
};
use std::{
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
};

#[tokio::test]
async fn phone_pcm_uses_private_audio_and_normal_stop_without_opening_a_device() {
    let root = tempfile::tempdir().unwrap();
    let recorder = CaptureRecorder::phone().unwrap();
    let path = recorder
        .prepare_destination(
            CaptureStorage::Persistent(root.path().join("new-recordings")),
            "phone.wav".into(),
        )
        .await
        .unwrap();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let callback = heard.clone();
    recorder
        .start(
            path.clone(),
            Some(Box::new(move |frames, _| {
                callback.lock().unwrap().extend_from_slice(frames);
                Ok(())
            })),
        )
        .await
        .unwrap();
    assert!(recorder.append_phone_audio(vec![1]).await.is_err());
    assert!(recorder.append_phone_audio(vec![0; 32_002]).await.is_err());
    let frames = [16384_i16.to_le_bytes(), (-16384_i16).to_le_bytes()]
        .concat()
        .repeat(4000);
    recorder.append_phone_audio(frames.clone()).await.unwrap();
    assert_eq!(recorder.audio_level(), 0.5);
    assert_eq!(recorder.stop().await.unwrap(), path);
    assert_eq!(*heard.lock().unwrap(), frames);
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let mut reader = WaveReader::open(&path).unwrap();
    assert_eq!(reader.read_frames(16000).unwrap(), frames);
    assert!(recorder.append_phone_audio(vec![0; 2]).await.is_err());
    recorder.close().await.unwrap();
    assert!(
        path.exists(),
        "finalized audio belongs to the workflow retention owner"
    );
}

#[tokio::test]
async fn two_hours_of_phone_pcm_is_complete_and_the_next_chunk_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let recorder = CaptureRecorder::phone().unwrap();
    let path = recorder
        .prepare_destination(
            CaptureStorage::Persistent(root.path().into()),
            "two-hours.wav".into(),
        )
        .await
        .unwrap();
    recorder.start(path.clone(), None).await.unwrap();
    for _ in 0..7200 {
        recorder.append_phone_audio(vec![0; 32000]).await.unwrap();
    }
    assert!(recorder.append_phone_audio(vec![0; 2]).await.is_err());
    recorder.stop().await.unwrap();
    assert_eq!(WaveReader::open(&path).unwrap().duration_seconds(), 7200.0);
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 44 + 230_400_000);
    recorder.close().await.unwrap();
}
