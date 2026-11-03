//! Content-free provider discovery; edited routes never share clients or results.
use crate::{Result, compatible::CompatibleClient, credentials::CloudProvider, models::Model};
use mluva_core::text;
use std::time::Duration;

/// What one audio hour costs on a cloud provider, per mode, before taxes. Checked 30 October 2026
/// against elevenlabs.io/pricing/api, microsoft.ai/models/mai-transcribe-2 and
/// docs.x.ai/developers/pricing; vendors change prices.
pub struct Pricing {
    pub realtime_per_hour: f64,
    pub batch_per_hour: f64,
    pub batch_note: Option<&'static str>,
    /// Shown with the Realtime choice, for a provider whose Batch mode is the better one.
    pub realtime_note: Option<&'static str>,
}
impl Pricing {
    pub fn realtime_label(&self) -> String {
        format!("Realtime · ${:.2} / audio hour", self.realtime_per_hour)
    }
    pub fn batch_label(&self) -> String {
        format!("Batch · ${:.2} / audio hour", self.batch_per_hour)
    }
}

pub struct Provider {
    pub id: &'static str,
    /// Short name on the provider choice.
    pub label: &'static str,
    pub model_field: &'static str,
    pub default_label: Option<&'static str>,
    pub description: &'static str,
    /// The provider listed first and labelled Suggested.
    pub suggested: bool,
    /// Present for cloud providers, which offer a realtime and a batch mode.
    pub pricing: Option<Pricing>,
}
impl Provider {
    pub fn cloud(&self) -> Option<CloudProvider> {
        CloudProvider::from_id(self.id)
    }
}

/// In display order; the suggested provider is first.
pub const SPEECH_PROVIDERS: [Provider; 6] = [
    Provider {
        id: "microsoft",
        label: "Microsoft MAI",
        model_field: "transcription_model",
        default_label: None,
        description: "Suggested · Microsoft MAI-Transcribe-2 through Azure Speech. Halves the live Czech error of Scribe v2 in our benchmark, with fast live text. Needs an Azure Speech key and its region. A Microsoft preview without an SLA; the free tier allows one live stream at a time.",
        suggested: true,
        pricing: Some(Pricing {
            realtime_per_hour: 0.54,
            batch_per_hour: 0.10,
            batch_note: Some("Batch is an introductory price until 31 December 2026."),
            realtime_note: None,
        }),
    },
    Provider {
        id: "elevenlabs",
        label: "ElevenLabs",
        model_field: "transcription_model",
        default_label: None,
        description: "Scribe v2 with live recognition and an explicit batch fallback. Best on technical Czech in our benchmark. Pricing may change. I’m not affiliated with ElevenLabs in any way—just a happy user.",
        suggested: false,
        pricing: Some(Pricing {
            realtime_per_hour: 0.39,
            batch_per_hour: 0.22,
            batch_note: None,
            realtime_note: None,
        }),
    },
    Provider {
        id: "xai",
        label: "xAI Grok",
        model_field: "transcription_model",
        default_label: None,
        description: "xAI Grok Voice Transcribe 2.0. The cheapest realtime in our benchmark, with weaker Czech. Needs an xAI API key.",
        suggested: false,
        pricing: Some(Pricing {
            realtime_per_hour: 0.20,
            batch_per_hour: 0.10,
            batch_note: None,
            realtime_note: None,
        }),
    },
    Provider {
        id: "mistral",
        label: "Mistral",
        model_field: "transcription_model",
        default_label: None,
        description: "Mistral Voxtral 3, EU-hosted: Voxtral Small for the most accurate batch English in our benchmark, Voxtral Mini Realtime for live text. Needs a Mistral API key. The entry pay-as-you-go tier answers bursts of requests with rate-limit errors.",
        suggested: false,
        pricing: Some(Pricing {
            realtime_per_hour: 0.48,
            batch_per_hour: 0.24,
            batch_note: Some(
                "Batch uses Voxtral Small; Voxtral Mini batch ($0.21) is not offered.",
            ),
            realtime_note: Some(
                "Batch is the recommended mode for Mistral: Voxtral Small is the most accurate batch route in English. Its realtime finishes more slowly after Stop than Microsoft MAI or Scribe.",
            ),
        }),
    },
    Provider {
        id: "local",
        label: "Local model",
        model_field: "local_model",
        default_label: None,
        description: "Mluva downloads and owns its models. Audio stays on this device. Weights load only while transcribing.",
        suggested: false,
        pricing: None,
    },
    Provider {
        id: "litellm",
        label: "Custom",
        model_field: "transcription_remote_model",
        default_label: None,
        description: "Recognizes audio through your LiteLLM or OpenAI-compatible server.",
        suggested: false,
        pricing: None,
    },
];

