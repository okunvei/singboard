use serde::Serialize;
use std::path::{Path, PathBuf};

const MAX_SCAN_DEPTH: usize = 8;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedRuntimeFiles {
    pub base_dir: String,
    pub singbox_path: Option<String>,
    pub config_path: Option<String>,
    pub found: bool,
}

fn is_singbox_binary(file_name: &str) -> bool {
    matches!(
        file_name.to_ascii_lowercase().as_str(),
        "sing-box.exe" | "sing-box" | "singbox.exe" | "singbox"
    )
}

fn scan_dir(
    dir: &Path,
    depth: usize,
    singbox_path: &mut Option<PathBuf>,
    config_path: &mut Option<PathBuf>,
) -> Result<(), String> {
    if depth > MAX_SCAN_DEPTH || (singbox_path.is_some() && config_path.is_some()) {
        return Ok(());
    }

    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory {}: {}", dir.display(), e))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    entries.sort();

    for path in entries {
        if singbox_path.is_some() && config_path.is_some() {
            break;
        }

        if path.is_file() {
            if config_path.is_none()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.eq_ignore_ascii_case("config.json"))
                    .unwrap_or(false)
            {
                *config_path = Some(path.clone());
            }

            if singbox_path.is_none() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if is_singbox_binary(file_name) {
                        *singbox_path = Some(path.clone());
                    }
                }
            }
        } else if path.is_dir() {
            scan_dir(&path, depth + 1, singbox_path, config_path)?;
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn detect_runtime_files() -> Result<DetectedRuntimeFiles, String> {
    tokio::task::spawn_blocking(move || {
        let base_dir = std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?;
        let mut singbox_path = None;
        let mut config_path = None;

        scan_dir(&base_dir, 0, &mut singbox_path, &mut config_path)?;
        let found = singbox_path.is_some() && config_path.is_some();

        Ok(DetectedRuntimeFiles {
            base_dir: base_dir.to_string_lossy().to_string(),
            singbox_path: singbox_path.map(|p| p.to_string_lossy().to_string()),
            config_path: config_path.map(|p| p.to_string_lossy().to_string()),
            found,
        })
    })
    .await
    .map_err(|e| format!("Failed to run detection task: {}", e))?
}

#[tauri::command]
pub async fn read_config(path: String) -> Result<String, String> {
    tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Failed to read config: {}", e))
}

#[tauri::command]
pub async fn write_config(path: String, content: String) -> Result<(), String> {
    let backup = format!("{}.bak", path);
    if Path::new(&path).exists() {
        let _ = tokio::fs::copy(&path, &backup).await;
    }

    serde_json::from_str::<serde_json::Value>(&content)
        .map_err(|e| format!("Invalid JSON: {}", e))?;

    tokio::fs::write(&path, &content)
        .await
        .map_err(|e| format!("Failed to write config: {}", e))
}

#[tauri::command]
pub async fn validate_config(singbox_path: String, config_path: String) -> Result<String, String> {
    let output = tokio::process::Command::new(&singbox_path)
        .args(["check", "-c", &config_path])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output()
        .await
        .map_err(|e| format!("Failed to run sing-box check: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok("Configuration is valid".into())
    } else {
        Err(format!("{}\n{}", stdout, stderr).trim().to_string())
    }
}
