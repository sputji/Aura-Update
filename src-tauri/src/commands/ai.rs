use super::config::AppState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRequest {
    pub context: String,       // what happened (package + error, or file + reason)
    pub context_type: String,  // "update_error", "cleanup_advice", "general"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModelInfo {
    pub id: String,
    pub name: String,
}

/// Fetch available models for a given provider.
/// `provider`: "gemini", "openai", "grok", "ollama", "auraneo", "custom"
/// `endpoint`: the API endpoint URL
/// `api_key`: the API key (may be empty for local)
#[tauri::command]
pub async fn list_ai_models(
    provider: String,
    endpoint: String,
    api_key: String,
) -> Result<Vec<AiModelInfo>, String> {
    let is_local = endpoint.contains("localhost") || endpoint.contains("127.0.0.1");
    let is_auraneo = endpoint.contains("auraneo.fr") || provider == "auraneo";

    let mut builder = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(8))
        .timeout(std::time::Duration::from_secs(15));

    if is_local || is_auraneo {
        builder = builder.danger_accept_invalid_certs(true);
    }

    let client = builder.build().map_err(|e| e.to_string())?;

    match provider.as_str() {
        "ollama" => {
            // Ollama: GET /api/tags
            let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));
            let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("Ollama error: {}", resp.status()));
            }
            let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            let models = json.get("models")
                .and_then(|m| m.as_array())
                .map(|arr| arr.iter().filter_map(|m| {
                    let name = m.get("name")?.as_str()?.to_string();
                    Some(AiModelInfo { id: name.clone(), name })
                }).collect::<Vec<_>>())
                .unwrap_or_default();
            Ok(models)
        }
        "auraneo" => {
            // Aura-IA: GET /api/agents/modes
            let base = endpoint.trim_end_matches('/');
            let base = if base.ends_with("/api/agents/chat") {
                &base[..base.len() - "/chat".len()]
            } else if base.ends_with("/api/agents") {
                base
            } else {
                base
            };
            let url = format!("{}/api/agents/modes", base);
            let mut req = client.get(&url);
            if !api_key.is_empty() {
                if api_key.contains(':') {
                    let parts: Vec<&str> = api_key.splitn(2, ':').collect();
                    req = req.header("X-Api-Key", parts[0]).header("X-Api-Secret", parts[1]);
                } else {
                    req = req.header("Authorization", format!("Bearer {}", api_key));
                }
            }
            let resp = req.send().await.map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                // Fallback: return hardcoded modes
                return Ok(vec![
                    AiModelInfo { id: "rapide".into(), name: "Rapide (Qwen 7B)".into() },
                    AiModelInfo { id: "reflexions".into(), name: "Réflexions (Qwen 7B)".into() },
                    AiModelInfo { id: "intelligent".into(), name: "Intelligent (DeepSeek 7B)".into() },
                ]);
            }
            let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            // Try to parse as array of modes
            let models = if let Some(arr) = json.as_array() {
                arr.iter().filter_map(|m| {
                    let id = m.get("agentId").or(m.get("id")).and_then(|v| v.as_str())?.to_string();
                    let name = m.get("name").or(m.get("label")).and_then(|v| v.as_str())
                        .unwrap_or(&id).to_string();
                    Some(AiModelInfo { id, name })
                }).collect()
            } else if let Some(modes) = json.get("modes").and_then(|m| m.as_array()) {
                modes.iter().filter_map(|m| {
                    let id = m.get("agentId").or(m.get("id")).and_then(|v| v.as_str())?.to_string();
                    let name = m.get("name").or(m.get("label")).and_then(|v| v.as_str())
                        .unwrap_or(&id).to_string();
                    Some(AiModelInfo { id, name })
                }).collect()
            } else {
                // Fallback hardcoded
                vec![
                    AiModelInfo { id: "rapide".into(), name: "Rapide (Qwen 7B)".into() },
                    AiModelInfo { id: "reflexions".into(), name: "Réflexions (Qwen 7B)".into() },
                    AiModelInfo { id: "intelligent".into(), name: "Intelligent (DeepSeek 7B)".into() },
                ]
            };
            Ok(models)
        }
        "gemini" | "openai" | "grok" | _ => {
            // OpenAI-compatible: GET /v1/models (also works for Gemini, xAI, etc.)
            let base = endpoint.trim_end_matches('/');
            let url = if base.contains("/v1beta/") || base.contains("/v2beta/") {
                // Gemini: use the Google-specific models endpoint
                format!("{}/models", base.split("/openai").next().unwrap_or(base))
            } else if base.ends_with("/v1") {
                format!("{}/models", base)
            } else if base.contains("/v1/") {
                format!("{}/models", base.split("/v1/").next().map(|b| format!("{}/v1", b)).unwrap_or(base.to_string()))
            } else {
                format!("{}/v1/models", base)
            };

            let mut req = client.get(&url);
            if !api_key.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", api_key));
            }
            let resp = match req.send().await {
                Ok(r) => r,
                Err(_) => {
                    // Return provider-specific hardcoded models as fallback
                    return Ok(get_fallback_models(&provider));
                }
            };
            if !resp.status().is_success() {
                return Ok(get_fallback_models(&provider));
            }
            let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

            let mut models: Vec<AiModelInfo> = if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
                // OpenAI / xAI format: { data: [{ id: "model-name", ... }] }
                data.iter().filter_map(|m| {
                    let id = m.get("id")?.as_str()?.to_string();
                    Some(AiModelInfo { id: id.clone(), name: id })
                }).collect()
            } else if let Some(models_arr) = json.get("models").and_then(|m| m.as_array()) {
                // Gemini format: { models: [{ name: "models/gemini-...", displayName: "..." }] }
                models_arr.iter().filter_map(|m| {
                    let full_name = m.get("name")?.as_str()?;
                    let id = full_name.strip_prefix("models/").unwrap_or(full_name).to_string();
                    let display = m.get("displayName").and_then(|d| d.as_str()).unwrap_or(&id).to_string();
                    Some(AiModelInfo { id, name: display })
                }).collect()
            } else {
                get_fallback_models(&provider)
            };

            // Filter: only keep chat/text models, not image/embedding/tts
            models.retain(|m| {
                let id = m.id.to_lowercase();
                !id.contains("embed") && !id.contains("tts") && !id.contains("whisper")
                && !id.contains("dall") && !id.contains("image") && !id.contains("video")
                && !id.contains("moderation")
            });

            // Sort by name
            models.sort_by(|a, b| a.name.cmp(&b.name));

            if models.is_empty() {
                return Ok(get_fallback_models(&provider));
            }
            Ok(models)
        }
    }
}

