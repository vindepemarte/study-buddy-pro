use parking_lot::RwLock;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::config::AppConfig;

#[derive(Clone, Serialize)]
pub struct SetupReadiness {
    pub os: String,
    pub ollama_reachable: bool,
    pub ollama_models: usize,
    pub windows_ocr_model: Option<String>,
    pub windows_ocr_model_installed: bool,
    pub python_available: bool,
    pub mlx_vlm_supported: bool,
    pub mlx_vlm_ready: bool,
    pub mlx_vlm_model: Option<String>,
    pub mlx_vlm_runtime_path: Option<String>,
    pub supertonic_runtime_found: bool,
    pub supertonic_runtime_path: Option<String>,
    pub voice_reachable: bool,
    pub docker_available: bool,
    pub searxng_reachable: bool,
    pub reader_reachable: bool,
    pub search_ready: bool,
    pub search_runtime_path: Option<String>,
    pub core_ready: bool,
    pub ready: bool,
    pub missing: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Serialize)]
pub struct SearchServicesResult {
    pub started: bool,
    pub runtime_path: String,
    pub message: String,
}

async fn endpoint_ok(client: &reqwest::Client, url: String) -> bool {
    client
        .get(url)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

async fn ollama_models(client: &reqwest::Client, base_url: &str) -> (bool, Vec<String>) {
    let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
    let Ok(response) = client.get(url).send().await else {
        return (false, Vec::new());
    };
    if !response.status().is_success() {
        return (false, Vec::new());
    }
    let Ok(value) = response.json::<serde_json::Value>().await else {
        return (true, Vec::new());
    };
    let models = value
        .get("models")
        .and_then(|v| v.as_array())
        .map(|models| {
            models
                .iter()
                .filter_map(|model| model.get("name").and_then(|name| name.as_str()))
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    (true, models)
}

fn docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn copy_dir_contents(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dst)
        .map_err(|e| format!("failed to create search runtime {}: {e}", dst.display()))?;
    for entry in std::fs::read_dir(src)
        .map_err(|e| format!("failed to read search resource {}: {e}", src.display()))?
    {
        let entry = entry.map_err(|e| format!("failed to read search resource entry: {e}"))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|e| format!("failed to inspect {}: {e}", src_path.display()))?;
        if file_type.is_dir() {
            copy_dir_contents(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            std::fs::copy(&src_path, &dst_path).map_err(|e| {
                format!(
                    "failed to copy search resource {} -> {}: {e}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn search_resource_candidates(app: &AppHandle) -> Vec<std::path::PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("search-box"));
        candidates.push(resource_dir.join("resources").join("search-box"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("resources").join("search-box"));
        candidates.push(cwd.join("src-tauri").join("resources").join("search-box"));
        candidates.push(cwd.join("sandbox").join("search-box"));
    }
    candidates
}

fn find_search_resource_dir(app: &AppHandle) -> Option<std::path::PathBuf> {
    search_resource_candidates(app)
        .into_iter()
        .find(|dir| dir.join("docker-compose.yml").is_file())
}

fn search_runtime_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("failed to resolve app local data dir: {e}"))?
        .join("search-box"))
}

fn ensure_search_runtime(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let runtime = search_runtime_dir(app)?;
    let Some(resource) = find_search_resource_dir(app) else {
        return Err("Bundled search-box resources were not found.".to_string());
    };
    copy_dir_contents(&resource, &runtime)?;
    Ok(runtime)
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn start_search_services(app: AppHandle) -> Result<SearchServicesResult, String> {
    if !docker_available() {
        return Err(
            "Docker Desktop is required for /search. Install Docker Desktop, start it, then try again."
                .to_string(),
        );
    }
    let runtime = ensure_search_runtime(&app)?;
    let compose = runtime.join("docker-compose.yml");
    let output = std::process::Command::new("docker")
        .args(["compose", "-f"])
        .arg(&compose)
        .args(["up", "-d", "--build"])
        .current_dir(&runtime)
        .output()
        .map_err(|e| format!("failed to start Docker search services: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "Docker search services failed to start: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(SearchServicesResult {
        started: true,
        runtime_path: runtime.to_string_lossy().to_string(),
        message: "Search services are starting. Re-check setup in a few seconds.".to_string(),
    })
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn stop_search_services(app: AppHandle) -> Result<(), String> {
    let runtime = search_runtime_dir(&app)?;
    let compose = runtime.join("docker-compose.yml");
    if !compose.is_file() {
        return Ok(());
    }
    let output = std::process::Command::new("docker")
        .args(["compose", "-f"])
        .arg(&compose)
        .arg("down")
        .current_dir(&runtime)
        .output()
        .map_err(|e| format!("failed to stop Docker search services: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Docker search services failed to stop: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub async fn get_setup_readiness(
    app: AppHandle,
    client: State<'_, reqwest::Client>,
    app_config: State<'_, RwLock<AppConfig>>,
) -> Result<SetupReadiness, String> {
    let config = app_config.read().clone();
    let (ollama_reachable, installed_models) =
        ollama_models(&client, &config.inference.ollama_url).await;
    let ollama_models = installed_models.len();
    let windows_ocr_model = if cfg!(target_os = "windows") {
        Some(crate::ocr::WINDOWS_OCR_MODEL.to_string())
    } else {
        None
    };
    let windows_ocr_model_installed = windows_ocr_model
        .as_deref()
        .is_none_or(|model| installed_models.iter().any(|installed| installed == model));
    let voice_reachable = endpoint_ok(
        &client,
        format!("{}/v1/health", config.voice.base_url.trim_end_matches('/')),
    )
    .await;
    let searxng_reachable = endpoint_ok(
        &client,
        format!(
            "{}/search?q=study-buddy-pro&format=json",
            config.search.searxng_url.trim_end_matches('/')
        ),
    )
    .await;
    let reader_reachable = endpoint_ok(
        &client,
        format!("{}/healthz", config.search.reader_url.trim_end_matches('/')),
    )
    .await;
    let docker_available = docker_available();
    let python_available = crate::voice::python_available();
    let mlx_vlm = crate::mlx_vlm::mlx_vlm_status(app.clone()).ok();
    let runtime = crate::voice::supertonic_runtime_status(&app);
    let search_ready = docker_available && searxng_reachable && reader_reachable;
    let search_runtime_path = search_runtime_dir(&app)
        .ok()
        .map(|path| path.to_string_lossy().to_string());
    let core_ready = ollama_reachable
        && ollama_models > 0
        && windows_ocr_model_installed
        && python_available
        && runtime.found
        && voice_reachable;
    let ready = core_ready;

    let mut missing = Vec::new();
    if !ollama_reachable {
        missing.push("ollama".to_string());
    } else if ollama_models == 0 {
        missing.push("ollama_model".to_string());
    }
    if !windows_ocr_model_installed {
        if let Some(model) = windows_ocr_model.as_deref() {
            missing.push(format!("windows_ocr_model:{model}"));
        }
    }
    if !python_available {
        missing.push("python3".to_string());
    }
    if !runtime.found {
        missing.push("supertonic_runtime".to_string());
    }
    if !voice_reachable {
        missing.push("supertonic_voice".to_string());
    }

    let mut warnings = Vec::new();
    if !search_ready {
        warnings.push("search_optional_offline".to_string());
    }
    if let Some(error) = runtime.error.clone() {
        warnings.push(format!("supertonic_runtime_status:{error}"));
    }
    if let Some(mlx_vlm) = mlx_vlm.as_ref() {
        if mlx_vlm.supported && !mlx_vlm.ready {
            warnings.push("mlx_vlm_optional_not_ready".to_string());
        }
    }

    Ok(SetupReadiness {
        os: std::env::consts::OS.to_string(),
        ollama_reachable,
        ollama_models,
        windows_ocr_model,
        windows_ocr_model_installed,
        python_available,
        mlx_vlm_supported: mlx_vlm.as_ref().is_some_and(|status| status.supported),
        mlx_vlm_ready: mlx_vlm.as_ref().is_some_and(|status| status.ready),
        mlx_vlm_model: mlx_vlm.as_ref().map(|status| status.model_id.clone()),
        mlx_vlm_runtime_path: mlx_vlm
            .as_ref()
            .and_then(|status| status.runtime_path.clone()),
        supertonic_runtime_found: runtime.found,
        supertonic_runtime_path: runtime.runtime_path,
        voice_reachable,
        docker_available,
        searxng_reachable,
        reader_reachable,
        search_ready,
        search_runtime_path,
        core_ready,
        ready,
        missing,
        warnings,
    })
}
