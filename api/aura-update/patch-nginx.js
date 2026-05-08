// Patch nginx config: add /aura-update/ location block before the catch-all location /
const fs = require('fs');
const path = '/Users/admin/AuraServices/nginx/sites-enabled/api.auraneo.fr.conf';
let content = fs.readFileSync(path, 'utf8');

// Check if already patched
if (content.includes('/aura-update/')) {
  console.log('SKIP: /aura-update/ location already exists');
  process.exit(0);
}

const auraUpdateBlock = `
    # ── Aura Update Crash Report API (port 8300) ──
    location /aura-update/ {
        proxy_pass http://127.0.0.1:8300;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_connect_timeout 10s;
        proxy_send_timeout 10s;
        proxy_read_timeout 10s;
        client_max_body_size 64k;
    }

`;

// Insert before "# Proxy to API Gateway" or "location / {"
const insertPoint = content.indexOf('    # Proxy to API Gateway');
if (insertPoint === -1) {
  console.log('ERROR: Could not find insertion point');
  process.exit(1);
}

content = content.slice(0, insertPoint) + auraUpdateBlock + content.slice(insertPoint);
fs.writeFileSync(path, content, 'utf8');
console.log('OK: nginx api.auraneo.fr.conf updated with /aura-update/ block');
