/**
 * Aura Update — Crash Report API (Node.js)
 * Endpoint: POST /aura-update/v1/crash-report
 *
 * Receives crash reports from the desktop app, validates the security token,
 * formats the data and sends an HTML email to the support inbox.
 *
 * No user data is stored on the server (RGPD-compliant).
 */

const http = require('http');
const nodemailer = require('nodemailer');

// ── Configuration ────────────────────────────────
const PORT = parseInt(process.env.PORT, 10) || 8300;
const AURA_TOKEN = process.env.AURA_UPDATE_TOKEN || 'CHANGE_ME_TO_RANDOM_SECRET';
const MAIL_TO = process.env.MAIL_TO || 'contact@auraneo.fr';
const MAIL_FROM = process.env.MAIL_FROM || 'noreply@auraneo.fr';
const SMTP_HOST = process.env.SMTP_HOST || 'ssl0.ovh.net';
const SMTP_PORT = parseInt(process.env.SMTP_PORT, 10) || 587;
const SMTP_USER = process.env.SMTP_USER || '';
const SMTP_PASS = process.env.SMTP_PASS || '';
const MAX_BODY_SIZE = 65536; // 64 KB

// ── SMTP transporter ─────────────────────────────
const smtpConfig = {
  host: SMTP_HOST,
  port: SMTP_PORT,
  secure: SMTP_PORT === 465,
};
if (SMTP_USER && SMTP_PASS) {
  smtpConfig.auth = { user: SMTP_USER, pass: SMTP_PASS };
}
const transporter = nodemailer.createTransport(smtpConfig);

