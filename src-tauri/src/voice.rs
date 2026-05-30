use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Mutex;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Manager, State};

use crate::config::AppConfig;

pub struct VoicePlaybackState {
    child: Mutex<Option<Child>>,
}

impl VoicePlaybackState {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
        }
    }
}

impl Default for VoicePlaybackState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Serialize)]
pub struct VoiceHealth {
    pub reachable: bool,
    pub base_url: String,
    pub status: Option<String>,
    pub model: Option<String>,
    pub sample_rate: Option<u32>,
    pub voices_loaded: Option<u32>,
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct VoiceSpeakResult {
    pub spoken: bool,
    pub path: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct VoiceStartResult {
    pub started: bool,
    pub repo_path: Option<String>,
    pub runtime_path: Option<String>,
    pub message: String,
}

#[derive(Clone, Serialize)]
pub struct SupertonicRuntimeStatus {
    pub found: bool,
    pub runtime_path: Option<String>,
    pub source_path: Option<String>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
struct SupertonicHealth {
    status: Option<String>,
    model: Option<String>,
    sample_rate: Option<u32>,
    voices_loaded: Option<u32>,
}

fn voice_base_url(config: &AppConfig) -> String {
    config.voice.base_url.trim_end_matches('/').to_string()
}

fn stop_child(state: &VoicePlaybackState) {
    if let Ok(mut guard) = state.child.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn speech_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("failed to resolve app cache dir: {e}"))?
        .join("speech");
    fs::create_dir_all(&dir).map_err(|e| format!("failed to create speech cache: {e}"))?;
    Ok(dir.join("assistant.wav"))
}

fn spawn_player(path: &PathBuf) -> Result<Child, String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("afplay")
            .arg(path)
            .spawn()
            .map_err(|e| format!("failed to start afplay: {e}"))
    }

    #[cfg(target_os = "windows")]
    {
        let escaped = path.to_string_lossy().replace('\'', "''");
        let script = format!("(New-Object Media.SoundPlayer '{escaped}').PlaySync()");
        return Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .spawn()
            .map_err(|e| format!("failed to start Windows audio playback: {e}"));
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Command::new("aplay")
            .arg(path)
            .spawn()
            .map_err(|e| format!("failed to start aplay: {e}"))
    }
}

fn supertonic_candidates(app: &AppHandle) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(dir) = std::env::var("STUDY_BUDDY_SUPERTONIC_DIR") {
        candidates.push(PathBuf::from(dir));
    }

    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("supertonic"));
        candidates.push(resource_dir.join("resources").join("supertonic"));
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("resources").join("supertonic"));
        candidates.push(cwd.join("src-tauri").join("resources").join("supertonic"));
        candidates.push(cwd.join("supertonic"));
        if let Some(parent) = cwd.parent() {
            candidates.push(parent.join("supertonic"));
        }
    }

    if let Some(documents) = dirs::document_dir() {
        candidates.push(documents.join("codex-projects").join("supertonic"));
    }

    candidates
}

fn supertonic_manage_py(dir: &Path) -> PathBuf {
    dir.join("native-server").join("manage.py")
}

fn find_supertonic_source_dir(app: &AppHandle) -> Option<PathBuf> {
    supertonic_candidates(app)
        .into_iter()
        .find(|dir| supertonic_manage_py(dir).is_file())
}

fn writable_supertonic_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("failed to resolve app local data dir: {e}"))?
        .join("supertonic"))
}

