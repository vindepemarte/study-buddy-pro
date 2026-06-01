//! Typed shape of the Thuki configuration file.
//!
//! Serde derives the TOML mapping automatically. Each section struct carries
//! `#[serde(default)]` so a partial file (missing whole sections or fields)
//! deserializes cleanly: missing fields inherit the compiled defaults via the
//! manual `Default` impls below.
//!
//! Section structs use manual `Default` impls (NOT `#[derive(Default)]`)
//! because deriving Default would fill fields with zero/empty values
//! (`String::default() == ""`, `u64::default() == 0`), which is the opposite
//! of what the user expects. `AppConfig` itself uses `#[derive(Default)]`
//! because it delegates entirely to each section's own `Default` impl.

use serde::{Deserialize, Serialize};

use super::defaults::{
    DEFAULT_DEBUG_TRACE_ENABLED, DEFAULT_INFERENCE_PROVIDER, DEFAULT_JUDGE_TIMEOUT_S,
    DEFAULT_KEEP_WARM_INACTIVITY_MINUTES, DEFAULT_MAX_CHAT_HEIGHT, DEFAULT_MAX_IMAGES,
    DEFAULT_MAX_ITERATIONS, DEFAULT_NUM_CTX, DEFAULT_OLLAMA_URL, DEFAULT_OPENROUTER_APP_TITLE,
    DEFAULT_OPENROUTER_BASE_URL, DEFAULT_OPENROUTER_CHAT_MODEL, DEFAULT_OPENROUTER_EMBEDDING_MODEL,
    DEFAULT_OPENROUTER_GENERAL_MODEL, DEFAULT_OPENROUTER_REASONING_MODEL,
    DEFAULT_OPENROUTER_SITE_URL, DEFAULT_OPENROUTER_STT_MODEL, DEFAULT_OPENROUTER_TTS_MODEL,
    DEFAULT_OPENROUTER_USE_GENERAL_MODEL, DEFAULT_OPENROUTER_VISION_MODEL, DEFAULT_OVERLAY_WIDTH,
    DEFAULT_PIPELINE_WALL_CLOCK_BUDGET_S, DEFAULT_QUOTE_MAX_CONTEXT_LENGTH,
    DEFAULT_QUOTE_MAX_DISPLAY_CHARS, DEFAULT_QUOTE_MAX_DISPLAY_LINES,
    DEFAULT_READER_BATCH_TIMEOUT_S, DEFAULT_READER_PER_URL_TIMEOUT_S, DEFAULT_READER_URL,
    DEFAULT_ROUTER_TIMEOUT_S, DEFAULT_SEARCH_TIMEOUT_S, DEFAULT_SEARXNG_MAX_RESULTS,
    DEFAULT_SEARXNG_URL, DEFAULT_SYSTEM_CUSTOMIZED, DEFAULT_SYSTEM_PROMPT_BASE,
    DEFAULT_TEXT_BASE_PX, DEFAULT_TEXT_FONT_WEIGHT, DEFAULT_TEXT_LETTER_SPACING_PX,
    DEFAULT_TEXT_LINE_HEIGHT, DEFAULT_TOP_K_URLS, DEFAULT_UPDATER_AUTO_CHECK,
    DEFAULT_UPDATER_CHECK_INTERVAL_HOURS, DEFAULT_UPDATER_MANIFEST_URL,
    DEFAULT_VOICE_AUTO_SPEAK_STUDY, DEFAULT_VOICE_BASE_URL, DEFAULT_VOICE_ENABLED,
    DEFAULT_VOICE_LANG, DEFAULT_VOICE_MAX_CHUNK_LENGTH, DEFAULT_VOICE_NAME,
    DEFAULT_VOICE_OPENROUTER_VOICE, DEFAULT_VOICE_PROVIDER, DEFAULT_VOICE_SPEED,
    DEFAULT_VOICE_STEPS,
};

/// Static, user-tunable inference daemon configuration.
///
/// The active model selection is NOT stored here. Active-model state is
/// runtime UI state owned by [`crate::models::ActiveModelState`] and
/// persisted in the SQLite `app_config` table under
/// [`crate::models::ACTIVE_MODEL_KEY`]. Storing a model slug in TOML would
/// duplicate ground truth from Ollama's `/api/tags` and create a staleness
/// trap: the file would happily reference a model the user has since
/// removed. This section keeps only the truly static knob, the Ollama
/// endpoint URL.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct InferenceSection {
    /// Active inference provider: "ollama" or "openrouter".
    pub provider: String,
    /// HTTP base URL of the local Ollama instance.
    pub ollama_url: String,
    /// Minutes of inactivity before Thuki tells Ollama to release the model.
    /// 0 means do not manage (Ollama's 5-minute default applies).
    /// -1 means keep indefinitely. Valid range: -1 or 0..=1440.
    pub keep_warm_inactivity_minutes: i32,
    /// Context window size (in tokens) sent to Ollama with every request.
    /// Warmup and chat use the same value so Ollama reuses the same runner
    /// instance and its cached KV prefix for the system prompt. Raise to fit
    /// longer conversations in a single context; lower to use less VRAM.
    /// Valid range: 2048..=1048576.
    pub num_ctx: u32,
}