/// The short benchmark behind "Why these providers", for the Providers page. Word error rates are
/// percentages, lower is better, over 62 public clips: Google FLEURS Czech and English plus
/// 12 synthetic Czech/English technical sentences.
pub const BENCHMARK_HEADER: [&str; 7] = [
    "Route",
    "Czech",
    "English",
    "Czech technical",
    "First text",
    "After stop",
    "$ / audio h",
];
pub const BENCHMARK_ROWS: [[&str; 7]; 8] = [
    [
        "Scribe v2 realtime",
        "10.8",
        "4.1",
        "14.8",
        "2.2 s",
        "0.22 s",
        "0.39",
    ],
    [
        "MAI realtime",
        "5.3",
        "4.4",
        "22.2",
        "0.4–1.3 s",
        "0.16 s",
        "0.54",
    ],
    [
        "Grok realtime",
        "12.2",
        "4.6",
        "22.2",
        "1.1 s",
        "0.30 s",
        "0.20",
    ],
    [
        "Scribe v2 batch",
        "4.0",
        "5.0",
        "11.1",
        "–",
        "1–4 s",
        "0.22",
    ],
    ["MAI batch", "4.2", "3.4", "22.2", "–", "0.6 s", "0.10"],
    ["Grok batch", "9.9", "4.6", "23.5", "–", "0.7–2.5 s", "0.10"],
    [
        "Mistral realtime",
        "10.1",
        "5.2",
        "23.5",
        "1.2–2.0 s",
        "0.66–0.92 s",
        "0.48",
    ],
    [
        "Mistral batch",
        "3.8",
        "2.3",
        "17.3",
        "–",
        "0.8–1.0 s",
        "0.24",
    ],
];
/// One line per provider on why it is offered.
pub const WHY_LINES: [&str; 4] = [
    "Microsoft MAI-Transcribe-2 halves Scribe v2’s live Czech error (5.3% against 10.8%), finishes fastest after Stop (0.16 s) and ties for the lowest batch price. It is weaker on Czech technical terms (22.2%).",
    "ElevenLabs Scribe v2 is best on Czech technical terms (14.8% live, 11.1% batch) and an established service. Its live words arrive later (2.2 s).",
    "xAI Grok Voice Transcribe 2.0 has the lowest realtime price ($0.20 per audio hour) and English on a par with the others, with weaker Czech (12.2% live).",
    "Mistral Voxtral Small is the most accurate batch route in English (2.3% against Scribe’s 5.0%), on a par in Czech and EU-hosted. Its realtime finishes more slowly after Stop (0.66–0.92 s) and is weaker in Czech than MAI, so batch is the recommended mode.",
];
pub const WHY_NOTE: &str = "English is a tie for the realtime routes; Voxtral Small batch beats Scribe batch (95% interval of the difference −5.0 to −0.2 points). Not offered: Alibaba Fun-ASR, because its international sign-up is complex. Prices checked 30 October 2026. Microsoft’s MAI models are a preview without an SLA.";

pub fn connection_hint(provider: &str, key_env: &str) -> &'static str {
    match provider {
        "local" => "Models are managed by Mluva. No other dictation app is required.",
        _ => {
            let cloud = CloudProvider::from_id(provider);
            let names = match cloud {
                Some(cloud) => cloud.key_variables(),
                None => std::slice::from_ref(&key_env),
            };
            if names
                .iter()
                .any(|name| std::env::var(name).is_ok_and(|value| !text::trim(&value).is_empty()))
            {
                "Key found in Mluva’s environment. Account access is checked when you use the provider."
            } else if cloud.is_some() {
                "Add a key in Settings → Providers or Welcome setup, or set the provider’s key variable in your launch environment and restart Mluva."
            } else {
                "No key found. A local server may not need one. Otherwise set the named variable in Mluva’s launch environment and restart. Connection details contain the variable name only."
            }
        }
    }
}
#[derive(Clone)]
pub struct CatalogRequest {
    pub base_url: String,
    pub key_env: String,
}
pub struct CatalogClient(CompatibleClient);
impl CatalogRequest {
    pub fn client(&self) -> Result<CatalogClient> {
        Ok(CatalogClient(CompatibleClient::new(
            &self.base_url,
            self.key_env.clone(),
            None,
            Duration::from_secs(5),
        )?))
    }
    pub fn message(&self, models: Option<&[Model]>) -> String {
        match models {
            None => "Could not load models. Check the endpoint (including /v1 if needed) and key variable. You can still enter a deployment ID; your choice is kept.".into(),
            Some([]) => "No matching models listed. Enter a model ID or check the server’s deployment configuration.".into(),
            Some(models) => format!("{} model(s) listed. Listing a model does not verify that your account can run it.", models.len()),
        }
    }
}
impl CatalogClient {
    pub async fn read(&self) -> Result<Vec<Model>> {
        self.0.list_models().await
    }
    pub fn cancel(&self) {
        self.0.cancel();
    }
    pub async fn close(&self) {
        self.0.close();
    }
}