fn copy_dir_contents(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst)
        .map_err(|e| format!("failed to create runtime directory {}: {e}", dst.display()))?;
    for entry in fs::read_dir(src)
        .map_err(|e| format!("failed to read bundled runtime {}: {e}", src.display()))?
    {
        let entry = entry.map_err(|e| format!("failed to read runtime entry: {e}"))?;
        let file_name = entry.file_name();
        if matches!(
            file_name.to_str(),
            Some("__pycache__" | ".pytest_cache" | ".mypy_cache")
        ) {
            continue;
        }
        let src_path = entry.path();
        let dst_path = dst.join(&file_name);
        let file_type = entry
            .file_type()
            .map_err(|e| format!("failed to inspect {}: {e}", src_path.display()))?;
        if file_type.is_dir() {
            copy_dir_contents(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            fs::copy(&src_path, &dst_path).map_err(|e| {
                format!(
                    "failed to copy Supertonic runtime {} -> {}: {e}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn ensure_supertonic_runtime(app: &AppHandle) -> Result<PathBuf, String> {
    let runtime = writable_supertonic_dir(app)?;
    let source = find_supertonic_source_dir(app);

    if !supertonic_manage_py(&runtime).is_file() {
        let Some(source) = source.as_ref() else {
            return Err(
                "Supertonic runtime was not found. Bundle resources/supertonic or set STUDY_BUDDY_SUPERTONIC_DIR."
                    .to_string(),
            );
        };
        if !same_path(source, &runtime) {
            copy_dir_contents(
                &source.join("native-server"),
                &runtime.join("native-server"),
            )?;
        }
    } else if let Some(source) = source.as_ref() {
        if !same_path(source, &runtime) {
            // Refresh the lightweight manager/config files without touching
            // .native-venv or native-runtime in the writable folder.
            copy_dir_contents(
                &source.join("native-server"),
                &runtime.join("native-server"),
            )?;
        }
    }

    if supertonic_manage_py(&runtime).is_file() {
        Ok(runtime)
    } else {
        Err("Supertonic runtime copy did not produce native-server/manage.py.".to_string())
    }
}

fn existing_supertonic_runtime(app: &AppHandle) -> Option<PathBuf> {
    writable_supertonic_dir(app)
        .ok()
        .filter(|dir| supertonic_manage_py(dir).is_file())
        .or_else(|| find_supertonic_source_dir(app))
}

pub fn supertonic_runtime_status(app: &AppHandle) -> SupertonicRuntimeStatus {
    let runtime = writable_supertonic_dir(app);
    let source = find_supertonic_source_dir(app);
    match runtime {
        Ok(runtime_path) => {
            let found = supertonic_manage_py(&runtime_path).is_file() || source.is_some();
            SupertonicRuntimeStatus {
                found,
                runtime_path: Some(runtime_path.to_string_lossy().to_string()),
                source_path: source.map(|p| p.to_string_lossy().to_string()),
                error: None,
            }
        }
        Err(error) => SupertonicRuntimeStatus {
            found: source.is_some(),
            runtime_path: None,
            source_path: source.map(|p| p.to_string_lossy().to_string()),
            error: Some(error),
        },
    }
}

fn python_command() -> Result<(String, Vec<String>), String> {
    #[cfg(target_os = "windows")]
    {
        if Command::new("py").arg("--version").output().is_ok() {
            return Ok(("py".to_string(), vec!["-3".to_string()]));
        }
        if Command::new("python").arg("--version").output().is_ok() {
            return Ok(("python".to_string(), Vec::new()));
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        for candidate in ["python3.11", "python3.12", "python3.13", "python3"] {
            if Command::new(candidate).arg("--version").output().is_ok() {
                return Ok((candidate.to_string(), Vec::new()));
            }
        }
    }

    Err("Python 3 is required to start the local Supertonic server.".to_string())
}

pub fn python_available() -> bool {
    python_command().is_ok()
}

fn spawn_supertonic_manager(
    app: &AppHandle,
    repo: &PathBuf,
    command_name: &str,
) -> Result<(), String> {
    let (python, mut args) = python_command()?;
    args.extend([
        "native-server/manage.py".to_string(),
        command_name.to_string(),
    ]);

    if command_name == "start" {
        args.push("--no-wait".to_string());
    }

    let log_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("failed to resolve app cache dir: {e}"))?
        .join("supertonic");
    fs::create_dir_all(&log_dir)
        .map_err(|e| format!("failed to create Supertonic log dir: {e}"))?;
    let log_path = log_dir.join(format!("{command_name}.log"));
    let stdout = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("failed to open Supertonic log: {e}"))?;
    let stderr = stdout
        .try_clone()
        .map_err(|e| format!("failed to clone Supertonic log handle: {e}"))?;

    let mut command = Command::new(python);
    command
        .args(args)
        .current_dir(repo)
        .stdout(stdout)
        .stderr(stderr);
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("failed to launch Supertonic manager: {e}"))
}

fn speakable_text(input: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code || trimmed.starts_with('|') {
            continue;
        }
        let stripped = trimmed
            .trim_start_matches(['#', '>', '-', '*', ' '])
            .replace(['`', '*', '_'], "");
        if !stripped.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&stripped);
        }
    }
    out.trim().to_string()
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub async fn voice_health(
    client: State<'_, reqwest::Client>,
    app_config: State<'_, RwLock<AppConfig>>,
) -> Result<VoiceHealth, String> {
    let config = app_config.read().clone();
    let base_url = voice_base_url(&config);
    let url = format!("{base_url}/v1/health");
    let health = match client.get(&url).send().await {
        Ok(response) if response.status().is_success() => {
            match response.json::<SupertonicHealth>().await {
                Ok(body) => VoiceHealth {
                    reachable: true,
                    base_url,
                    status: body.status,
                    model: body.model,
                    sample_rate: body.sample_rate,
                    voices_loaded: body.voices_loaded,
                    error: None,
                },
                Err(e) => VoiceHealth {
                    reachable: false,
                    base_url,
                    status: None,
                    model: None,
                    sample_rate: None,
                    voices_loaded: None,
                    error: Some(format!("invalid health response: {e}")),
                },
            }
        }
        Ok(response) => VoiceHealth {
            reachable: false,
            base_url,
            status: None,
            model: None,
            sample_rate: None,
            voices_loaded: None,
            error: Some(format!("HTTP {}", response.status())),
        },
        Err(e) => VoiceHealth {
            reachable: false,
            base_url,
            status: None,
            model: None,
            sample_rate: None,
            voices_loaded: None,
            error: Some(e.to_string()),
        },
    };
    Ok(health)
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub async fn voice_styles(
    client: State<'_, reqwest::Client>,
    app_config: State<'_, RwLock<AppConfig>>,
) -> Result<serde_json::Value, String> {
    let config = app_config.read().clone();
    let url = format!("{}/v1/styles", voice_base_url(&config));
    let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Supertonic styles request failed: {}",
            response.status()
        ));
    }
    response.json().await.map_err(|e| e.to_string())
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub async fn speak_text(
    text: String,
    app: AppHandle,
    client: State<'_, reqwest::Client>,
    app_config: State<'_, RwLock<AppConfig>>,
    playback: State<'_, VoicePlaybackState>,
) -> Result<VoiceSpeakResult, String> {
    let config = app_config.read().clone();
    if !config.voice.enabled {
        return Ok(VoiceSpeakResult {
            spoken: false,
            path: None,
            error: Some("voice is disabled".to_string()),
        });
    }

    let text = speakable_text(&text);
    if text.is_empty() {
        return Ok(VoiceSpeakResult {
            spoken: false,
            path: None,
            error: Some("nothing speakable".to_string()),
        });
    }

    stop_child(&playback);

    let payload = json!({
        "text": text,
        "voice": config.voice.voice,
        "lang": if config.voice.lang == "auto" { "na" } else { config.voice.lang.as_str() },
        "steps": config.voice.steps,
        "speed": config.voice.speed,
        "max_chunk_length": config.voice.max_chunk_length,
        "response_format": "wav",
    });

    let url = format!("{}/v1/tts", voice_base_url(&config));
    let response = match client.post(&url).json(&payload).send().await {
        Ok(response) => response,
        Err(e) => {
            return Ok(VoiceSpeakResult {
                spoken: false,
                path: None,
                error: Some(e.to_string()),
            });
        }
    };
    if !response.status().is_success() {
        return Ok(VoiceSpeakResult {
            spoken: false,
            path: None,
            error: Some(format!("Supertonic TTS failed: {}", response.status())),
        });
    }

    let bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => {
            return Ok(VoiceSpeakResult {
                spoken: false,
                path: None,
                error: Some(e.to_string()),
            });
        }
    };
    let path = match speech_path(&app) {
        Ok(path) => path,
        Err(e) => {
            return Ok(VoiceSpeakResult {
                spoken: false,
                path: None,
                error: Some(e),
            });
        }
    };
    if let Err(e) = fs::write(&path, &bytes) {
        return Ok(VoiceSpeakResult {
            spoken: false,
            path: None,
            error: Some(format!("failed to write speech WAV: {e}")),
        });
    }

    let result = match spawn_player(&path) {
        Ok(child) => {
            if let Ok(mut guard) = playback.child.lock() {
                *guard = Some(child);
            }
            VoiceSpeakResult {
                spoken: true,
                path: Some(path.to_string_lossy().to_string()),
                error: None,
            }
        }
        Err(e) => VoiceSpeakResult {
            spoken: false,
            path: Some(path.to_string_lossy().to_string()),
            error: Some(e),
        },
    };
    Ok(result)
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn stop_speech(playback: State<'_, VoicePlaybackState>) {
    stop_child(&playback);
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn voice_start(app: AppHandle) -> Result<VoiceStartResult, String> {
    let repo = ensure_supertonic_runtime(&app)?;

    spawn_supertonic_manager(&app, &repo, "start")?;
    Ok(VoiceStartResult {
        started: true,
        repo_path: Some(repo.to_string_lossy().to_string()),
        runtime_path: Some(repo.to_string_lossy().to_string()),
        message: "Supertonic startup requested. Readiness may take a while on first install or model download."
            .to_string(),
    })
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn voice_stop(app: AppHandle, playback: State<'_, VoicePlaybackState>) -> Result<(), String> {
    stop_child(&playback);
    if let Some(repo) = existing_supertonic_runtime(&app) {
        spawn_supertonic_manager(&app, &repo, "stop")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::speakable_text;

    #[test]
    fn speakable_text_removes_code_blocks_and_markdown_noise() {
        let input = "# Title\n\n- Explain `photosynthesis`.\n```js\nalert(1)\n```\n| a | b |";
        assert_eq!(speakable_text(input), "Title\nExplain photosynthesis.");
    }
}