impl Default for InferenceSection {
    fn default() -> Self {
        Self {
            provider: DEFAULT_INFERENCE_PROVIDER.to_string(),
            ollama_url: DEFAULT_OLLAMA_URL.to_string(),
            keep_warm_inactivity_minutes: DEFAULT_KEEP_WARM_INACTIVITY_MINUTES,
            num_ctx: DEFAULT_NUM_CTX,
        }
    }
}

/// OpenRouter API configuration. The API key is stored locally in the user's
/// app config file and never leaves the machine except as the Authorization
/// header for OpenRouter requests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct OpenRouterSection {
    pub api_key: String,
    pub base_url: String,
    pub use_general_model: bool,
    pub general_model: String,
    pub chat_model: String,
    pub vision_model: String,
    pub reasoning_model: String,
    pub embedding_model: String,
    pub stt_model: String,
    pub tts_model: String,
    pub app_title: String,
    pub site_url: String,
}

impl Default for OpenRouterSection {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: DEFAULT_OPENROUTER_BASE_URL.to_string(),
            use_general_model: DEFAULT_OPENROUTER_USE_GENERAL_MODEL,
            general_model: DEFAULT_OPENROUTER_GENERAL_MODEL.to_string(),
            chat_model: DEFAULT_OPENROUTER_CHAT_MODEL.to_string(),
            vision_model: DEFAULT_OPENROUTER_VISION_MODEL.to_string(),
            reasoning_model: DEFAULT_OPENROUTER_REASONING_MODEL.to_string(),
            embedding_model: DEFAULT_OPENROUTER_EMBEDDING_MODEL.to_string(),
            stt_model: DEFAULT_OPENROUTER_STT_MODEL.to_string(),
            tts_model: DEFAULT_OPENROUTER_TTS_MODEL.to_string(),
            app_title: DEFAULT_OPENROUTER_APP_TITLE.to_string(),
            site_url: DEFAULT_OPENROUTER_SITE_URL.to_string(),
        }
    }
}

/// Prompt configuration. `system` holds the user-editable persona prompt; on
/// first run it is seeded with the full built-in body so the file is the
/// single source of truth. The slash-command appendix is composed at load
/// time into `resolved_system` and is never written back to the file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PromptSection {
    /// User-editable persona prompt. Seeded with the built-in body and
    /// freely editable thereafter. If the user clears it (with
    /// `system_customized` set), no persona is sent (only the
    /// slash-command appendix).
    pub system: String,
    /// Set to `true` the first time the user explicitly saves the system
    /// prompt via Settings. Guards upgrade migration: configs from before
    /// the Settings UI was added have `system = ""` because that was the
    /// old compiled default, not an intentional clear. The loader resets
    /// an empty `system` to the built-in default when this flag is
    /// `false`, preserving the intentional-clear semantic for users who
    /// actively cleared the field in the new UI.
    pub system_customized: bool,
    /// Composed runtime value (base prompt plus slash-command appendix).
    /// Not serialized; computed by the loader.
    #[serde(skip)]
    pub resolved_system: String,
}

impl Default for PromptSection {
    fn default() -> Self {
        Self {
            system: DEFAULT_SYSTEM_PROMPT_BASE.to_string(),
            system_customized: DEFAULT_SYSTEM_CUSTOMIZED,
            resolved_system: String::new(),
        }
    }
}

/// Overlay UI configuration. Holds window geometry and input attachment
/// limits. The collapsed-bar height and the close-animation deadline are
/// baked into the frontend (see `App.tsx`) because their effective range is
/// invisible to the user (collapsed height is overwritten by the
/// ResizeObserver within a frame; the hide delay sits below normal perception
/// across its usable range and creates a visible pop if dropped below the
/// exit-animation duration).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct WindowSection {
    /// Logical width of the overlay window.
    pub overlay_width: f64,
    /// Maximum height the expanded chat window is allowed to grow to.
    pub max_chat_height: f64,
    /// Maximum number of manually attached images per message. One additional
    /// image from /screen capture is allowed on top, for a total of
    /// max_images + 1 per message.
    pub max_images: u32,
    /// Base font size (in CSS pixels) for chat text and the AskBar input.
    /// Drives the `--thuki-text-base` CSS variable consumed by the AI
    /// markdown body, the user chat bubble text, and the AskBar textarea
    /// (plus its caret-tracking mirror). Other UI surfaces keep fixed sizes.
    /// Valid range: 11.0..=22.0.
    pub text_base_px: f64,
    /// Line-height multiplier applied to chat + AskBar text. Drives the
    /// `--thuki-text-line-height` CSS variable. Valid range: 1.0..=2.5.
    pub text_line_height: f64,
    /// Letter spacing (in CSS pixels) applied to chat + AskBar text.
    /// Drives the `--thuki-text-letter-spacing` CSS variable. Negative
    /// values tighten the typography; positive values airy it out.
    /// Valid range: -0.5..=2.0.
    pub text_letter_spacing_px: f64,
    /// CSS `font-weight` applied to chat + AskBar text. Drives the
    /// `--thuki-text-font-weight` CSS variable. Restricted to the four
    /// loaded Nunito weights (400, 500, 600, 700); values outside this
    /// set reset to the compiled default.
    pub text_font_weight: u32,
}

