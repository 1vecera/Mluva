//! Native ONNX sessions with released allocation/provider and greedy decoding choices.
use crate::{
    Error, Result,
    frontend::{Features, Frontend, Kind},
    tokens::{ParakeetTokens, WhisperTokens},
};
use ort::{
    ep,
    memory::{AllocationDevice, AllocatorType, MemoryInfo, MemoryType},
    session::{
        Session,
        builder::{GraphOptimizationLevel, SessionBuilder},
    },
    value::{DynValue, Tensor, ValueType},
};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Device {
    Cpu,
    Cuda,
}

pub fn initialize(runtime: &Path) -> Result<()> {
    let builder = ort::init_from(runtime).map_err(|_| Error::Inference)?;
    if !builder
        .with_name("MluvaLocal")
        .with_telemetry(false)
        .commit()
    {
        return Err(Error::Inference);
    }
    Ok(())
}

fn builder(device: Device, whisper: bool) -> Result<SessionBuilder> {
    let threads = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) }.clamp(1, 4) as usize;
    let mut options = Session::builder()
        .map_err(|_| Error::Inference)?
        .with_intra_threads(threads)
        .map_err(|_| Error::Inference)?
        .with_inter_threads(1)
        .map_err(|_| Error::Inference)?
        .with_memory_pattern(false)
        .map_err(|_| Error::Inference)?;
    let cpu = ep::CPU::default()
        .with_arena_allocator(false)
        .build()
        .error_on_failure();
    options = if device == Device::Cuda {
        options
            .with_execution_providers([
                ep::CUDA::default()
                    .with_memory_limit(2_500_000_000)
                    .with_conv_max_workspace(false)
                    .with_arena_extend_strategy(ep::ArenaExtendStrategy::SameAsRequested)
                    .build()
                    .error_on_failure(),
                cpu,
            ])
            .map_err(|_| Error::Gpu)?
    } else {
        options
            .with_execution_providers([cpu])
            .map_err(|_| Error::Inference)?
    };
    if device == Device::Cuda && whisper {
        options = options
            .with_optimization_level(GraphOptimizationLevel::Disable)
            .map_err(|_| Error::Inference)?;
    }
    Ok(options)
}

fn output_device(device: Device) -> Result<MemoryInfo<'static>> {
    if device == Device::Cpu {
        return Ok(MemoryInfo::default());
    }
    MemoryInfo::new(
        AllocationDevice::CUDA,
        0,
        AllocatorType::Device,
        MemoryType::Default,
    )
    .map_err(|_| Error::Gpu)
}

fn shape(value: &DynValue) -> Result<&[i64]> {
    match value.dtype() {
        ValueType::Tensor { shape, .. } => Ok(shape),
        _ => Err(Error::Model),
    }
}

fn static_shape(session: &Session, input: &str) -> Result<Vec<i64>> {
    match session
        .inputs()
        .iter()
        .find(|outlet| outlet.name() == input)
        .map(|outlet| outlet.dtype())
    {
        Some(ValueType::Tensor { shape, .. }) => Ok(shape.to_vec()),
        _ => Err(Error::Model),
    }
}

fn argmax(values: &[f32]) -> Result<usize> {
    let mut best = values.first().copied().ok_or(Error::Inference)?;
    let mut index = 0;
    for (next, value) in values.iter().copied().enumerate().skip(1) {
        // NumPy argmax selects the first occurrence, including its first NaN.
        if !best.is_nan() && (value.is_nan() || value > best) {
            best = value;
            index = next;
        }
    }
    Ok(index)
}

enum Preprocessor {
    Cpu(Frontend),
    Cuda(Session),
}
impl Preprocessor {
    fn new(device: Device, kind: Kind) -> Result<Self> {
        if device == Device::Cpu {
            return Ok(Self::Cpu(Frontend::new(kind)));
        }
        let bytes: &[u8] = match kind {
            Kind::Whisper => include_bytes!("../resources/whisper80_conv.onnx"),
            Kind::Parakeet => include_bytes!("../resources/nemo128_conv.onnx"),
        };
        Ok(Self::Cuda(
            builder(device, kind == Kind::Whisper)?
                .commit_from_memory(bytes)
                .map_err(|_| Error::Inference)?,
        ))
    }
    fn extract(&mut self, audio: &[f32]) -> Result<(Tensor<f32>, Tensor<i64>)> {
        match self {
            Self::Cpu(frontend) => {
                let Features {
                    values,
                    bands,
                    frames,
                    valid_frames,
                } = frontend.extract(audio)?;
                Ok((
                    Tensor::from_array(([1, bands, frames], values))
                        .map_err(|_| Error::Inference)?,
                    Tensor::from_array(([1], vec![valid_frames])).map_err(|_| Error::Inference)?,
                ))
            }
            Self::Cuda(session) => {
                if audio.len() > 400_000 || audio.iter().any(|sample| !sample.is_finite()) {
                    return Err(Error::Audio);
                }
                let mut outputs = session.run(ort::inputs![
                    "waveforms" => Tensor::from_array(([1, audio.len()], audio.to_vec())).map_err(|_| Error::Inference)?,
                    "waveforms_lens" => Tensor::from_array(([1], vec![audio.len() as i64])).map_err(|_| Error::Inference)?,
                ]).map_err(|_| Error::Inference)?;
                Ok((
                    outputs
                        .remove("features")
                        .ok_or(Error::Inference)?
                        .downcast()
                        .map_err(|_| Error::Inference)?,
                    outputs
                        .remove("features_lens")
                        .ok_or(Error::Inference)?
                        .downcast()
                        .map_err(|_| Error::Inference)?,
                ))
            }
        }
    }
}

