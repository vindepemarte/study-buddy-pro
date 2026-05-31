use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

pub const DEFAULT_MLX_VLM_MODEL: &str = "mlx-community/Qwen3-VL-8B-Instruct-4bit";
const RUNTIME_DIR: &str = "mlx-vlm";
const VENV_DIR: &str = ".venv";

#[derive(Clone, Serialize)]
pub struct MlxVlmStatus {
    pub supported: bool,
    pub apple_silicon: bool,
    pub python_available: bool,
    pub runtime_path: Option<String>,
    pub venv_python: Option<String>,
    pub package_installed: bool,
    pub model_id: String,
    pub model_cached: bool,
    pub ready: bool,
    pub installed_versions: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct MlxVlmInstallResult {
    pub installed: bool,
    pub status: MlxVlmStatus,
    pub message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MlxVlmDescribeRequest {
    pub image_paths: Vec<String>,
    pub ocr_text: String,
    pub note: Option<String>,
    pub model_id: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct MlxVlmDescribeResponse {
    pub model_id: String,
    pub notes: String,
}

fn is_apple_silicon() -> bool {
    cfg!(target_os = "macos") && std::env::consts::ARCH == "aarch64"
}

fn python_version_supported(candidate: &str) -> bool {
    let script =
        "import sys; raise SystemExit(0 if sys.version_info[:2] in [(3, 11), (3, 12)] else 1)";
    Command::new(candidate)
        .args(["-c", script])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn python_command() -> Option<String> {
    for candidate in ["python3.12", "python3.11", "python3"] {
        if python_version_supported(candidate) {
            return Some(candidate.to_string());
        }
    }
    None
}

fn runtime_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("failed to resolve app local data dir: {e}"))?
        .join(RUNTIME_DIR))
}

fn venv_python(runtime: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        runtime.join(VENV_DIR).join("Scripts").join("python.exe")
    }

    #[cfg(not(target_os = "windows"))]
    {
        runtime.join(VENV_DIR).join("bin").join("python")
    }
}

fn apply_runtime_env(command: &mut Command, runtime: &Path) {
    command
        .env("HF_HOME", runtime.join("hf-cache"))
        .env("TOKENIZERS_PARALLELISM", "false")
        .env("PYTHONUNBUFFERED", "1");
}

