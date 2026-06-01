use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::State;
use tokio_util::sync::CancellationToken;

use crate::commands::{ChatMessage, OllamaError, OllamaErrorKind, StreamChunk};
use crate::config::{AppConfig, OpenRouterSection};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct OpenRouterArchitecture {
    #[serde(default)]
    pub input_modalities: Vec<String>,
    #[serde(default)]
    pub output_modalities: Vec<String>,
    #[serde(default)]
    pub modality: Option<String>,
    #[serde(default)]
    pub tokenizer: Option<String>,
    #[serde(default)]
    pub instruct_type: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct OpenRouterPricing {
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub completion: String,
    #[serde(default)]
    pub request: String,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub web_search: String,
    #[serde(default)]
    pub internal_reasoning: String,
    #[serde(default)]
    pub input_cache_read: String,
    #[serde(default)]
    pub input_cache_write: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct OpenRouterTopProvider {
    #[serde(default)]
    pub context_length: Option<u64>,
    #[serde(default)]
    pub max_completion_tokens: Option<u64>,
    #[serde(default)]
    pub is_moderated: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct OpenRouterModel {
    pub id: String,
    #[serde(default)]
    pub canonical_slug: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub context_length: Option<u64>,
    #[serde(default)]
    pub architecture: OpenRouterArchitecture,
    #[serde(default)]
    pub pricing: OpenRouterPricing,
    #[serde(default)]
    pub supported_parameters: Vec<String>,
    #[serde(default)]
    pub supported_voices: Option<Vec<String>>,
    #[serde(default)]
    pub top_provider: Option<OpenRouterTopProvider>,
    #[serde(default)]
    pub created: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<OpenRouterModel>,
}

#[derive(Clone, Debug, Deserialize)]
struct EmbeddingItem {
    #[serde(default)]
    index: Option<usize>,
    embedding: Vec<f32>,
}

#[derive(Clone, Debug, Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingItem>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OpenRouterSelectedModels {
    pub general_model: String,
    pub chat_model: String,
    pub vision_model: String,
    pub reasoning_model: String,
    pub embedding_model: String,
    pub stt_model: String,
    pub tts_model: String,
    pub use_general_model: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct OpenRouterModelCatalog {
    pub configured: bool,
    pub base_url: String,
    pub selected: OpenRouterSelectedModels,
    pub models: Vec<OpenRouterModel>,
}

pub struct OpenRouterChatParams {
    pub base_url: String,
    pub api_key: String,
    pub app_title: String,
    pub site_url: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
}

fn openrouter_error(message: impl Into<String>) -> OllamaError {
    OllamaError {
        kind: OllamaErrorKind::Other,
        message: message.into(),
    }
}

fn api_base(config: &OpenRouterSection) -> String {
    config.base_url.trim().trim_end_matches('/').to_string()
}

pub fn selected_chat_model(config: &OpenRouterSection, has_images: bool) -> String {
    if config.use_general_model && !config.general_model.trim().is_empty() {
        return config.general_model.trim().to_string();
    }
    if has_images && !config.vision_model.trim().is_empty() {
        return config.vision_model.trim().to_string();
    }
    if !config.chat_model.trim().is_empty() {
        return config.chat_model.trim().to_string();
    }
    config.general_model.trim().to_string()
}

fn selected_models(config: &OpenRouterSection) -> OpenRouterSelectedModels {
    OpenRouterSelectedModels {
        general_model: config.general_model.clone(),
        chat_model: config.chat_model.clone(),
        vision_model: config.vision_model.clone(),
        reasoning_model: config.reasoning_model.clone(),
        embedding_model: config.embedding_model.clone(),
        stt_model: config.stt_model.clone(),
        tts_model: config.tts_model.clone(),
        use_general_model: config.use_general_model,
    }
}

fn apply_headers<'a>(
    request: reqwest::RequestBuilder,
    api_key: &'a str,
    site_url: &'a str,
    app_title: &'a str,
) -> reqwest::RequestBuilder {
    let request = request
        .bearer_auth(api_key)
        .header("Content-Type", "application/json");
    let request = if site_url.trim().is_empty() {
        request
    } else {
        request.header("HTTP-Referer", site_url.trim())
    };
    if app_title.trim().is_empty() {
        request
    } else {
        request.header("X-Title", app_title.trim())
    }
}

fn message_to_openrouter_value(message: &ChatMessage) -> serde_json::Value {
    let Some(images) = message.images.as_ref().filter(|images| !images.is_empty()) else {
        return json!({
            "role": message.role,
            "content": message.content,
        });
    };

    let mut content = vec![json!({
        "type": "text",
        "text": message.content,
    })];
    for image in images {
        content.push(json!({
            "type": "image_url",
            "image_url": {
                "url": format!("data:image/jpeg;base64,{image}")
            }
        }));
    }

    json!({
        "role": message.role,
        "content": content,
    })
}

fn parse_stream_payload(data: &str) -> Vec<StreamChunk> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
        return Vec::new();
    };
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("OpenRouter returned an error.");
        return vec![StreamChunk::Error(openrouter_error(format!(
            "OpenRouter request failed\n{message}"
        )))];
    }

    let Some(delta) = value
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta"))
    else {
        return Vec::new();
    };

    let mut chunks = Vec::new();
    if let Some(reasoning) = delta
        .get("reasoning")
        .or_else(|| delta.get("reasoning_content"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        chunks.push(StreamChunk::ThinkingToken(reasoning.to_string()));
    }
    if let Some(content) = delta
        .get("content")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        chunks.push(StreamChunk::Token(content.to_string()));
    }
    chunks
}

