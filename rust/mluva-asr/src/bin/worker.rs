//! App-owned offline ONNX process. Its caller owns readiness, cancellation and deadlines.
use mluva_asr::{
    Error, Result,
    onnx::{self, Device, Parakeet, Whisper},
};
use mluva_audio::wav::WaveReader;
use serde::Deserialize;
use std::{
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
struct Request {
    path: PathBuf,
    language: String,
}

enum Engine {
    Whisper(Whisper),
    Parakeet(Parakeet),
}
impl Engine {
    fn recognize(&mut self, audio: &[f32], language: &str) -> Result<String> {
        match self {
            Self::Whisper(engine) => engine.recognize(audio, language),
            Self::Parakeet(engine) => engine.recognize(audio),
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 5 {
        return Err(Error::Model);
    }
    let device = match args[4].to_str() {
        Some("cpu") => Device::Cpu,
        Some("cuda") => Device::Cuda,
        _ => return Err(Error::Model),
    };
    // No decoder may outlive the application that owns its pipes.
    let parent = unsafe { libc::getppid() };
    if parent <= 1
        || unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) } != 0
        || unsafe { libc::getppid() } != parent
    {
        return Err(Error::Inference);
    }
    if device == Device::Cpu {
        let limit = libc::rlimit {
            rlim_cur: 5_000_000_000,
            rlim_max: 5_000_000_000,
        };
        if unsafe { libc::setrlimit(libc::RLIMIT_AS, &limit) } != 0 {
            return Err(Error::Inference);
        }
    }
    onnx::initialize(Path::new(&args[1]))?;
    let directory = Path::new(&args[3]);
    let mut engine = match args[2].to_str() {
        Some("whisper-tiny") => Engine::Whisper(Whisper::load(directory, device)?),
        Some("parakeet-v3") => Engine::Parakeet(Parakeet::load(directory, device)?),
        _ => return Err(Error::Model),
    };
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    loop {
        let mut line = Vec::new();
        let length = input
            .by_ref()
            .take(2_000_001)
            .read_until(b'\n', &mut line)
            .map_err(|_| Error::Inference)?;
        if length == 0 {
            return Ok(());
        }
        if length > 2_000_000 || !line.ends_with(b"\n") {
            return Err(Error::Inference);
        }
        let request: Request = serde_json::from_slice(&line).map_err(|_| Error::Inference)?;
        let mut wave = WaveReader::open(&request.path).map_err(|_| Error::Audio)?;
        if (
            wave.metadata.channels,
            wave.metadata.sample_width,
            wave.metadata.sample_rate,
        ) != (1, 2, 16_000)
        {
            return Err(Error::Audio);
        }
        let mut parts = Vec::new();
        loop {
            let pcm = wave.read_frames(400_000).map_err(|_| Error::Audio)?;
            if pcm.is_empty() {
                break;
            }
            if pcm.len() % 2 != 0 {
                return Err(Error::Audio);
            }
            let audio: Vec<_> = pcm
                .chunks_exact(2)
                .map(|bytes| f32::from(i16::from_le_bytes(bytes.try_into().unwrap())) / 32768.0)
                .collect();
            parts.push(engine.recognize(&audio, &request.language)?);
        }
        let text = parts
            .join(" ")
            .trim_matches(mluva_core::text::whitespace)
            .to_owned();
        serde_json::to_writer(&mut output, &serde_json::json!({"text":text}))
            .map_err(|_| Error::Inference)?;
        output.write_all(b"\n").map_err(|_| Error::Inference)?;
        output.flush().map_err(|_| Error::Inference)?;
    }
}
