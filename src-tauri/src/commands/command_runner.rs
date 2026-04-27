use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use rand::{thread_rng, Rng};
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use super::logging;

#[derive(Debug, Clone)]
pub struct RunSpec {
    pub task: String,
    pub action: String,
    pub step: String,
    pub program: String,
    pub args: Vec<String>,
    pub timeout_secs: u64,
    pub start_percent: u8,
    pub done_percent: u8,
}

#[derive(Debug, Clone)]
pub struct CommandRunResult {
    pub run_id: String,
    pub output: String,
    pub success: bool,
    pub canceled: bool,
    pub timed_out: bool,
}

#[derive(Clone, serde::Serialize)]
struct MaintenanceEvent {
    task: String,
    status: String,
    output: String,
    percent: u8,
    run_id: String,
    heartbeat: bool,
    duration_ms: u128,
}

static RUN_CANCELLERS: LazyLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn request_cancel(task: &str) -> bool {
    if let Some(flag) = RUN_CANCELLERS.lock().unwrap().get(task).cloned() {
        flag.store(true, Ordering::Relaxed);
        return true;
    }
    false
}

fn emit_event(
    app: &tauri::AppHandle,
    task: &str,
    status: &str,
    output: &str,
    percent: u8,
    run_id: &str,
    heartbeat: bool,
    duration_ms: u128,
) {
    let _ = app.emit(
        "maintenance-progress",
        MaintenanceEvent {
            task: task.to_string(),
            status: status.to_string(),
            output: output.to_string(),
            percent,
            run_id: run_id.to_string(),
            heartbeat,
            duration_ms,
        },
    );
}

fn make_run_id(action: &str) -> String {
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let mut rng = thread_rng();
    let nonce: u32 = rng.gen_range(1000..=9999);
    format!("{action}-{ts}-{nonce}")
}

fn cap_line(v: &str) -> String {
    let max = 1400;
    if v.len() <= max {
        v.to_string()
    } else {
        format!("{}…", &v[..max])
    }
}

fn append_output_line(combined: &mut String, stream: &str, line: &str) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    if !combined.is_empty() {
        combined.push('\n');
    }
    combined.push_str(&format!("[{stream}] {trimmed}"));
}