pub async fn fetch_models(
    client: &reqwest::Client,
    config: &OpenRouterSection,
) -> Result<Vec<OpenRouterModel>, String> {
    let api_key = config.api_key.trim();
    if api_key.is_empty() {
        return Err("Add an OpenRouter API key in Settings first.".to_string());
    }

    let url = format!("{}/models?output_modalities=all", api_base(config));
    let response = apply_headers(
        client.get(&url).timeout(std::time::Duration::from_secs(20)),
        api_key,
        &config.site_url,
        &config.app_title,
    )
    .send()
    .await
    .map_err(|e| format!("failed to fetch OpenRouter models: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "OpenRouter models returned HTTP {}: {}",
            response.status().as_u16(),
            response.text().await.unwrap_or_default()
        ));
    }

    response
        .json::<ModelsResponse>()
        .await
        .map(|payload| payload.data)
        .map_err(|e| format!("failed to decode OpenRouter models: {e}"))
}

pub async fn embed_texts(
    client: &reqwest::Client,
    config: &OpenRouterSection,
    inputs: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    let api_key = config.api_key.trim();
    if api_key.is_empty() {
        return Err("Add an OpenRouter API key in Settings first.".to_string());
    }
    if inputs.is_empty() {
        return Ok(Vec::new());
    }

    let model = config.embedding_model.trim();
    if model.is_empty() {
        return Err("Choose an OpenRouter embedding model in Settings.".to_string());
    }

    let url = format!("{}/embeddings", api_base(config));
    let response = apply_headers(
        client
            .post(&url)
            .timeout(std::time::Duration::from_secs(60))
            .json(&json!({
                "model": model,
                "input": inputs,
                "encoding_format": "float",
            })),
        api_key,
        &config.site_url,
        &config.app_title,
    )
    .send()
    .await
    .map_err(|e| format!("failed to create OpenRouter embeddings: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "OpenRouter embeddings returned HTTP {}: {}",
            response.status().as_u16(),
            response.text().await.unwrap_or_default()
        ));
    }

    let mut data = response
        .json::<EmbeddingsResponse>()
        .await
        .map_err(|e| format!("failed to decode OpenRouter embeddings: {e}"))?
        .data;
    data.sort_by_key(|item| item.index.unwrap_or(usize::MAX));
    let vectors = data
        .into_iter()
        .map(|item| item.embedding)
        .collect::<Vec<_>>();
    if vectors.len() != inputs.len() {
        return Err(format!(
            "OpenRouter returned {} embeddings for {} inputs.",
            vectors.len(),
            inputs.len()
        ));
    }
    Ok(vectors)
}