pub struct Whisper {
    frontend: Preprocessor,
    encoder: Session,
    decoder: Session,
    tokens: WhisperTokens,
    device: Device,
}
impl Whisper {
    pub fn load(directory: &Path, device: Device) -> Result<Self> {
        Ok(Self {
            frontend: Preprocessor::new(device, Kind::Whisper)?,
            encoder: builder(device, true)?
                .commit_from_file(directory.join("onnx/encoder_model_quantized.onnx"))
                .map_err(|_| Error::Inference)?,
            decoder: builder(device, true)?
                .commit_from_file(directory.join("onnx/decoder_model_merged_quantized.onnx"))
                .map_err(|_| Error::Inference)?,
            tokens: WhisperTokens::load(directory)?,
            device,
        })
    }
    pub fn recognize(&mut self, audio: &[f32], language: &str) -> Result<String> {
        let forced = if language == "auto" {
            None
        } else {
            Some(self.tokens.language(language)?)
        };
        let (features, _) = self.frontend.extract(audio)?;
        let mut binding = self
            .encoder
            .create_binding()
            .map_err(|_| Error::Inference)?;
        binding
            .bind_input("input_features", &features)
            .map_err(|_| Error::Inference)?;
        binding
            .bind_output_to_device("last_hidden_state", &output_device(self.device)?)
            .map_err(|_| Error::Inference)?;
        let encoding = self
            .encoder
            .run_binding(&binding)
            .map_err(|_| Error::Inference)?
            .remove("last_hidden_state")
            .ok_or(Error::Inference)?;
        let language = match forced {
            Some(language) => language,
            None => *self
                .decode(&encoding, vec![self.tokens.bos], 3)?
                .get(1)
                .ok_or(Error::Inference)?,
        };
        let prompt = self.tokens.transcribe_prompt(language)?;
        let tokens = self.decode(&encoding, prompt, 448)?;
        self.tokens.decode(&tokens)
    }
    fn decode(
        &mut self,
        encoding: &DynValue,
        mut tokens: Vec<i64>,
        maximum: usize,
    ) -> Result<Vec<i64>> {
        let mut state = BTreeMap::<String, DynValue>::new();
        for input in self
            .decoder
            .inputs()
            .iter()
            .filter(|input| input.name().starts_with("past_key_values."))
        {
            let ValueType::Tensor { shape, .. } = input.dtype() else {
                return Err(Error::Model);
            };
            if shape.len() != 4 || shape[1] <= 0 || shape[3] <= 0 {
                return Err(Error::Model);
            }
            state.insert(
                input.name().into(),
                Tensor::<f32>::from_array(([0, shape[1], 0, shape[3]], vec![]))
                    .map_err(|_| Error::Inference)?
                    .into_dyn(),
            );
        }
        let output_memory = output_device(self.device)?;
        while tokens.len() < maximum {
            let use_cache = state
                .values()
                .any(|value| shape(value).is_ok_and(|shape| shape[0] != 0));
            let ids = if use_cache {
                &tokens[tokens.len() - 1..]
            } else {
                &tokens
            };
            let mut binding = self
                .decoder
                .create_binding()
                .map_err(|_| Error::Inference)?;
            binding
                .bind_input(
                    "input_ids",
                    &Tensor::from_array(([1, ids.len()], ids.to_vec()))
                        .map_err(|_| Error::Inference)?,
                )
                .map_err(|_| Error::Inference)?;
            binding
                .bind_input("encoder_hidden_states", encoding)
                .map_err(|_| Error::Inference)?;
            binding
                .bind_output_to_device("logits", &MemoryInfo::default())
                .map_err(|_| Error::Inference)?;
            if !state.is_empty() {
                binding
                    .bind_input(
                        "use_cache_branch",
                        &Tensor::from_array(([1], vec![use_cache]))
                            .map_err(|_| Error::Inference)?,
                    )
                    .map_err(|_| Error::Inference)?;
                for (key, value) in &state {
                    binding
                        .bind_input(key, value)
                        .map_err(|_| Error::Inference)?;
                    binding
                        .bind_output_to_device(
                            key.replacen("past_key_values.", "present.", 1),
                            &output_memory,
                        )
                        .map_err(|_| Error::Inference)?;
                }
            }
            let mut outputs = self
                .decoder
                .run_binding(&binding)
                .map_err(|_| Error::Inference)?;
            let (dimensions, logits) = outputs
                .get("logits")
                .ok_or(Error::Inference)?
                .try_extract_tensor::<f32>()
                .map_err(|_| Error::Inference)?;
            let vocab_size = *dimensions.last().ok_or(Error::Inference)? as usize;
            if vocab_size == 0 || logits.len() < vocab_size {
                return Err(Error::Inference);
            }
            let next = argmax(&logits[logits.len() - vocab_size..])? as i64;
            for (key, value) in &mut state {
                let next = outputs
                    .remove(key.replacen("past_key_values.", "present.", 1))
                    .ok_or(Error::Inference)?;
                if shape(&next)?.first().copied().ok_or(Error::Inference)? != 0 {
                    *value = next;
                }
            }
            tokens.push(next);
            if next == self.tokens.eos {
                break;
            }
        }
        Ok(tokens)
    }
}

