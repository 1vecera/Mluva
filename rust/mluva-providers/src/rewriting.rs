//! One frozen rewrite route and model/effort policy shared by Live and conversations.
use crate::{
    ProviderError, Result,
    codex::{CodexAppServerClient, CodexOptions},
    compatible::{CompatibleClient, RewriteOptions},
    models::{Model, select_codex_model},
};
use mluva_core::{config::AppConfig, screenshots::ImageInput};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewriteSelection {
    pub model: String,
    pub effort: Option<String>,
    pub service_tier: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewriteResult {
    pub text: String,
    pub model: String,
}

/// Preserve the released controller's distinction between an actionable speed
/// failure and other failures, without inspecting or exposing provider text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RewriteError {
    #[error("Fast mode is unavailable for this model. Turn it off or choose another model.")]
    UnsupportedRewriteSpeed,
    #[error("Thinking level is unavailable for this model. Choose Auto or another level.")]
    UnsupportedThinkingLevel,
    #[error(transparent)]
    Provider(#[from] ProviderError),
}

pub fn selection(
    config: &AppConfig,
    models: &[Model],
) -> std::result::Result<RewriteSelection, RewriteError> {
    if config.rewrite_provider == "none" {
        return Err(
            ProviderError::message("Choose a rewriting provider in Settings first.").into(),
        );
    }
    if config.rewrite_provider == "litellm" {
        let model = config
            .litellm_model
            .as_ref()
            .filter(|model| !model.is_empty())
            .ok_or_else(|| {
                ProviderError::message("Choose a LiteLLM model in Settings → Providers.")
            })?;
        return Ok(RewriteSelection {
            model: model.clone(),
            effort: config.litellm_reasoning_effort.clone(),
            service_tier: None,
        });
    }
    let requested = config
        .rewrite_model
        .as_deref()
        .filter(|model| !model.is_empty())
        .or(config.codex_model.as_deref());
    let selected = select_codex_model(models, requested)?;
    if config.rewrite_fast_mode && selected.fast_tier.is_none() {
        return Err(RewriteError::UnsupportedRewriteSpeed);
    }
    if config
        .rewrite_reasoning_effort
        .as_ref()
        .is_some_and(|effort| !selected.reasoning_efforts.contains(effort))
    {
        return Err(RewriteError::UnsupportedThinkingLevel);
    }
    Ok(RewriteSelection {
        model: selected.identifier.clone(),
        effort: config
            .rewrite_reasoning_effort
            .clone()
            .or_else(|| selected.rewrite_effort.clone()),
        service_tier: if config.rewrite_fast_mode {
            selected.fast_tier.clone()
        } else {
            Some("default".into())
        },
    })
}

enum Route {
    None,
    Compatible(CompatibleClient),
    Codex(CodexAppServerClient),
}
pub struct RewriteClient {
    config: AppConfig,
    route: Route,
}
impl RewriteClient {
    pub fn new(
        config: &AppConfig,
        request_timeout: Option<Duration>,
        turn_timeout: Option<Duration>,
    ) -> Result<Self> {
        let route = match config.rewrite_provider.as_str() {
            "none" => Route::None,
            "litellm" => Route::Compatible(CompatibleClient::new(
                &config.litellm_base_url,
                config.litellm_api_key_env.clone(),
                config.litellm_model.clone(),
                turn_timeout
                    .or(request_timeout)
                    .unwrap_or(Duration::from_secs(60)),
            )?),
            _ => Route::Codex(CodexAppServerClient::new(CodexOptions {
                request_timeout: request_timeout.unwrap_or(Duration::from_secs(30)),
                turn_timeout: turn_timeout.unwrap_or(Duration::from_secs(180)),
                ..Default::default()
            })),
        };
        Ok(Self {
            config: config.clone(),
            route,
        })
    }
    pub async fn list_models(&self) -> Result<Vec<Model>> {
        match &self.route {
            Route::None => Ok(vec![]),
            Route::Compatible(client) => client.list_models(None).await,
            Route::Codex(client) => client.list_models().await,
        }
    }
    /// Capture freezes a model before recognition. Document rewrite speed and
    /// thinking settings belong to `transform`, not to these capture turns.
    pub async fn resolve_model(&self, requested: Option<&str>) -> Result<String> {
        match &self.route {
            Route::None => Err(ProviderError::message(
                "Choose a rewriting provider in Settings first.",
            )),
            Route::Compatible(client) => Ok(client.resolve_model(requested)?.into()),
            Route::Codex(client) => client.resolve_model(requested).await,
        }
    }

    /// Segment cleanup owns independent Codex attempts with the parent's frozen
    /// command and deadlines, never its active connection.
    pub fn spawn_codex(&self) -> Option<CodexAppServerClient> {
        match &self.route {
            Route::Codex(client) => Some(client.spawn()),
            _ => None,
        }
    }

    pub async fn transform_capture(
        &self,
        prompt: &str,
        cwd: &Path,
        model: &str,
        images: &[ImageInput],
    ) -> Result<String> {
        self.transform_bounded(prompt, cwd, model, images, 8_000)
            .await
    }

    /// Titles use the frozen capture/default model, without rewrite speed or
    /// thinking overrides, and reject verbose output at the transport boundary.
    pub async fn transform_title(&self, prompt: &str, cwd: &Path, model: &str) -> Result<String> {
        self.transform_bounded(prompt, cwd, model, &[], 128).await
    }

    async fn transform_bounded(
        &self,
        prompt: &str,
        cwd: &Path,
        model: &str,
        images: &[ImageInput],
        maximum: usize,
    ) -> Result<String> {
        let options = RewriteOptions {
            model: Some(model),
            max_output_characters: maximum,
            images,
            ..Default::default()
        };
        match &self.route {
            Route::None => Err(ProviderError::message(
                "Choose a rewriting provider in Settings first.",
            )),
            Route::Compatible(client) => client.transform(prompt, cwd, options, None).await,
            Route::Codex(client) => client.transform(prompt, cwd, options, None).await,
        }
    }
    pub async fn transform(
        &self,
        prompt: &str,
        cwd: &Path,
        images: &[ImageInput],
        on_delta: Option<&mut (dyn FnMut(&str) + Send)>,
    ) -> std::result::Result<RewriteResult, RewriteError> {
        let models = if let Route::Codex(client) = &self.route {
            client.list_models().await?
        } else {
            vec![]
        };
        let selected = selection(&self.config, &models)?;
        let options = RewriteOptions {
            model: Some(&selected.model),
            max_output_characters: mluva_core::conversation::MAX_REWRITE_CHARACTERS,
            effort: selected.effort.as_deref(),
            service_tier: selected.service_tier.as_deref(),
            images,
        };
        let text = match &self.route {
            Route::None => unreachable!("disabled route fails before dispatch"),
            Route::Compatible(client) => client.transform(prompt, cwd, options, on_delta).await?,
            Route::Codex(client) => client.transform(prompt, cwd, options, on_delta).await?,
        };
        Ok(RewriteResult {
            text,
            model: selected.model,
        })
    }
    pub async fn close(&self) {
        match &self.route {
            Route::None => {}
            Route::Compatible(client) => client.close(),
            Route::Codex(client) => client.close().await,
        }
    }
    pub fn cancel(&self) {
        match &self.route {
            Route::None => {}
            Route::Compatible(client) => client.cancel(),
            Route::Codex(client) => client.cancel(),
        }
    }
}
