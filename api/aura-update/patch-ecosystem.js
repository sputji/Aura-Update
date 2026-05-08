// Patch script to update ecosystem.config.js
// Replaces the commented AURA-UPDATE section with an active one
const fs = require('fs');
const path = '/Users/admin/AuraServices/ecosystem.config.js';
let content = fs.readFileSync(path, 'utf8');

// Find the commented AURA-UPDATE section (lines 314-339 approx)
const regex = /\/\/\s*={3,}\s*\n\s*\/\/\s*.*AURA-UPDATE[\s\S]*?\/\/\s*\},/;
const match = content.match(regex);
if (!match) {
  console.log('ERROR: AURA-UPDATE section not found');
  process.exit(1);
}

console.log('Found section at index', match.index);

const newSection = `// ===========================
    // AURA-UPDATE CRASH API (8300)
    // ===========================
    {
      name: 'aura-update-backend',
      cwd: '/Users/admin/AuraServices/aura-update/backend',
      script: 'server.js',
      instances: 1,
      exec_mode: 'fork',
      autorestart: true,
      watch: false,
      max_memory_restart: '256M',
      restart_delay: 5000,
      max_restarts: 20,
      min_uptime: '10s',
      env: {
        NODE_ENV: 'production',
        PORT: 8300,
        AURA_UPDATE_TOKEN: 'aura_update_crash_2026',
        SMTP_HOST: 'localhost',
        SMTP_PORT: 25,
        MAIL_TO: 'contact@auraneo.fr',
        MAIL_FROM: 'noreply@auraneo.fr',
        NODE_OPTIONS: '--dns-result-order=ipv4first',
      },
      error_file: '/Users/admin/AuraServices/logs/aura-update-backend-error.log',
      out_file: '/Users/admin/AuraServices/logs/aura-update-backend-out.log',
      log_date_format: 'YYYY-MM-DD HH:mm:ss Z',
      merge_logs: true,
    },`;

content = content.replace(match[0], newSection);
fs.writeFileSync(path, content, 'utf8');
console.log('OK: ecosystem.config.js updated');