pub struct Parakeet {
    frontend: Preprocessor,
    encoder: Session,
    decoder: Session,
    tokens: ParakeetTokens,
    maximum_tokens: usize,
}
impl Parakeet {
    pub fn load(directory: &Path, device: Device) -> Result<Self> {
        let config: serde_json::Value = serde_json::from_slice(
            &std::fs::read(directory.join("config.json")).map_err(|_| Error::Model)?,
        )
        .map_err(|_| Error::Model)?;
        if config["features_size"].as_u64().unwrap_or(80) != 128 {
            return Err(Error::Model);
        }
        Ok(Self {
            frontend: Preprocessor::new(device, Kind::Parakeet)?,
            encoder: builder(device, false)?
                .commit_from_file(directory.join("encoder-model.int8.onnx"))
                .map_err(|_| Error::Inference)?,
            decoder: builder(device, false)?
                .commit_from_file(directory.join("decoder_joint-model.int8.onnx"))
                .map_err(|_| Error::Inference)?,
            tokens: ParakeetTokens::load(directory)?,
            maximum_tokens: config["max_tokens_per_step"].as_u64().unwrap_or(10) as usize,
        })
    }
    pub fn recognize(&mut self, audio: &[f32]) -> Result<String> {
        let (features, lengths) = self.frontend.extract(audio)?;
        let outputs = self
            .encoder
            .run(ort::inputs!["audio_signal" => features, "length" => lengths])
            .map_err(|_| Error::Inference)?;
        let (dimensions, encodings) = outputs
            .get("outputs")
            .ok_or(Error::Inference)?
            .try_extract_tensor::<f32>()
            .map_err(|_| Error::Inference)?;
        if dimensions.len() != 3 || dimensions[0] != 1 || dimensions[1] <= 0 || dimensions[2] < 0 {
            return Err(Error::Inference);
        }
        let channels = dimensions[1] as usize;
        let frames = dimensions[2] as usize;
        let length = outputs
            .get("encoded_lengths")
            .ok_or(Error::Inference)?
            .try_extract_tensor::<i64>()
            .map_err(|_| Error::Inference)?
            .1;
        if length.len() != 1 || length[0] < 0 || length[0] as usize > frames {
            return Err(Error::Inference);
        }
        let mut states = Vec::new();
        for key in ["input_states_1", "input_states_2"] {
            let shape = static_shape(&self.decoder, key)?;
            if shape.len() != 3 || shape[0] <= 0 || shape[2] <= 0 {
                return Err(Error::Model);
            }
            states.push(
                Tensor::from_array((
                    [shape[0], 1, shape[2]],
                    vec![0.0_f32; (shape[0] * shape[2]) as usize],
                ))
                .map_err(|_| Error::Inference)?
                .into_dyn(),
            );
        }
        let mut tokens = Vec::new();
        let mut time = 0;
        let mut emitted = 0;
        while time < length[0] as usize {
            let encoding: Vec<_> = (0..channels)
                .map(|channel| encodings[channel * frames + time])
                .collect();
            let mut outputs = self.decoder.run(ort::inputs![
                "encoder_outputs" => Tensor::from_array(([1, channels, 1], encoding)).map_err(|_| Error::Inference)?,
                "targets" => Tensor::from_array(([1, 1], vec![*tokens.last().unwrap_or(&self.tokens.blank) as i32])).map_err(|_| Error::Inference)?,
                "target_length" => Tensor::from_array(([1], vec![1_i32])).map_err(|_| Error::Inference)?,
                "input_states_1" => &states[0], "input_states_2" => &states[1],
            ]).map_err(|_| Error::Inference)?;
            let logits = outputs
                .get("outputs")
                .ok_or(Error::Inference)?
                .try_extract_tensor::<f32>()
                .map_err(|_| Error::Inference)?
                .1;
            let vocab = self.tokens.size();
            if logits.len() <= vocab {
                return Err(Error::Inference);
            }
            let token = argmax(&logits[..vocab])? as i64;
            let step = argmax(&logits[vocab..])?;
            if token != self.tokens.blank {
                states[0] = outputs.remove("output_states_1").ok_or(Error::Inference)?;
                states[1] = outputs.remove("output_states_2").ok_or(Error::Inference)?;
                tokens.push(token);
                emitted += 1;
            }
            if step > 0 {
                time += step;
                emitted = 0;
            } else if token == self.tokens.blank || emitted == self.maximum_tokens {
                time += 1;
                emitted = 0;
            }
        }
        self.tokens.decode(&tokens)
    }
}