fn get_fallback_models(provider: &str) -> Vec<AiModelInfo> {
    match provider {
        "gemini" => vec![
            AiModelInfo { id: "gemini-2.5-flash".into(), name: "Gemini 2.5 Flash".into() },
            AiModelInfo { id: "gemini-2.5-pro".into(), name: "Gemini 2.5 Pro".into() },
            AiModelInfo { id: "gemini-2.0-flash-lite".into(), name: "Gemini 2.0 Flash Lite".into() },
        ],
        "openai" => vec![
            AiModelInfo { id: "gpt-4o-mini".into(), name: "GPT-4o Mini".into() },
            AiModelInfo { id: "gpt-4o".into(), name: "GPT-4o".into() },
            AiModelInfo { id: "gpt-4.1-mini".into(), name: "GPT-4.1 Mini".into() },
            AiModelInfo { id: "gpt-4.1-nano".into(), name: "GPT-4.1 Nano".into() },
            AiModelInfo { id: "gpt-3.5-turbo".into(), name: "GPT-3.5 Turbo".into() },
        ],
        "grok" => vec![
            AiModelInfo { id: "grok-4-1-fast-non-reasoning".into(), name: "Grok 4.1 Fast".into() },
            AiModelInfo { id: "grok-4-1-fast-reasoning".into(), name: "Grok 4.1 Fast (Reasoning)".into() },
            AiModelInfo { id: "grok-4.20-0309-non-reasoning".into(), name: "Grok 4.20".into() },
        ],
        "ollama" => vec![
            AiModelInfo { id: "llama3".into(), name: "Llama 3".into() },
            AiModelInfo { id: "mistral".into(), name: "Mistral".into() },
            AiModelInfo { id: "neural-chat".into(), name: "Neural Chat".into() },
            AiModelInfo { id: "codellama".into(), name: "Code Llama".into() },
            AiModelInfo { id: "phi3".into(), name: "Phi-3".into() },
        ],
        _ => vec![],
    }
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
    let _is_xai = cfg.ai_endpoint.contains("x.ai");

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
        let b = serde_json::json!({
            "model": cfg.ai_model.clone(),
            "messages": [
                { "role": "system", "content": &system_prompt },
                { "role": "user", "content": request.context }
            ],
            "temperature": 0.4,
            "max_tokens": 300,
        });
        b
    };

    let is_local = endpoint.contains("localhost") || endpoint.contains("127.0.0.1");

    let mut builder = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(if is_local { 120 } else if is_auraneo { 90 } else { 60 }));

    // Accept invalid certs for local servers AND Aura-IA (rustls can fail on some certs)
    if is_local || is_auraneo {
        builder = builder.danger_accept_invalid_certs(true);
    }

    let client = builder.build().map_err(|e| e.to_string())?;

    // ── Send with retry on connection error ──
    let send_request = |client: &reqwest::Client| {
        let mut req = client
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .json(&body);

        // Authorization headers
        if !cfg.ai_api_key.is_empty() {
            if is_auraneo && cfg.ai_api_key.contains(':') {
                let parts: Vec<&str> = cfg.ai_api_key.splitn(2, ':').collect();
                req = req.header("X-Api-Key", parts[0])
                         .header("X-Api-Secret", parts[1]);
            } else {
                req = req.header("Authorization", format!("Bearer {}", cfg.ai_api_key));
            }
        }
        req
    };

    // First attempt
    let resp = match send_request(&client).send().await {
        Ok(r) => r,
        Err(e) if e.is_connect() || e.is_timeout() => {
            // Retry once on connection/timeout errors
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            send_request(&client).send().await
                .map_err(|e2| format!("Connexion impossible après 2 tentatives: {}", e2))?
        }
        Err(e) => return Err(format!("Erreur réseau: {}", e)),
    };

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