impl Default for WindowSection {
    fn default() -> Self {
        Self {
            overlay_width: DEFAULT_OVERLAY_WIDTH,
            max_chat_height: DEFAULT_MAX_CHAT_HEIGHT,
            max_images: DEFAULT_MAX_IMAGES,
            text_base_px: DEFAULT_TEXT_BASE_PX,
            text_line_height: DEFAULT_TEXT_LINE_HEIGHT,
            text_letter_spacing_px: DEFAULT_TEXT_LETTER_SPACING_PX,
            text_font_weight: DEFAULT_TEXT_FONT_WEIGHT,
        }
    }
}

/// Selected-text quote display configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct QuoteSection {
    pub max_display_lines: u32,
    pub max_display_chars: u32,
    pub max_context_length: u32,
}

impl Default for QuoteSection {
    fn default() -> Self {
        Self {
            max_display_lines: DEFAULT_QUOTE_MAX_DISPLAY_LINES,
            max_display_chars: DEFAULT_QUOTE_MAX_DISPLAY_CHARS,
            max_context_length: DEFAULT_QUOTE_MAX_CONTEXT_LENGTH,
        }
    }
}

/// Search pipeline and service configuration.
///
/// Service URLs control where the SearXNG and reader sidecar processes live.
/// The defaults match the Docker sandbox bindings in `sandbox/docker-compose.yml`.
/// Users who remap ports or run the services on a different host set these in
/// `[search]` in config.toml; no rebuild required.
///
/// Pipeline tuning knobs (`max_iterations`, `top_k_urls`) let users trade
/// search quality against latency. Timeout fields cover slow networks and slow
/// local hardware. Values that would create an inconsistency (e.g.
/// `reader_batch_timeout_s <= reader_per_url_timeout_s`) are silently corrected
/// by the loader.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SearchSection {
    /// Base URL of the SearXNG instance (scheme + host + port, no path).
    /// The `/search` endpoint is appended automatically.
    pub searxng_url: String,
    /// Base URL of the reader/extractor sidecar (scheme + host + port, no path).
    pub reader_url: String,
    /// Maximum number of search-refine iterations before the pipeline gives up.
    pub max_iterations: u32,
    /// Number of top-ranked URLs forwarded to the reader after reranking.
    pub top_k_urls: u32,
    /// Maximum number of results each SearXNG query contributes to the
    /// reranker. Acts before rerank to bound prompt size and latency: lower
    /// values trade recall for speed; higher values give the reranker more
    /// candidates per query.
    pub searxng_max_results: u32,
    /// Seconds before a SearXNG query is abandoned.
    pub search_timeout_s: u64,
    /// Seconds allowed for a single URL fetch inside the reader.
    pub reader_per_url_timeout_s: u64,
    /// Seconds allowed for the full parallel reader batch to complete.
    /// Must exceed `reader_per_url_timeout_s`; the loader corrects violations.
    pub reader_batch_timeout_s: u64,
    /// Seconds before the judge LLM call is abandoned.
    pub judge_timeout_s: u64,
    /// Seconds before the router LLM call is abandoned.
    pub router_timeout_s: u64,
    /// Wall-clock budget for the full `/search` pipeline turn (seconds).
    /// When exceeded, the gap-refinement loop bails out early and the
    /// pipeline force-synthesizes on whatever evidence it has gathered,
    /// surfacing a `BudgetExhausted` warning. Raise for deeper research;
    /// lower for snappier interactive use.
    pub pipeline_wall_clock_budget_s: u64,
}

