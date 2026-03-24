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
    let is_local = cfg.ai_endpoint.contains("localhost") || cfg.ai_endpoint.contains("127.0.0.1");
    cfg.ai_enabled && cfg.ai_consent_given && (is_local || !cfg.ai_api_key.is_empty())
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

    let is_local = cfg.ai_endpoint.contains("localhost") || cfg.ai_endpoint.contains("127.0.0.1");
    if !cfg.ai_enabled || !cfg.ai_consent_given || (!is_local && cfg.ai_api_key.is_empty()) {
        return Err("AI not configured".into());
    }

    let lang_instruction = if cfg.language == "fr" {
        "IMPORTANT: Always answer in French."
    } else {
        "IMPORTANT: Always answer in English."
    };

    let base_prompt = match request.context_type.as_str() {
        "update_error" => {
            "You are a system update assistant. The user has an update error. \
             Explain the problem simply and suggest solutions. Be concise (3-5 lines max). \
             No personal data is shared with you."
        }
        "cleanup_advice" => {
            "You are a system cleanup advisor. Explain in one simple sentence why this \
             file/folder can be safely deleted. Be reassuring and concise."
        }
        _ => {
            "You are Aura-IA, a helpful PC health assistant. Be concise and clear."
        }
    };

    let system_prompt = format!("{} {}", base_prompt, lang_instruction);

    // ── Detect provider type ──
    let is_auraneo = cfg.ai_endpoint.contains("auraneo.fr");
    let is_xai = cfg.ai_endpoint.contains("x.ai");

    // ── Build endpoint URL ──
    let endpoint = if is_auraneo {
        // Aura-IA uses a custom gateway at /api/agents/chat
        let base = cfg.ai_endpoint.trim_end_matches('/');
        if base.ends_with("/api/agents/chat") {
            base.to_string()
        } else {
            format!("{}/api/agents/chat", base)
        }
    } else if cfg.ai_endpoint.is_empty() {
        "https://ia.auraneo.fr/api/agents/chat".to_string()
    } else if cfg.ai_endpoint.ends_with("/chat/completions") || cfg.ai_endpoint.ends_with("/chat/completions/") {
        cfg.ai_endpoint.clone()
    } else if cfg.ai_endpoint.contains("/v1beta/") || cfg.ai_endpoint.contains("/v2beta/") {
        format!("{}/chat/completions", cfg.ai_endpoint.trim_end_matches('/'))
    } else {
        format!("{}/v1/chat/completions", cfg.ai_endpoint.trim_end_matches('/'))
    };

    // ── Build request body ──
    let body = if is_auraneo {
        // Aura-IA ChatDto format
        serde_json::json!({
            "agentId": cfg.ai_model.clone(),
            "userMessage": request.context,
            "conversationHistory": [],
            "language": cfg.language.clone(),
        })
    } else {
        let mut b = serde_json::json!({
            "model": cfg.ai_model.clone(),
            "messages": [
                { "role": "system", "content": &system_prompt },
                { "role": "user", "content": request.context }
            ],
            "temperature": 0.4,
        });
        if is_xai {
            b["max_completion_tokens"] = serde_json::json!(300);
        } else {
            b["max_tokens"] = serde_json::json!(300);
        }
        b
    };

    let is_local = endpoint.contains("localhost") || endpoint.contains("127.0.0.1");

    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(if is_local { 120 } else { 60 }));

    if is_local {
        builder = builder.danger_accept_invalid_certs(true);
    }

    let client = builder.build().map_err(|e| e.to_string())?;

    let mut req = client
        .post(&endpoint)
        .header("Content-Type", "application/json")
        .json(&body);

    // ── Authorization headers ──
    if !cfg.ai_api_key.is_empty() {
        if is_auraneo && cfg.ai_api_key.contains(':') {
            // Aura-IA: "publicKey:secretKey" → X-Api-Key + X-Api-Secret headers
            let parts: Vec<&str> = cfg.ai_api_key.splitn(2, ':').collect();
            req = req.header("X-Api-Key", parts[0])
                     .header("X-Api-Secret", parts[1]);
        } else {
            // All providers: standard Bearer token (OpenAI, Gemini, Grok, Aura-IA fallback)
            req = req.header("Authorization", format!("Bearer {}", cfg.ai_api_key));
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let status_code = status.as_u16();
        let err_body = resp.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<serde_json::Value>(&err_body)
            .ok()
            .and_then(|v| {
                // Try OpenAI format: { error: { message: "..." } }
                v.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str().map(String::from))
                // Try simple format: { message: "..." }
                .or_else(|| v.get("message").and_then(|m| m.as_str().map(String::from)))
            })
            .unwrap_or_default();
        let hint = match status_code {
            400 => " — Vérifiez le nom du modèle et le format de la requête",
            401 => " — Clé API invalide ou expirée",
            403 => " — Accès refusé. Vérifiez votre clé API et votre abonnement",
            404 => " — Modèle ou endpoint introuvable",
            429 => " — Quota dépassé. Vérifiez votre facturation",
            500..=599 => " — Erreur serveur du fournisseur IA",
            _ => "",
        };
        if detail.is_empty() {
            return Err(format!("API error {}{}", status, hint));
        } else {
            return Err(format!("API error {}: {}", status, detail));
        }
    }

    // ── Parse response ──
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let content = if is_auraneo {
        // Aura-IA returns { "response": "..." }
        json.get("response")
            .and_then(|r| r.as_str())
            .unwrap_or("No response from Aura-IA")
    } else {
        // OpenAI format: { choices: [{ message: { content: "..." } }] }
        json.get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("No response from AI")
    };

    Ok(content.to_string())
}