pub async fn run_logged_command(
    app: &tauri::AppHandle,
    spec: RunSpec,
) -> Result<CommandRunResult, String> {
    let run_id = make_run_id(&spec.action);
    let cancel_flag = Arc::new(AtomicBool::new(false));
    RUN_CANCELLERS
        .lock()
        .unwrap()
        .insert(spec.task.clone(), cancel_flag.clone());

    let started = Instant::now();
    emit_event(
        app,
        &spec.task,
        "start",
        &format!("{}…", spec.step),
        spec.start_percent,
        &run_id,
        false,
        0,
    );
    logging::log_action_event(
        &run_id,
        "maintenance",
        &spec.action,
        "start",
        Some(&spec.step),
        None,
        None,
        None,
        Some(spec.timeout_secs * 1000),
        false,
        &format!("{} {}", spec.program, spec.args.join(" ")),
    );

    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        cmd.creation_flags(0x0800_0000);
    }

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            RUN_CANCELLERS.lock().unwrap().remove(&spec.task);
            logging::log_action_event(
                &run_id,
                "maintenance",
                &spec.action,
                "error",
                Some(&spec.step),
                None,
                None,
                None,
                Some(spec.timeout_secs * 1000),
                false,
                &format!("spawn_error: {e}"),
            );
            return Err(format!("Impossible d'exécuter {}: {e}", spec.program));
        }
    };

    let pid = child.id();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, mut rx) = mpsc::unbounded_channel::<(String, String)>();

    if let Some(out) = stdout {
        let txo = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = txo.send(("stdout".to_string(), line));
            }
        });
    }
    if let Some(err) = stderr {
        let txe = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = txe.send(("stderr".to_string(), line));
            }
        });
    }

    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(5));
    let deadline = Instant::now() + std::time::Duration::from_secs(spec.timeout_secs);
    let mut combined = String::new();
    let mut canceled = false;
    let mut timed_out = false;
    let mut status: Option<std::process::ExitStatus> = None;

    loop {
        if cancel_flag.load(Ordering::Relaxed) {
            canceled = true;
            let _ = child.kill().await;
            break;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill().await;
            break;
        }

        tokio::select! {
            Some((stream, line)) = rx.recv() => {
                let line = cap_line(&line);
                append_output_line(&mut combined, &stream, &line);
                let elapsed = started.elapsed().as_millis();
                emit_event(app, &spec.task, "progress", &line, spec.start_percent, &run_id, false, elapsed);
                logging::log_action_event(
                    &run_id,
                    "maintenance",
                    &spec.action,
                    "progress",
                    Some(&spec.step),
                    pid,
                    None,
                    Some(elapsed),
                    Some(spec.timeout_secs * 1000),
                    false,
                    &line,
                );
            }
            _ = heartbeat.tick() => {
                let elapsed = started.elapsed().as_millis();
                let message = format!("Action en cours… {}s", (elapsed / 1000));
                emit_event(app, &spec.task, "heartbeat", &message, spec.start_percent, &run_id, true, elapsed);
                logging::log_action_event(
                    &run_id,
                    "maintenance",
                    &spec.action,
                    "heartbeat",
                    Some(&spec.step),
                    pid,
                    None,
                    Some(elapsed),
                    Some(spec.timeout_secs * 1000),
                    false,
                    &message,
                );
            }
            wait_res = child.wait() => {
                status = wait_res.ok();
                break;
            }
        }
    }

    // Drain any remaining buffered lines after process exit.
    while let Ok((stream, line)) = rx.try_recv() {
        let line = cap_line(&line);
        append_output_line(&mut combined, &stream, &line);
    }

    let duration = started.elapsed().as_millis();
    let exit_code = status.and_then(|s| s.code());
    let success = !canceled && !timed_out && exit_code.unwrap_or(-1) == 0;

    let final_status = if canceled {
        "canceled"
    } else if timed_out {
        "error"
    } else if success {
        "done"
    } else {
        "error"
    };

    let final_message = if canceled {
        "Action annulée par l'utilisateur".to_string()
    } else if timed_out {
        format!("Délai dépassé après {}s", spec.timeout_secs)
    } else if success {
        if combined.trim().is_empty() {
            format!("{} terminé avec succès (run_id: {})", spec.step, run_id)
        } else {
            combined.clone()
        }
    } else if combined.trim().is_empty() {
        format!("Erreur (code {})", exit_code.unwrap_or(-1))
    } else {
        combined.clone()
    };

    emit_event(
        app,
        &spec.task,
        final_status,
        &final_message,
        if success { spec.done_percent } else { spec.start_percent },
        &run_id,
        false,
        duration,
    );
    logging::log_action_event(
        &run_id,
        "maintenance",
        &spec.action,
        final_status,
        Some(&spec.step),
        pid,
        exit_code,
        Some(duration),
        Some(spec.timeout_secs * 1000),
        canceled,
        &final_message,
    );

    RUN_CANCELLERS.lock().unwrap().remove(&spec.task);

    let result = CommandRunResult {
        run_id,
        output: final_message,
        success,
        canceled,
        timed_out,
    };

    println!(
        "[command_runner] action={} success={} canceled={} timed_out={}",
        result.run_id,
        result.success,
        result.canceled,
        result.timed_out
    );

    if result.success {
        Ok(result)
    } else if result.canceled {
        Err("Action annulée par l'utilisateur".to_string())
    } else if result.timed_out {
        Err(format!("Timeout de {}s atteint", spec.timeout_secs))
    } else {
        Err(result.output.clone())
    }
}