fn run_output(command: &mut Command, context: &str) -> Result<String, String> {
    let output = command
        .output()
        .map_err(|e| format!("{context} failed to start: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(format!("{context} failed: {detail}"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn versions(runtime: &Path, python: &Path) -> Option<String> {
    let script = concat!(
        "import importlib.metadata, json\n",
        "import mlx\n",
        "print(json.dumps({",
        "'mlx': getattr(mlx, '__version__', 'unknown'),",
        "'mlx_vlm': importlib.metadata.version('mlx-vlm')",
        "}))\n"
    );
    let mut command = Command::new(python);
    command.args(["-c", script]).current_dir(runtime);
    apply_runtime_env(&mut command, runtime);
    run_output(&mut command, "MLX-VLM version probe").ok()
}

fn package_installed(runtime: &Path, python: &Path) -> bool {
    let mut command = Command::new(python);
    command
        .args(["-c", "import mlx, mlx_vlm, huggingface_hub"])
        .current_dir(runtime);
    apply_runtime_env(&mut command, runtime);
    command
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn model_cached(runtime: &Path, python: &Path, model_id: &str) -> bool {
    let script = concat!(
        "from huggingface_hub import snapshot_download\n",
        "import sys\n",
        "snapshot_download(repo_id=sys.argv[1], local_files_only=True)\n"
    );
    let mut command = Command::new(python);
    command.args(["-c", script, model_id]).current_dir(runtime);
    apply_runtime_env(&mut command, runtime);
    command
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn status_for(app: &AppHandle, model_id: &str) -> MlxVlmStatus {
    let apple_silicon = is_apple_silicon();
    let supported = apple_silicon;
    let python_available = python_command().is_some();
    let runtime = match runtime_dir(app) {
        Ok(path) => path,
        Err(error) => {
            return MlxVlmStatus {
                supported,
                apple_silicon,
                python_available,
                runtime_path: None,
                venv_python: None,
                package_installed: false,
                model_id: model_id.to_string(),
                model_cached: false,
                ready: false,
                installed_versions: None,
                error: Some(error),
            };
        }
    };
    let python = venv_python(&runtime);
    let venv_exists = python.is_file();
    let package_installed = supported && venv_exists && package_installed(&runtime, &python);
    let model_cached = package_installed && model_cached(&runtime, &python, model_id);
    let ready = supported && python_available && package_installed && model_cached;
    let error = if !supported {
        Some("MLX-VLM is only enabled on Apple Silicon macOS.".to_string())
    } else if !python_available {
        Some("Python 3.11 or 3.12 is required for the MLX-VLM runtime.".to_string())
    } else if !venv_exists {
        Some("MLX-VLM runtime has not been installed yet.".to_string())
    } else if !package_installed {
        Some("MLX-VLM Python packages are not installed yet.".to_string())
    } else if !model_cached {
        Some(format!("MLX-VLM model is not downloaded yet: {model_id}"))
    } else {
        None
    };

    MlxVlmStatus {
        supported,
        apple_silicon,
        python_available,
        runtime_path: Some(runtime.to_string_lossy().to_string()),
        venv_python: venv_exists.then(|| python.to_string_lossy().to_string()),
        package_installed,
        model_id: model_id.to_string(),
        model_cached,
        ready,
        installed_versions: if package_installed {
            versions(&runtime, &python)
        } else {
            None
        },
        error,
    }
}

fn install_runtime(app: &AppHandle, model_id: &str) -> Result<MlxVlmStatus, String> {
    if !is_apple_silicon() {
        return Err("MLX-VLM install is only supported on Apple Silicon macOS.".to_string());
    }
    let Some(system_python) = python_command() else {
        return Err("Python 3.11 or 3.12 is required for MLX-VLM.".to_string());
    };
    let runtime = runtime_dir(app)?;
    std::fs::create_dir_all(&runtime)
        .map_err(|e| format!("failed to create MLX-VLM runtime directory: {e}"))?;
    let python = venv_python(&runtime);
    if !python.is_file() {
        let mut command = Command::new(system_python);
        command.args(["-m", "venv", VENV_DIR]).current_dir(&runtime);
        run_output(&mut command, "MLX-VLM venv creation")?;
    }

    let mut pip = Command::new(&python);
    pip.args(["-m", "pip", "install", "--upgrade", "pip"])
        .current_dir(&runtime);
    apply_runtime_env(&mut pip, &runtime);
    run_output(&mut pip, "MLX-VLM pip upgrade")?;

    let mut install = Command::new(&python);
    install
        .args([
            "-m",
            "pip",
            "install",
            "--upgrade",
            "mlx-vlm",
            "huggingface_hub",
        ])
        .current_dir(&runtime);
    apply_runtime_env(&mut install, &runtime);
    run_output(&mut install, "MLX-VLM package install")?;

    let script = concat!(
        "from huggingface_hub import snapshot_download\n",
        "import sys\n",
        "snapshot_download(repo_id=sys.argv[1])\n"
    );
    let mut download = Command::new(&python);
    download
        .args(["-c", script, model_id])
        .current_dir(&runtime);
    apply_runtime_env(&mut download, &runtime);
    run_output(&mut download, "MLX-VLM model download")?;

    Ok(status_for(app, model_id))
}

fn build_study_prompt(ocr_text: &str, note: Option<&str>) -> String {
    let title = note
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("saved study page");
    format!(
        "You are indexing a Study Buddy Pro source page for later answer checking.\n\
         Source note: {title}\n\n\
         Return concise markdown only. Do not guess beyond the image and OCR.\n\
         Include:\n\
         - Page type\n\
         - Key rules or facts\n\
         - Quiz question/options/selected answer if visible\n\
         - Important terms a student may need explained\n\
         - Evidence useful for checking future answers\n\n\
         OCR text from Apple Vision:\n{ocr_text}"
    )
}

fn trim_model_output(stdout: &str) -> String {
    stdout
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn mlx_vlm_status(app: AppHandle) -> Result<MlxVlmStatus, String> {
    Ok(status_for(&app, DEFAULT_MLX_VLM_MODEL))
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn mlx_vlm_install(
    app: AppHandle,
    model_id: Option<String>,
) -> Result<MlxVlmInstallResult, String> {
    let model_id = model_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_MLX_VLM_MODEL);
    let status = install_runtime(&app, model_id)?;
    Ok(MlxVlmInstallResult {
        installed: status.ready,
        status,
        message: "MLX Vision is installed. Future /remember saves can include structured page understanding."
            .to_string(),
    })
}

#[tauri::command]
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn mlx_vlm_describe_images(
    app: AppHandle,
    request: MlxVlmDescribeRequest,
) -> Result<MlxVlmDescribeResponse, String> {
    let model_id = request
        .model_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_MLX_VLM_MODEL)
        .to_string();
    if request.image_paths.is_empty() {
        return Err("MLX-VLM needs at least one image path.".to_string());
    }
    let status = status_for(&app, &model_id);
    if !status.ready {
        return Err(status
            .error
            .unwrap_or_else(|| "MLX-VLM runtime is not ready.".to_string()));
    }
    let runtime = runtime_dir(&app)?;
    let python = venv_python(&runtime);
    let prompt = build_study_prompt(&request.ocr_text, request.note.as_deref());

    let mut command = Command::new(&python);
    command
        .args(["-m", "mlx_vlm.generate"])
        .arg("--model")
        .arg(&model_id)
        .args(["--max-tokens", "700", "--temperature", "0.0"])
        .arg("--prompt")
        .arg(&prompt)
        .arg("--image")
        .args(&request.image_paths)
        .current_dir(&runtime);
    apply_runtime_env(&mut command, &runtime);
    let stdout = run_output(&mut command, "MLX-VLM page understanding")?;
    let notes = trim_model_output(&stdout);
    if notes.is_empty() {
        return Err("MLX-VLM returned no page notes.".to_string());
    }
    Ok(MlxVlmDescribeResponse { model_id, notes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_study_prompt_includes_ocr_and_note() {
        let prompt = build_study_prompt("Stop signs require a full stop.", Some("Chapter 4"));
        assert!(prompt.contains("Chapter 4"));
        assert!(prompt.contains("Stop signs require a full stop."));
        assert!(prompt.contains("Do not guess"));
    }

    #[test]
    fn trim_model_output_removes_blank_edges() {
        assert_eq!(trim_model_output("\n\nA\n\nB  \n"), "A\nB");
    }
}
