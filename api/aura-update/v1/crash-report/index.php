<?php
/**
 * Aura Update — Crash Report API
 * Endpoint: POST /aura-update/v1/crash-report
 *
 * Receives crash reports from the desktop app, validates the security token,
 * formats the data and sends an HTML email to the support inbox.
 *
 * No user data is stored on the server (RGPD-compliant).
 */

// ── Configuration ────────────────────────────────────────────
define('AURA_TOKEN', getenv('AURA_UPDATE_TOKEN') ?: 'CHANGE_ME_TO_RANDOM_SECRET');
define('MAIL_TO', 'contact@auraneo.fr');
define('MAIL_FROM', 'noreply@auraneo.fr');
define('MAX_BODY_SIZE', 65536); // 64 KB max

// ── CORS ─────────────────────────────────────────────────────
header('Content-Type: application/json; charset=utf-8');
header('X-Content-Type-Options: nosniff');

// Preflight
if ($_SERVER['REQUEST_METHOD'] === 'OPTIONS') {
    http_response_code(204);
    exit;
}

// ── Only accept POST ─────────────────────────────────────────
if ($_SERVER['REQUEST_METHOD'] !== 'POST') {
    http_response_code(405);
    echo json_encode(['error' => 'Method not allowed']);
    exit;
}

// ── Validate security token ──────────────────────────────────
$token = $_SERVER['HTTP_X_AURA_TOKEN'] ?? '';
if (!hash_equals(AURA_TOKEN, $token)) {
    http_response_code(403);
    echo json_encode(['error' => 'Forbidden']);
    exit;
}

// ── Read & parse JSON body ───────────────────────────────────
$raw = file_get_contents('php://input', false, null, 0, MAX_BODY_SIZE);
if (empty($raw)) {
    http_response_code(400);
    echo json_encode(['error' => 'Empty body']);
    exit;
}

$data = json_decode($raw, true);
if (!is_array($data)) {
    http_response_code(400);
    echo json_encode(['error' => 'Invalid JSON']);
    exit;
}

// ── Extract fields (sanitize) ────────────────────────────────
$crash_data    = htmlspecialchars(mb_substr($data['crash_data'] ?? '', 0, 5000), ENT_QUOTES, 'UTF-8');
$user_message  = htmlspecialchars(mb_substr($data['user_message'] ?? '', 0, 2000), ENT_QUOTES, 'UTF-8');
$os            = htmlspecialchars(mb_substr($data['os'] ?? 'unknown', 0, 50), ENT_QUOTES, 'UTF-8');
$app_version   = htmlspecialchars(mb_substr($data['app_version'] ?? 'unknown', 0, 20), ENT_QUOTES, 'UTF-8');
$log_tail      = htmlspecialchars(mb_substr($data['log_tail'] ?? '', 0, 10000), ENT_QUOTES, 'UTF-8');

$date = date('Y-m-d H:i:s');

// ── Build HTML email ─────────────────────────────────────────
$subject = "[Aura Update] Crash Report — v{$app_version} / {$os}";

$body = <<<HTML
<!DOCTYPE html>
<html><head><meta charset="UTF-8"></head>
<body style="font-family: -apple-system, BlinkMacSystemFont, sans-serif; color: #222; max-width: 700px;">
<h2 style="color: #e74c3c;">⚠️ Aura Update — Crash Report</h2>
<table style="border-collapse: collapse; width: 100%;">
<tr><td style="padding:6px; font-weight:bold; color:#555;">Date</td><td style="padding:6px;">{$date}</td></tr>
<tr><td style="padding:6px; font-weight:bold; color:#555;">App Version</td><td style="padding:6px;">{$app_version}</td></tr>
<tr><td style="padding:6px; font-weight:bold; color:#555;">OS</td><td style="padding:6px;">{$os}</td></tr>
</table>

<h3 style="margin-top: 20px;">💬 Message utilisateur</h3>
<div style="background: #f7f7f7; padding: 12px; border-radius: 6px; white-space: pre-wrap;">{$user_message}</div>

<h3 style="margin-top: 20px;">🔴 Données du crash</h3>
<pre style="background: #2d2d2d; color: #f8f8f2; padding: 12px; border-radius: 6px; overflow-x: auto; font-size: 13px;">{$crash_data}</pre>

<h3 style="margin-top: 20px;">📋 Dernières lignes du log</h3>
<pre style="background: #2d2d2d; color: #a6e22e; padding: 12px; border-radius: 6px; overflow-x: auto; font-size: 12px; max-height: 400px;">{$log_tail}</pre>

<hr style="margin-top: 30px; border: none; border-top: 1px solid #ddd;">
<p style="color: #999; font-size: 12px;">Ce rapport a été généré automatiquement par Aura Update.<br>Aucune donnée personnelle n'est stockée sur le serveur.</p>
</body></html>
HTML;

// ── Send email ───────────────────────────────────────────────
$headers  = "From: Aura Update <" . MAIL_FROM . ">\r\n";
$headers .= "Reply-To: " . MAIL_FROM . "\r\n";
$headers .= "MIME-Version: 1.0\r\n";
$headers .= "Content-Type: text/html; charset=UTF-8\r\n";

$sent = mail(MAIL_TO, $subject, $body, $headers);

if ($sent) {
    http_response_code(200);
    echo json_encode(['success' => true, 'message' => 'Crash report received']);
} else {
    // Log the failure for debugging (no user data in the log)
    error_log("[aura-update] Failed to send crash report email at {$date}");
    http_response_code(500);
    echo json_encode(['error' => 'Failed to send email']);
}
