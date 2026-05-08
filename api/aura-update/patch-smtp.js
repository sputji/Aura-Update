// Patch: add SMTP config to aura-update-backend env in ecosystem.config.js
const fs = require('fs');
const path = '/Users/admin/AuraServices/ecosystem.config.js';
let content = fs.readFileSync(path, 'utf8');

// Find the aura-update env block and add SMTP vars
const oldEnv = `AURA_UPDATE_TOKEN: 'aura_update_crash_2026',
        SMTP_HOST: 'localhost',
        SMTP_PORT: 25,`;

const newEnv = `AURA_UPDATE_TOKEN: 'aura_update_crash_2026',
        SMTP_HOST: 'ssl0.ovh.net',
        SMTP_PORT: 587,
        SMTP_USER: 'contact@auraneo.fr',
        SMTP_PASS: process.env.AURANEO_SMTP_PASS || 'CHANGE_ME',`;

if (!content.includes(oldEnv)) {
  console.log('ERROR: env block not found');
  process.exit(1);
}

content = content.replace(oldEnv, newEnv);
fs.writeFileSync(path, content, 'utf8');
console.log('OK: SMTP config updated in ecosystem.config.js');