// ── Helpers ───────────────────────────────────────
function escapeHtml(str) {
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function truncate(str, max) {
  return typeof str === 'string' ? str.slice(0, max) : '';
}

function sendJson(res, status, obj) {
  res.writeHead(status, { 'Content-Type': 'application/json; charset=utf-8' });
  res.end(JSON.stringify(obj));
}

// ── Request handler ──────────────────────────────
function handleRequest(req, res) {
  // CORS headers
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'POST, OPTIONS');
  res.setHeader('Access-Control-Allow-Headers', 'Content-Type, X-Aura-Token');

  // Preflight
  if (req.method === 'OPTIONS') {
    res.writeHead(204);
    return res.end();
  }

  // Route: POST /aura-update/v1/crash-report
  if (req.url !== '/aura-update/v1/crash-report' || req.method !== 'POST') {
    return sendJson(res, 404, { error: 'Not found' });
  }

  // Validate token (constant-time comparison via Buffer)
  const token = req.headers['x-aura-token'] || '';
  const expected = Buffer.from(AURA_TOKEN, 'utf8');
  const received = Buffer.from(token, 'utf8');
  if (expected.length !== received.length || !require('crypto').timingSafeEqual(expected, received)) {
    return sendJson(res, 403, { error: 'Forbidden' });
  }

  // Read body
  const chunks = [];
  let size = 0;

  req.on('data', (chunk) => {
    size += chunk.length;
    if (size > MAX_BODY_SIZE) {
      req.destroy();
      return sendJson(res, 413, { error: 'Payload too large' });
    }
    chunks.push(chunk);
  });

  req.on('end', async () => {
    const raw = Buffer.concat(chunks).toString('utf8');
    if (!raw) return sendJson(res, 400, { error: 'Empty body' });

    let data;
    try {
      data = JSON.parse(raw);
    } catch {
      return sendJson(res, 400, { error: 'Invalid JSON' });
    }

    // Extract & sanitize fields
    const crashDataRaw = truncate(data.crash_data, 5000);
    const userMessage = escapeHtml(truncate(data.user_message, 2000));
    const os = escapeHtml(truncate(data.os || 'unknown', 50));
    const appVersion = escapeHtml(truncate(data.app_version || 'unknown', 20));
    const logTailRaw = truncate(data.log_tail, 10000);
    const date = new Date().toISOString().replace('T', ' ').slice(0, 19);
    const crashDataHtml = escapeHtml(crashDataRaw);
    const logTailHtml = escapeHtml(logTailRaw);

    // Build HTML email (summary only — full data in attachments)
    const subject = `[Aura Update] Crash Report — v${appVersion} / ${os}`;
    const crashPreview = crashDataHtml.length > 300 ? crashDataHtml.slice(0, 300) + '…' : crashDataHtml;
    const logPreview = logTailHtml.length > 300 ? logTailHtml.slice(0, 300) + '…' : logTailHtml;
    const html = `<!DOCTYPE html>
<html><head><meta charset="UTF-8"></head>
<body style="font-family: -apple-system, BlinkMacSystemFont, sans-serif; color: #222; max-width: 700px;">
<h2 style="color: #e74c3c;">⚠️ Aura Update — Crash Report</h2>
<table style="border-collapse: collapse; width: 100%;">
<tr><td style="padding:6px; font-weight:bold; color:#555;">Date</td><td style="padding:6px;">${date}</td></tr>
<tr><td style="padding:6px; font-weight:bold; color:#555;">App Version</td><td style="padding:6px;">${appVersion}</td></tr>
<tr><td style="padding:6px; font-weight:bold; color:#555;">OS</td><td style="padding:6px;">${os}</td></tr>
</table>
<h3 style="margin-top: 20px;">💬 Message utilisateur</h3>
<div style="background: #f7f7f7; padding: 12px; border-radius: 6px; white-space: pre-wrap;">${userMessage}</div>
<h3 style="margin-top: 20px;">🔴 Aperçu du crash</h3>
<pre style="background: #2d2d2d; color: #f8f8f2; padding: 12px; border-radius: 6px; overflow-x: auto; font-size: 13px;">${crashPreview}</pre>
<h3 style="margin-top: 20px;">📋 Aperçu des logs</h3>
<pre style="background: #2d2d2d; color: #a6e22e; padding: 12px; border-radius: 6px; overflow-x: auto; font-size: 12px;">${logPreview}</pre>
<hr style="margin-top: 30px; border: none; border-top: 1px solid #ddd;">
<p style="color: #888; font-size: 13px;">📎 <strong>Les fichiers complets sont en pièces jointes :</strong><br>
&nbsp;&nbsp;• <code>crash-report.txt</code> — Données complètes du crash<br>
&nbsp;&nbsp;• <code>app-logs.txt</code> — Dernières lignes du log applicatif</p>
<p style="color: #999; font-size: 12px;">Ce rapport a été généré automatiquement par Aura Update.<br>Aucune donnée personnelle n'est stockée sur le serveur.</p>
</body></html>`;

    // Build file attachments for debugging
    const crashFileContent = `=== AURA UPDATE — CRASH REPORT ===
Date: ${date}
App Version: ${appVersion}
OS: ${os}

=== MESSAGE UTILISATEUR ===
${truncate(data.user_message, 2000)}

=== DONNÉES DU CRASH ===
${crashDataRaw}
`;

    const logFileContent = `=== AURA UPDATE — APPLICATION LOGS ===
Date: ${date}
App Version: ${appVersion}
OS: ${os}

=== DERNIÈRES LIGNES DU LOG ===
${logTailRaw}
`;

    try {
      await transporter.sendMail({
        from: `Aura Update <${MAIL_FROM}>`,
        to: MAIL_TO,
        subject,
        html,
        attachments: [
          {
            filename: `crash-report-v${appVersion}-${date.replace(/[: ]/g, '-')}.txt`,
            content: crashFileContent,
            contentType: 'text/plain; charset=utf-8',
          },
          {
            filename: `app-logs-v${appVersion}-${date.replace(/[: ]/g, '-')}.txt`,
            content: logFileContent,
            contentType: 'text/plain; charset=utf-8',
          },
        ],
      });
      sendJson(res, 200, { success: true, message: 'Crash report received' });
    } catch (err) {
      console.error('[aura-update] Failed to send crash report email:', err.message);
      sendJson(res, 500, { error: 'Failed to send email' });
    }
  });
}

// ── Start server ─────────────────────────────────
const server = http.createServer(handleRequest);
server.listen(PORT, '127.0.0.1', () => {
  console.log(`[aura-update] Crash Report API listening on http://127.0.0.1:${PORT}`);
});