impl Default for SearchSection {
    fn default() -> Self {
        Self {
            searxng_url: DEFAULT_SEARXNG_URL.to_string(),
            reader_url: DEFAULT_READER_URL.to_string(),
            max_iterations: DEFAULT_MAX_ITERATIONS,
            top_k_urls: DEFAULT_TOP_K_URLS,
            searxng_max_results: DEFAULT_SEARXNG_MAX_RESULTS,
            search_timeout_s: DEFAULT_SEARCH_TIMEOUT_S,
            reader_per_url_timeout_s: DEFAULT_READER_PER_URL_TIMEOUT_S,
            reader_batch_timeout_s: DEFAULT_READER_BATCH_TIMEOUT_S,
            judge_timeout_s: DEFAULT_JUDGE_TIMEOUT_S,
            router_timeout_s: DEFAULT_ROUTER_TIMEOUT_S,
            pipeline_wall_clock_budget_s: DEFAULT_PIPELINE_WALL_CLOCK_BUDGET_S,
        }
    }
}

/// Text-to-speech configuration. Study Buddy Pro can speak through the local
/// Supertonic sidecar or OpenRouter's `/audio/speech` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct VoiceSection {
    /// Enables the voice system. When false, speak commands become no-ops.
    pub enabled: bool,
    /// Automatically speak guided Study Mode turns. Normal chat stays manual.
    pub auto_speak_study: bool,
    /// Active voice provider: "supertonic" or "openrouter".
    pub provider: String,
    /// Base URL of the local Supertonic server.
    pub base_url: String,
    /// Built-in or imported Supertonic voice/style name.
    pub voice: String,
    /// Voice identifier sent to OpenRouter TTS models.
    pub openrouter_voice: String,
    /// ISO language code, or "auto" to let Study Buddy Pro infer per turn.
    pub lang: String,
    /// Supertonic quality/speed steps.
    pub steps: u32,
    /// Spoken speed multiplier.
    pub speed: f64,
    /// Chunk size sent to Supertonic for long tutor responses.
    pub max_chunk_length: u32,
}

impl Default for VoiceSection {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_VOICE_ENABLED,
            auto_speak_study: DEFAULT_VOICE_AUTO_SPEAK_STUDY,
            provider: DEFAULT_VOICE_PROVIDER.to_string(),
            base_url: DEFAULT_VOICE_BASE_URL.to_string(),
            voice: DEFAULT_VOICE_NAME.to_string(),
            openrouter_voice: DEFAULT_VOICE_OPENROUTER_VOICE.to_string(),
            lang: DEFAULT_VOICE_LANG.to_string(),
            steps: DEFAULT_VOICE_STEPS,
            speed: DEFAULT_VOICE_SPEED,
            max_chunk_length: DEFAULT_VOICE_MAX_CHUNK_LENGTH,
        }
    }
}

/// Developer and power-user debugging knobs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct DebugSection {
    /// Records every chat conversation and `/search` session to JSON-Lines
    /// files under `app_data_dir/traces/{chat,search}/<conversation_id>.jsonl`.
    /// Off by default; toggleable from Settings.
    pub trace_enabled: bool,
}

impl Default for DebugSection {
    fn default() -> Self {
        Self {
            trace_enabled: DEFAULT_DEBUG_TRACE_ENABLED,
        }
    }
}

/// Auto-update configuration. Determines whether and how often Thuki polls
/// for new releases via the bundled tauri-plugin-updater.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct UpdaterSection {
    /// Poll for updates automatically at startup and every
    /// `check_interval_hours` hours while running.
    #[serde(default = "default_updater_auto_check")]
    pub auto_check: bool,

    /// Hours between automatic background checks. Bound to 1..168.
    #[serde(default = "default_updater_check_interval_hours")]
    pub check_interval_hours: u64,

    /// URL to fetch the update manifest from.
    #[serde(default = "default_updater_manifest_url")]
    pub manifest_url: String,
}

fn default_updater_auto_check() -> bool {
    DEFAULT_UPDATER_AUTO_CHECK
}
fn default_updater_check_interval_hours() -> u64 {
    DEFAULT_UPDATER_CHECK_INTERVAL_HOURS
}
fn default_updater_manifest_url() -> String {
    DEFAULT_UPDATER_MANIFEST_URL.to_string()
}

impl Default for UpdaterSection {
    fn default() -> Self {
        Self {
            auto_check: DEFAULT_UPDATER_AUTO_CHECK,
            check_interval_hours: DEFAULT_UPDATER_CHECK_INTERVAL_HOURS,
            manifest_url: DEFAULT_UPDATER_MANIFEST_URL.to_string(),
        }
    }
}

/// Top-level application configuration. Managed Tauri state; every subsystem
/// reads from `State<RwLock<AppConfig>>` and nowhere else. The loader resolves all
/// empty strings and out-of-bounds numerics to compiled defaults before the
/// `AppConfig` is installed, so every field here holds a usable value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct AppConfig {
    pub inference: InferenceSection,
    pub openrouter: OpenRouterSection,
    pub prompt: PromptSection,
    pub window: WindowSection,
    pub quote: QuoteSection,
    pub search: SearchSection,
    pub voice: VoiceSection,
    pub debug: DebugSection,
    #[serde(default)]
    pub updater: UpdaterSection,
}
