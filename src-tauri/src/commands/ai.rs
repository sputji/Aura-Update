use super::config::AppState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRequest {
    pub context: String,       // what happened (package + error, or file + reason)
    pub context_type: String,  // "update_error", "cleanup_advice", "general"
}

#[tauri::command]
pub fn ai_is_available(state: tauri::State<'_, AppState>) -> bool {
    let cfg = state.config.lock().unwrap();
    cfg.ai_enabled && cfg.ai_consent_given && !cfg.ai_api_key.is_empty()
}

#[tauri::command]
pub async fn configure_ai(
    state: tauri::State<'_, AppState>,
    enabled: bool,
    endpoint: String,
    api_key: String,
    consent_given: bool,
) -> Result<bool, String> {
    let mut cfg = state.config.lock().unwrap();
    cfg.ai_enabled = enabled;
    cfg.ai_endpoint = endpoint;
    cfg.ai_api_key = api_key;
    cfg.ai_consent_given = consent_given;
    // Save is handled by set_config_value, but we persist here too
    let data_dir = state.data_dir.clone();
    let cfg_snapshot = cfg.clone();
    drop(cfg);
    super::config::save_config(&data_dir, &cfg_snapshot);
    Ok(true)
}

/// Ask Aura-IA for analysis. Sends ONLY technical context — no personal data.
#[tauri::command]
pub async fn ai_analyze(
    state: tauri::State<'_, AppState>,
    request: AiRequest,
) -> Result<String, String> {
    let cfg = state.config.lock().unwrap().clone();

    if !cfg.ai_enabled || !cfg.ai_consent_given || cfg.ai_api_key.is_empty() {
        return Err("AI not configured".into());
    }

    let system_prompt = match request.context_type.as_str() {
        "update_error" => {
            "You are a system update assistant. The user has an update error. \
             Explain the problem simply and suggest solutions. Be concise (3-5 lines max). \
             Answer in the user's language. No personal data is shared with you."
        }
        "cleanup_advice" => {
            "You are a system cleanup advisor. Explain in one simple sentence why this \
             file/folder can be safely deleted. Be reassuring and concise."
        }
        _ => {
            "You are Aura-IA, a helpful PC health assistant. Be concise and clear."
        }
    };

    let body = serde_json::json!({
        "model": "aura-ia",
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": request.context }
        ],
        "max_tokens": 300,
        "temperature": 0.4,
    });

    // Endpoint and App Key hardcoded — never exposed to frontend
    const AI_ENDPOINT: &str = "https://ia.auraneo.fr/v1/chat/completions";
    const APP_KEY: &str = "aura_aura_update_mmkzgiz4";

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post(AI_ENDPOINT)
        .header("Authorization", format!("Bearer {}", cfg.ai_api_key))
        .header("X-App-Key", APP_KEY)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("AI API error: {}", resp.status()));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let content = json
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("No response from AI");

    Ok(content.to_string())
}