pub async fn stream_openrouter_chat(
    params: OpenRouterChatParams,
    client: &reqwest::Client,
    cancel_token: CancellationToken,
    on_chunk: impl Fn(StreamChunk),
) -> String {
    let OpenRouterChatParams {
        base_url,
        api_key,
        app_title,
        site_url,
        model,
        messages,
    } = params;
    if api_key.trim().is_empty() {
        on_chunk(StreamChunk::Error(openrouter_error(
            "OpenRouter is selected\nAdd an OpenRouter API key in Settings.",
        )));
        return String::new();
    }

    let url = format!("{}/chat/completions", base_url.trim().trim_end_matches('/'));
    let request_payload = json!({
        "model": model,
        "stream": true,
        "messages": messages.iter().map(message_to_openrouter_value).collect::<Vec<_>>(),
        "temperature": 1.0,
        "top_p": 0.95,
    });

    let mut accumulated = String::new();
    let res = apply_headers(
        client.post(&url).json(&request_payload),
        api_key.trim(),
        &site_url,
        &app_title,
    )
    .send()
    .await;

    match res {
        Ok(response) => {
            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body = response.text().await.unwrap_or_default();
                on_chunk(StreamChunk::Error(openrouter_error(format!(
                    "OpenRouter request failed\nHTTP {status}: {body}"
                ))));
                return accumulated;
            }

            let mut stream = response.bytes_stream();
            let mut buffer: Vec<u8> = Vec::new();

            loop {
                tokio::select! {
                    biased;
                    _ = cancel_token.cancelled() => {
                        drop(stream);
                        on_chunk(StreamChunk::Cancelled);
                        return accumulated;
                    }
                    chunk_opt = stream.next() => {
                        match chunk_opt {
                            Some(Ok(bytes)) => {
                                buffer.extend_from_slice(&bytes);
                                while let Some(idx) = buffer.iter().position(|&b| b == b'\n') {
                                    let line_bytes = buffer.drain(..=idx).collect::<Vec<u8>>();
                                    let Ok(line_text) = String::from_utf8(line_bytes) else {
                                        continue;
                                    };
                                    let trimmed = line_text.trim();
                                    if trimmed.is_empty() || trimmed.starts_with(':') {
                                        continue;
                                    }
                                    let data = trimmed.strip_prefix("data:").map(str::trim).unwrap_or(trimmed);
                                    if data == "[DONE]" {
                                        on_chunk(StreamChunk::Done);
                                        return accumulated;
                                    }
                                    for chunk in parse_stream_payload(data) {
                                        if let StreamChunk::Token(token) = &chunk {
                                            accumulated.push_str(token);
                                        }
                                        on_chunk(chunk);
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                on_chunk(StreamChunk::Error(openrouter_error(format!(
                                    "OpenRouter stream failed\n{e}"
                                ))));
                                return accumulated;
                            }
                            None => {
                                on_chunk(StreamChunk::Done);
                                return accumulated;
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            on_chunk(StreamChunk::Error(openrouter_error(format!(
                "OpenRouter is unreachable\n{e}"
            ))));
        }
    }

    accumulated
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub async fn openrouter_list_models(
    client: State<'_, reqwest::Client>,
    config: State<'_, parking_lot::RwLock<AppConfig>>,
) -> Result<OpenRouterModelCatalog, String> {
    let config = config.read().openrouter.clone();
    let models = fetch_models(&client, &config).await?;
    Ok(OpenRouterModelCatalog {
        configured: !config.api_key.trim().is_empty(),
        base_url: config.base_url.clone(),
        selected: selected_models(&config),
        models,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> OpenRouterSection {
        OpenRouterSection {
            use_general_model: false,
            general_model: "general/model".to_string(),
            chat_model: "chat/model".to_string(),
            vision_model: "vision/model".to_string(),
            ..OpenRouterSection::default()
        }
    }

    #[test]
    fn selected_chat_model_uses_vision_when_images_present() {
        assert_eq!(selected_chat_model(&config(), true), "vision/model");
    }

    #[test]
    fn selected_chat_model_uses_chat_for_text_only() {
        assert_eq!(selected_chat_model(&config(), false), "chat/model");
    }

    #[test]
    fn selected_chat_model_prefers_general_when_enabled() {
        let mut config = config();
        config.use_general_model = true;
        assert_eq!(selected_chat_model(&config, true), "general/model");
    }

    #[test]
    fn message_to_openrouter_value_converts_images_to_data_urls() {
        let value = message_to_openrouter_value(&ChatMessage {
            role: "user".to_string(),
            content: "What is this?".to_string(),
            images: Some(vec!["abc".to_string()]),
        });
        assert_eq!(value["content"][0]["type"], "text");
        assert_eq!(
            value["content"][1]["image_url"]["url"],
            "data:image/jpeg;base64,abc"
        );
    }
}
