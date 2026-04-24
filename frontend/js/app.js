/* ═══════════════════════════════════════════════════════════
   Aura Update — Health Center  |  v2.0  |  Frontend JS
   ═══════════════════════════════════════════════════════════ */
'use strict';

/* ─── Tauri API ────────────────────────────────────────────── */
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const STRICT_PRIVACY_MODE = true;

/* ─── State ────────────────────────────────────────────────── */
const state = {
    config: null,
    i18n: {},
    platform: 'windows',
    updates: [],
    cleanup: { items: [], total_bytes: 0 },
    residues: { items: [], total_bytes: 0 },
    browserGranular: [],
    startupItems: [],
    processes: [],
    health: null,
    busy: false,
    turboActive: false,
    bloatwareCache: { items: null, scannedAt: 0 },
    appUpdateInfo: null,
    appUpdateInstalling: false,
};

/* ─── DOM cache ────────────────────────────────────────────── */
const $ = (s) => document.querySelector(s);
const $$ = (s) => document.querySelectorAll(s);

function isLocalEndpoint(url) {
    if (!url) return false;
    const v = String(url).toLowerCase();
    return v.includes('localhost') || v.includes('127.0.0.1');
}

/* ─── AI Provider Presets ──────────────────────────────────── */
const AI_PRESETS = {
    gemini:  { endpoint: 'https://generativelanguage.googleapis.com/v1beta/openai/chat/completions', model: 'gemini-2.5-flash', needsKey: true  },
    openai:  { endpoint: 'https://api.openai.com',  model: 'gpt-4o-mini',         needsKey: true  },
    grok:    { endpoint: 'https://api.x.ai',         model: 'grok-4-1-fast-non-reasoning', needsKey: true  },
    ollama:  { endpoint: 'http://localhost:11434',    model: 'llama3',              needsKey: false },
    auraneo: { endpoint: 'https://ia.auraneo.fr',    model: 'rapide',              needsKey: true  },
};

/* ─── AI Model Loader ──────────────────────────────────────── */
let _modelsLoading = false;
async function loadAIModels(providerKey, forceRefresh = false) {
    const sel = $('#aiModelSelect');
    if (!sel) return;

    const preset = AI_PRESETS[providerKey];
    const endpoint = preset ? preset.endpoint : ($('#aiEndpointInput')?.value || '').trim();
    const apiKey = ($('#aiApiKey')?.value || '').trim();

    if (STRICT_PRIVACY_MODE && !isLocalEndpoint(endpoint)) {
        sel.innerHTML = '<option value="">' + (t('ai_modele_local_only') || 'Mode strict: IA locale uniquement') + '</option>';
        return;
    }

    if (!endpoint && providerKey === 'custom') {
        sel.innerHTML = '<option value="">' + (t('ai_model_custom_hint') || '-- Saisissez un endpoint --') + '</option>';
        return;
    }

    // Show loading state
    _modelsLoading = true;
    const refreshBtn = $('#btnRefreshModels');
    if (refreshBtn) refreshBtn.classList.add('spinning');
    sel.disabled = true;
    const prevVal = sel.value || (preset ? preset.model : '') || (state.config?.ai_model || '');
    sel.innerHTML = '<option value="">' + (t('ai_loading_models') || 'Chargement...') + '</option>';

    try {
        const models = await invoke('list_ai_models', {
            provider: providerKey || 'custom',
            endpoint: endpoint,
            apiKey: apiKey,
        });

        sel.innerHTML = '';
        if (models.length === 0) {
            sel.innerHTML = '<option value="">' + (t('ai_no_models') || 'Aucun modèle trouvé') + '</option>';
        } else {
            for (const m of models) {
                const opt = document.createElement('option');
                opt.value = m.id;
                opt.textContent = m.name || m.id;
                sel.appendChild(opt);
            }
            // Re-select previous value or preset default
            if (prevVal && [...sel.options].some(o => o.value === prevVal)) {
                sel.value = prevVal;
            } else if (preset && [...sel.options].some(o => o.value === preset.model)) {
                sel.value = preset.model;
            }
        }
    } catch (e) {
        console.warn('Failed to load models:', e);
        sel.innerHTML = '';
        // Fallback: use preset model as only option
        if (preset) {
            const opt = document.createElement('option');
            opt.value = preset.model;
            opt.textContent = preset.model;
            sel.appendChild(opt);
            sel.value = preset.model;
        } else {
            sel.innerHTML = '<option value="">' + (t('ai_model_error') || 'Erreur de chargement') + '</option>';
        }
    } finally {
        sel.disabled = false;
        _modelsLoading = false;
        if (refreshBtn) refreshBtn.classList.remove('spinning');
    }
}

/* ─── Helpers ──────────────────────────────────────────────── */
function t(key) { return state.i18n[key] || key; }

function formatBytes(bytes) {
    if (bytes < 1024) return bytes + ' B';
    if (bytes < 1048576) return (bytes / 1024).toFixed(1) + ' KB';
    if (bytes < 1073741824) return (bytes / 1048576).toFixed(1) + ' MB';
    return (bytes / 1073741824).toFixed(2) + ' GB';
}

function showToast(msg, type = '') {
    const el = document.createElement('div');
    el.className = 'toast ' + type;
    el.textContent = msg;
    document.body.appendChild(el);
    setTimeout(() => el.remove(), 4000);
}

function setBusy(v) {
    state.busy = v;
    $('#btnAutoPilot').disabled = v;
}

/* ─── Particle System Evolution ─────────────────────────────── */
class ParticleSystem {
    constructor(canvasId) {
        this.canvas = document.getElementById(canvasId);
        if (!this.canvas) return;
        this.ctx = this.canvas.getContext('2d');
        this.particles = [];
        this.scanning = false;
        // Couleurs par défaut (Bleu Aura)
        this.currentColor = { r: 88, g: 166, b: 255 };
        this.targetColor  = { r: 88, g: 166, b: 255 };
        this.running = true;
        this.resize();
        window.addEventListener('resize', () => this.resize());
        this._initParticles(15);
        this.update();
    }

    resize() {
        if (!this.canvas) return;
        this.canvas.width = window.innerWidth;
        this.canvas.height = window.innerHeight;
    }

    _initParticles(count) {
        const W = this.canvas.width || window.innerWidth;
        const H = this.canvas.height || window.innerHeight;
        for (let i = 0; i < count; i++) {
            this.particles.push(this._createParticle(W, H));
        }
    }

    _createParticle(W, H) {
        const angle = Math.random() * Math.PI * 2;
        const speed = 0.3 + Math.random() * 0.5;
        return {
            x: Math.random() * W,
            y: Math.random() * H,
            vx: Math.cos(angle) * speed,
            vy: Math.sin(angle) * speed,
            size: 1.5 + Math.random() * 2,
        };
    }

    setThemeColor(tabName) {
        const colors = {
            'updates':   { r: 88,  g: 166, b: 255 }, // Bleu
            'cleanup':   { r: 210, g: 153, b: 34  }, // Jaune
            'turbo':     { r: 188, g: 140, b: 255 }, // Violet
            'startup':   { r: 255, g: 200, b: 60  }, // Gold
            'processes': { r: 255, g: 100, b: 100 }, // Rouge
            'default':   { r: 139, g: 148, b: 158 }, // Gris
        };
        this.targetColor = colors[tabName] || colors['default'];
    }

    setScanning(active) {
        this.scanning = active;
    }

    update() {
        if (!this.running || !this.ctx) return;
        const ctx = this.ctx;
        const W = this.canvas.width;
        const H = this.canvas.height;

        ctx.clearRect(0, 0, W, H);

        // Transition fluide de la couleur (Interpolation)
        this.currentColor.r += (this.targetColor.r - this.currentColor.r) * 0.1;
        this.currentColor.g += (this.targetColor.g - this.currentColor.g) * 0.1;
        this.currentColor.b += (this.targetColor.b - this.currentColor.b) * 0.1;

        const colorStr = `rgb(${Math.trunc(this.currentColor.r)}, ${Math.trunc(this.currentColor.g)}, ${Math.trunc(this.currentColor.b)})`;
        const speedMultiplier = this.scanning ? 6 : 1;

        // Maintain 15 particles
        while (this.particles.length < 15) {
            this.particles.push(this._createParticle(W, H));
        }

        this.particles.forEach(p => {
            p.x += p.vx * speedMultiplier;
            p.y += p.vy * speedMultiplier;
            if (p.x < 0 || p.x > W) p.vx *= -1;
            if (p.y < 0 || p.y > H) p.vy *= -1;

            ctx.beginPath();
            ctx.arc(p.x, p.y, p.size, 0, Math.PI * 2);
            ctx.fillStyle = colorStr;
            ctx.fill();
            // Effet de halo léger en mode scanning
            if (this.scanning) {
                ctx.shadowBlur = 10;
                ctx.shadowColor = colorStr;
            } else {
                ctx.shadowBlur = 0;
            }
        });

        requestAnimationFrame(() => this.update());
    }

    destroy() {
        this.running = false;
    }
}

let auraParticles = null;

/* ─── i18n ─────────────────────────────────────────────────── */
async function loadTranslations(lang) {
    try {
        state.i18n = await invoke('get_translations', { lang });
        applyTranslations();
        // Re-render dynamic JS content that uses t()
        if (state.health) renderHealthScore(state.health);
    } catch (e) { console.warn('i18n load failed', e); }
}

function applyTranslations() {
    $$('[data-i18n]').forEach(el => {
        const key = el.getAttribute('data-i18n');
        if (state.i18n[key]) el.textContent = state.i18n[key];
    });
    $$('[data-i18n-title]').forEach(el => {
        const key = el.getAttribute('data-i18n-title');
        if (state.i18n[key]) el.title = state.i18n[key];
    });
    $$('[data-i18n-placeholder]').forEach(el => {
        const key = el.getAttribute('data-i18n-placeholder');
        if (state.i18n[key]) el.placeholder = state.i18n[key];
    });
}

/* ─── Theme ────────────────────────────────────────────────── */
function setTheme(theme) {
    let resolved = theme;
    if (theme === 'system') {
        resolved = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
    }
    document.documentElement.setAttribute('data-theme', resolved);
    $('#themeIcon').textContent = resolved === 'dark' ? '🌙' : '☀️';
    highlightOption('[data-theme-choice]', theme);
}

/* ─── Health Score ─────────────────────────────────────────── */
function renderHealthScore(hs) {
    state.health = hs;

    const scoreUpdates = $('#scoreUpdates');
    const scoreDisk = $('#scoreDisk');
    const scoreStartup = $('#scoreStartup');
    const scoreTemp = $('#scoreTemp');

    if (scoreUpdates) scoreUpdates.textContent = `${hs.update_score}/40`;
    if (scoreDisk) scoreDisk.textContent = `${hs.disk_score}/20`;
    if (scoreStartup) scoreStartup.textContent = `${hs.startup_score}/20`;
    if (scoreTemp) scoreTemp.textContent = `${hs.temp_score}/20`;

    // Health Circle SVG
    const total = hs.total ?? (hs.update_score + hs.disk_score + hs.startup_score + hs.temp_score);
    const circle = $('#healthCircleProgress');
    const scoreText = $('#healthCircleScore');
    const glow = $('#healthCircleGlow');
    if (circle && scoreText) {
        const circumference = 2 * Math.PI * 52; // ~326.73
        const dashLen = (total / 100) * circumference;
        circle.setAttribute('stroke-dasharray', `${dashLen} ${circumference}`);
        scoreText.textContent = total;

        // Color coding
        const colorClass = total < 40 ? 'red' : total <= 70 ? 'yellow' : 'green';
        circle.classList.remove('red', 'yellow', 'green');
        circle.classList.add(colorClass);
        if (glow) {
            glow.classList.remove('red', 'yellow', 'green');
            glow.classList.add(colorClass);
        }
    }
}

async function refreshHealth() {
    try {
        const hs = await invoke('get_health_score');
        renderHealthScore(hs);
    } catch (e) { console.warn('Health score error', e); }
}

/// Apply vitals data to the dashboard UI.
function displayVitals(v) {
    const cpuEl = $('#valCpuTemp');
    if (cpuEl) {
        if (v.cpu_temp != null) {
            cpuEl.textContent = v.cpu_temp.toFixed(0) + '°C';
        } else {
            cpuEl.textContent = t('vital_na');
        }
    }
    const gpuEl = $('#valGpuTemp');
    if (gpuEl) {
        if (v.gpu_temp != null) {
            gpuEl.textContent = v.gpu_temp.toFixed(0) + '°C';
        } else {
            gpuEl.textContent = t('vital_na');
        }
    }
}

async function refreshVitals() {
    try {
        const v = await invoke('get_system_vitals');
        displayVitals(v);
    } catch (e) { console.warn('Vitals error', e); }
    // Auto-update health circle
    refreshHealth();
}

function tempClass(temp) {
    if (temp >= 85) return 'hot';
    if (temp >= 65) return 'warm';
    return 'cool';
}

function batteryIcon(v) {
    if (v.battery_charging) return '⚡';
    if (v.battery_percent <= 20) return '🪫';
    return '🔋';
}

/* ─── Cool Boost ───────────────────────────────────────────── */
let coolBoostInterval = null;
let cooldownInterval = null;

async function runCoolBoost() {
    const container = $('#coolingContainer');
    const btn = $('#btnCoolBoost');
    const statusText = $('#coolStatusText');
    const timer = $('#coolTimer');

    btn.disabled = true;
    container.classList.remove('cooldown', 'cracking');
    container.classList.add('active');
    statusText.textContent = t('cool_active');

    try {
        const result = await invoke('set_fan_boost', { active: true });

        // Display detailed log if available
        if (result.log && result.log.length > 0) {
            let logEl = $('#coolLog');
            if (!logEl) {
                logEl = document.createElement('div');
                logEl.id = 'coolLog';
                logEl.className = 'cool-log';
                container.appendChild(logEl);
            }
            logEl.textContent = result.log.join('\n');
        }

        if (!result.success) {
            showToast(t(result.message) || t('cool_error'), 'error');
            resetCoolingUI();
            return;
        }

        // Show success status
        statusText.textContent = t(result.message) || t('cool_boost_started');
        showToast(t(result.message) || t('cool_boost_started'), 'success');
    } catch (e) {
        showToast(t('cool_error'), 'error');
        resetCoolingUI();
        return;
    }

    // 30s countdown
    let remaining = 30;
    timer.textContent = remaining + 's';
    coolBoostInterval = setInterval(() => {
        remaining--;
        timer.textContent = remaining + 's';
        if (remaining <= 0) {
            clearInterval(coolBoostInterval);
            coolBoostInterval = null;
            finishCoolBoost();
        }
    }, 1000);
}

async function finishCoolBoost() {
    const container = $('#coolingContainer');
    const statusText = $('#coolStatusText');
    const timer = $('#coolTimer');
    const btn = $('#btnCoolBoost');

    // Ice-crack animation
    container.classList.remove('active');
    container.classList.add('cracking');

    try {
        await invoke('set_fan_boost', { active: false });
    } catch (_) { /* best-effort restore */ }

    // After crack animation, start 60s cooldown
    setTimeout(() => {
        container.classList.remove('cracking');
        container.classList.add('cooldown');
        statusText.textContent = t('cool_cooldown');

        let cd = 60;
        timer.textContent = cd + 's';
        cooldownInterval = setInterval(() => {
            cd--;
            timer.textContent = cd + 's';
            if (cd <= 0) {
                clearInterval(cooldownInterval);
                cooldownInterval = null;
                resetCoolingUI();
            }
        }, 1000);
    }, 600);
}

function resetCoolingUI() {
    const container = $('#coolingContainer');
    const btn = $('#btnCoolBoost');
    const statusText = $('#coolStatusText');
    const timer = $('#coolTimer');

    container.classList.remove('active', 'cracking', 'cooldown');
    btn.disabled = false;
    statusText.textContent = t('cool_ready');
    timer.textContent = '';

    // Clear log
    const logEl = $('#coolLog');
    if (logEl) logEl.remove();
}

/* ─── Tab Navigation ───────────────────────────────────────── */
function switchTab(name) {
    $$('.tab').forEach(t => t.classList.toggle('active', t.dataset.tab === name));
    $$('.tab-content').forEach(c => c.classList.toggle('active', c.id === 'tab-' + name));
    if (auraParticles) auraParticles.setThemeColor(name);
    if (name === 'turbo') {
        loadPredictiveGain();
        filterInstalledBrowsers();
    }
}

/* ─── Browser Detection ───────────────────────────────────── */
async function filterInstalledBrowsers() {
    try {
        const installed = await invoke('detect_installed_browsers');
        const rows = document.querySelectorAll('.browser-filter-row');
        let visibleCount = 0;
        rows.forEach(row => {
            const key = row.dataset.browser;
            if (installed.includes(key)) {
                row.style.display = '';
                visibleCount++;
            } else {
                row.style.display = 'none';
                // Uncheck all filters for hidden browsers
                row.querySelectorAll('input[type="checkbox"]').forEach(cb => cb.checked = false);
            }
        });
        // Show message if no browsers detected
        const grid = $('#browserGranularGrid');
        let noMsg = grid?.querySelector('.no-browser-msg');
        if (visibleCount === 0) {
            if (!noMsg) {
                noMsg = document.createElement('p');
                noMsg.className = 'no-browser-msg';
                noMsg.textContent = t('no_browsers_detected') || 'Aucun navigateur détecté';
                noMsg.style.cssText = 'text-align:center;opacity:0.6;padding:1rem 0';
                grid?.appendChild(noMsg);
            }
        } else if (noMsg) {
            noMsg.remove();
        }
    } catch (e) {
        console.warn('Browser detection failed:', e);
    }
}

/* ─── Updates Tab ──────────────────────────────────────────── */
async function scanUpdates() {
    if (STRICT_PRIVACY_MODE) {
        state.updates = [];
        renderUpdateList();
        showToast(t('privacy_updates_disabled') || 'Mode confidentialité stricte: mises à jour réseau désactivées', 'warning');
        return;
    }

    if (state.busy) return;
    setBusy(true);

    // Show scanning state
    const status = $('#statusSection');
    status.classList.remove('hidden');
    $('#updateList').classList.add('hidden');
    $('#statusIcon').textContent = '🔄';
    $('#statusTitle').textContent = t('scanning');
    $('#statusDesc').textContent = t('scanning_desc');
    $('#btnScan').disabled = true;

    try {
        state.updates = await Promise.race([
            invoke('check_updates'),
            new Promise((_, reject) => setTimeout(() => reject('timeout'), 130000))
        ]);
        renderUpdateList();
    } catch (e) {
        if (e === 'timeout') {
            showToast(t('error_timeout_updates') || 'La recherche de mises à jour a expiré. Réessayez.', 'error');
            state.updates = [];
            renderUpdateList();
        } else {
            showToast(t('error_scan') + ': ' + e, 'error');
        }
    } finally {
        setBusy(false);
        $('#btnScan').disabled = false;
    }
}

function renderUpdateList() {
    const list = $('#updateList');
    const status = $('#statusSection');
    const btnAll = $('#btnUpdateAll');
    const countEl = $('#updateCount');
    const tabCount = $('#tabUpdatesCount');

    if (state.updates.length === 0) {
        list.classList.add('hidden');
        btnAll.classList.add('hidden');
        countEl.classList.add('hidden');
        tabCount.classList.add('hidden');
        status.classList.remove('hidden');
        $('#statusIcon').textContent = '✅';
        $('#statusTitle').textContent = t('up_to_date');
        $('#statusDesc').textContent = t('up_to_date_desc');
        return;
    }

    status.classList.add('hidden');
    list.classList.remove('hidden');
    btnAll.classList.remove('hidden');
    countEl.classList.remove('hidden');
    tabCount.classList.remove('hidden');
    countEl.textContent = state.updates.length + ' ' + t('updates_available');
    tabCount.textContent = state.updates.length;

    list.innerHTML = state.updates.map(pkg => {
        const badgeClass = pkg.type === 'critical' ? 'badge-critical' : pkg.type === 'system' ? 'badge-system' : 'badge-app';
        const icon = pkg.type === 'critical' ? '🔴' : pkg.type === 'system' ? '🟡' : '🔵';
        return `
        <div class="update-item" data-id="${escapeHtml(pkg.id)}">
            <div class="update-icon">${icon}</div>
            <div class="update-info">
                <div class="update-name">${escapeHtml(pkg.name)}</div>
                <div class="update-meta">
                    <span>${escapeHtml(pkg.current_version)} → ${escapeHtml(pkg.new_version)}</span>
                    <span class="update-badge ${badgeClass}">${escapeHtml(pkg.manager)}</span>
                </div>
            </div>
            <div class="update-actions">
                <button class="btn btn-sm btn-primary btn-install" data-id="${escapeHtml(pkg.id)}">${t('btn_install')}</button>
                <button class="btn btn-sm btn-secondary btn-ai-help" data-context="${escapeHtml(pkg.name + ' ' + pkg.new_version)}" title="AI Help">🤖</button>
            </div>
        </div>`;
    }).join('');
}

function escapeHtml(s) {
    const d = document.createElement('div');
    d.textContent = s;
    return d.innerHTML;
}

async function installUpdate(id) {
    const pkg = state.updates.find(u => u.id === id);
    if (!pkg) return;

    const item = $(`.update-item[data-id="${CSS.escape(id)}"]`);
    if (item) {
        const btn = item.querySelector('.btn-install');
        if (btn) { btn.disabled = true; btn.textContent = '⏳'; }
    }

    try {
        await invoke('install_update', { pkg });
        if (item) { item.classList.add('installed'); }
        state.updates = state.updates.filter(u => u.id !== id);
        showToast(pkg.name + ' ' + t('installed'), 'success');
        // Update tab count
        const tabCount = $('#tabUpdatesCount');
        tabCount.textContent = state.updates.length;
        if (state.updates.length === 0) tabCount.classList.add('hidden');
    } catch (e) {
        showToast(t('error_install') + ': ' + e, 'error');
        if (item) {
            const btn = item.querySelector('.btn-install');
            if (btn) { btn.disabled = false; btn.textContent = t('btn_install'); }
        }
    }
}

async function installAllUpdates() {
    if (state.busy) return;
    setBusy(true);
    showProgress(t('installing_all'), state.updates.length);

    let installed = 0;
    for (const pkg of [...state.updates]) {
        updateProgress(pkg.name, installed, state.updates.length + installed);
        try {
            await invoke('install_update', { pkg });
            installed++;
            const item = $(`.update-item[data-id="${CSS.escape(pkg.id)}"]`);
            if (item) item.classList.add('installed');
        } catch (e) { console.warn('Failed:', pkg.name, e); }
    }

    hideProgress();
    state.updates = [];
    renderUpdateList();
    showToast(installed + ' ' + t('updates_installed'), 'success');
    refreshHealth();
    setBusy(false);
}

/* ─── Progress helpers ─────────────────────────────────────── */
function showProgress(label, total) {
    const s = $('#progressSection');
    s.classList.remove('hidden');
    $('#progressLabel').textContent = label;
    $('#progressCount').textContent = total ? `0/${total}` : '';
    const bar = $('#progressBar');
    bar.classList.remove('indeterminate');
    bar.style.width = '0%';
    $('#progressMessage').textContent = '';
}

function updateProgress(message, current, total) {
    if (total) {
        $('#progressCount').textContent = `${current}/${total}`;
        $('#progressBar').style.width = Math.round((current / total) * 100) + '%';
    }
    $('#progressMessage').textContent = message;
}

function hideProgress() {
    $('#progressSection').classList.add('hidden');
}

/* ─── Cleanup Tab ──────────────────────────────────────────── */
async function scanCleanup() {
    if (state.busy) return;
    setBusy(true);
    $('#btnScanClean').disabled = true;
    if (auraParticles) auraParticles.setScanning(true);

    try {
        state.cleanup = await invoke('scan_cleanup');
        renderCleanupList();
    } catch (e) {
        showToast(t('error_scan') + ': ' + e, 'error');
    } finally {
        if (auraParticles) auraParticles.setScanning(false);
        setBusy(false);
        $('#btnScanClean').disabled = false;
    }
}

function renderCleanupList() {
    const list = $('#cleanupList');
    const status = $('#cleanupStatus');
    const btnClean = $('#btnRunClean');

    if (state.cleanup.items.length === 0) {
        list.innerHTML = '';
        btnClean.classList.add('hidden');
        status.classList.remove('hidden');
        return;
    }

    status.classList.add('hidden');
    btnClean.classList.remove('hidden');

    let html = `<div class="cleanup-summary">
        <span class="cleanup-summary-text">
            ${state.cleanup.items.length} ${t('items_found')} — ${formatBytes(state.cleanup.total_bytes)} ${t('recoverable')}
        </span>
    </div>`;

    html += `<div class="cleanup-summary" style="margin-top:8px;display:flex;gap:8px;align-items:center;justify-content:space-between;flex-wrap:wrap;">
        <span class="cleanup-summary-text" id="cleanupSelectedInfo">${t('selected') || 'Sélection'}: ${state.cleanup.items.length}/${state.cleanup.items.length}</span>
        <div style="display:flex;gap:8px;flex-wrap:wrap;">
            <button id="btnCleanupSelectAll" class="btn btn-sm btn-secondary">${t('btn_select_all')}</button>
            <button id="btnCleanupDeselectAll" class="btn btn-sm btn-secondary">${t('btn_deselect_all')}</button>
        </div>
    </div>`;

    html += state.cleanup.items.map(item => `
        <div class="cleanup-item">
            <label style="display:flex;align-items:center;gap:8px;cursor:pointer">
                <input type="checkbox" checked data-clean-path="${escapeHtml(item.path)}">
                <span class="cleanup-item-icon">${item.category === 'temp' ? '🗑️' : item.category === 'cache' ? '📦' : '📂'}</span>
            </label>
            <div class="cleanup-item-info">
                <div class="cleanup-item-name">${escapeHtml(item.description)}</div>
                <div class="cleanup-item-path">${escapeHtml(item.path)}</div>
            </div>
            <span class="cleanup-item-size">${formatBytes(item.size_bytes)}</span>
        </div>`).join('');

    list.innerHTML = html;

    const refreshSelectedInfo = () => {
        const checked = Array.from(list.querySelectorAll('input[data-clean-path]:checked'));
        const selectedPaths = new Set(checked.map(cb => cb.dataset.cleanPath));
        const selectedBytes = state.cleanup.items
            .filter(i => selectedPaths.has(i.path))
            .reduce((sum, i) => sum + (i.size_bytes || 0), 0);
        const info = $('#cleanupSelectedInfo');
        if (info) info.textContent = `${t('selected') || 'Sélection'}: ${checked.length}/${state.cleanup.items.length} — ${formatBytes(selectedBytes)}`;
    };

    const btnSelectAll = $('#btnCleanupSelectAll');
    if (btnSelectAll) {
        btnSelectAll.addEventListener('click', () => {
            list.querySelectorAll('input[data-clean-path]').forEach(cb => { cb.checked = true; });
            refreshSelectedInfo();
        });
    }

    const btnDeselectAll = $('#btnCleanupDeselectAll');
    if (btnDeselectAll) {
        btnDeselectAll.addEventListener('click', () => {
            list.querySelectorAll('input[data-clean-path]').forEach(cb => { cb.checked = false; });
            refreshSelectedInfo();
        });
    }

    list.querySelectorAll('input[data-clean-path]').forEach(cb => {
        cb.addEventListener('change', refreshSelectedInfo);
    });
    refreshSelectedInfo();
}

async function runCleanup() {
    if (state.busy || state.cleanup.items.length === 0) return;
    setBusy(true);

    // Auto-snapshot before heavy cleanup
    await safeSnapshot('Nettoyage — Aura Update');

    try {
        const selected = Array.from($$('#cleanupList input[data-clean-path]:checked'));
        const paths = selected.length > 0
            ? selected.map(i => i.dataset.cleanPath)
            : state.cleanup.items.map(i => i.path);
        if (paths.length === 0) {
            showToast(t('no_selection'), 'error');
            return;
        }
        const freed = await invoke('run_cleanup', { paths });
        showToast(formatBytes(freed) + ' ' + t('freed'), 'success');
        state.cleanup = { items: [], total_bytes: 0 };
        renderCleanupList();
        refreshHealth();
    } catch (e) {
        showToast(t('error_clean') + ': ' + e, 'error');
    } finally { setBusy(false); }
}

async function scanResidues() {
    if (state.busy) return;
    setBusy(true);
    $('#btnScanResidues').disabled = true;

    try {
        state.residues = await invoke('scan_os_residues');
        if (state.residues.items.length === 0) {
            showToast(t('no_residues'), 'success');
        } else {
            const names = state.residues.items.map(i => i.description).join(', ');
            if (confirm(t('confirm_clean_residues') + '\n\n' + names)) {
                const residues = state.residues.items.map(i => i.description);
                const msg = await invoke('clean_os_residues', { residues });
                showToast(msg, 'success');
                refreshHealth();
            }
        }
    } catch (e) {
        showToast(t('error_scan') + ': ' + e, 'error');
    } finally {
        setBusy(false);
        $('#btnScanResidues').disabled = false;
    }
}

/* ─── Browser Granular Cleanup ────────────────────────────── */
function buildBrowserFilters() {
    const rows = document.querySelectorAll('.browser-filter-row');
    const filters = [];
    rows.forEach(row => {
        const browser = row.dataset.browser;
        const cacheEl = row.querySelector('[data-filter="cache"]');
        const histEl  = row.querySelector('[data-filter="history"]');
        const cookEl  = row.querySelector('[data-filter="cookies"]');
        const sessEl  = row.querySelector('[data-filter="sessions"]');
        const cache    = cacheEl ? cacheEl.checked : false;
        const history  = histEl  ? histEl.checked  : false;
        const cookies  = cookEl  ? cookEl.checked  : false;
        const sessions = sessEl  ? sessEl.checked  : false;
        if (cache || history || cookies || sessions) {
            filters.push({ browser, cache, history, cookies, sessions });
        }
    });
    return filters;
}

async function scanBrowserGranular() {
    const filters = buildBrowserFilters();
    if (filters.length === 0) {
        showToast(t('browser_no_selection') || 'Sélectionnez au moins un navigateur', 'warning');
        return;
    }
    if (state.busy) return;
    setBusy(true);
    const btn = $('#btnScanBrowserGranular');
    if (btn) btn.disabled = true;

    try {
        const raw = await invoke('scan_browser_granular', { filters });
        // Robust: handle both CleanupReport object and raw array
        const items = Array.isArray(raw) ? raw : (raw && Array.isArray(raw.items) ? raw.items : []);
        state.browserGranular = items;
        renderBrowserGranularResults(items);
    } catch (e) {
        showToast(t('error_scan') + ': ' + e, 'error');
    } finally {
        setBusy(false);
        if (btn) btn.disabled = false;
    }
}

function renderBrowserGranularResults(results) {
    const container = $('#browserGranularResults');
    const btnClean = $('#btnCleanBrowserGranular');
    if (!container) return;

    // Ensure results is always an array
    const items = Array.isArray(results) ? results : (results && Array.isArray(results.items) ? results.items : []);

    if (items.length === 0) {
        container.innerHTML = `<div class="browser-empty-state">
            <span class="browser-empty-icon">✨</span>
            <p>${t('browser_nothing_found') || 'Aucun fichier trouvé'}</p>
        </div>`;
        if (btnClean) btnClean.classList.add('hidden');
        return;
    }

    if (btnClean) btnClean.classList.remove('hidden');
    const totalBytes = items.reduce((s, r) => s + (r.size_bytes || 0), 0);

    let html = `<div class="browser-results-summary">
        <div class="browser-results-badge">${items.length}</div>
        <div class="browser-results-info">
            <span class="browser-results-count">${items.length} ${t('items_found') || 'éléments trouvés'}</span>
            <span class="browser-results-size">${formatBytes(totalBytes)} ${t('recoverable') || 'récupérables'}</span>
        </div>
    </div>`;

    html += '<div class="browser-results-list">';
    html += items.map(item => {
        const icon = item.category?.includes('chrome') ? '🟡' :
                     item.category?.includes('edge') ? '🔵' :
                     item.category?.includes('firefox') ? '🟠' :
                     item.category?.includes('brave') ? '🟤' :
                     item.category?.includes('opera') ? '🔴' : '🌐';
        return `<div class="browser-result-item">
            <span class="browser-result-icon">${icon}</span>
            <div class="browser-result-info">
                <span class="browser-result-name">${escapeHtml(item.description)}</span>
                <span class="browser-result-path">${escapeHtml(item.path)}</span>
            </div>
            <span class="browser-result-size">${formatBytes(item.size_bytes)}</span>
        </div>`;
    }).join('');
    html += '</div>';

    container.innerHTML = html;
}

async function cleanBrowserGranular() {
    if (state.busy || !state.browserGranular || state.browserGranular.length === 0) return;
    setBusy(true);

    await safeSnapshot('Nettoyage navigateurs — Aura Update');

    // Kill only selected browsers to unlock cache files
    const filters = buildBrowserFilters();
    const selectedBrowsers = filters.map(f => f.browser);
    try { await invoke('kill_browser_processes', { browsers: selectedBrowsers }); } catch (_) { /* non-blocking */ }

    try {
        const paths = state.browserGranular.map(i => i.path);
        const freed = await invoke('run_cleanup', { paths });
        showToast(formatBytes(freed) + ' ' + t('freed'), 'success');
        state.browserGranular = [];
        renderBrowserGranularResults([]);
        refreshHealth();
    } catch (e) {
        showToast(t('error_clean') + ': ' + e, 'error');
    } finally {
        setBusy(false);
    }
}

/* ─── Startup Tab ──────────────────────────────────────────── */
async function loadStartupItems() {
    if (state.busy) return;
    setBusy(true);
    $('#btnLoadStartup').disabled = true;

    try {
        state.startupItems = await invoke('get_startup_items');
        renderStartupList();
    } catch (e) {
        showToast(t('error_load') + ': ' + e, 'error');
    } finally {
        setBusy(false);
        $('#btnLoadStartup').disabled = false;
    }
}

function renderStartupList() {
    const list = $('#startupList');
    const status = $('#startupStatus');

    if (state.startupItems.length === 0) {
        list.innerHTML = '';
        status.classList.remove('hidden');
        return;
    }

    status.classList.add('hidden');
    list.innerHTML = state.startupItems.map(item => `
        <div class="startup-item" data-name="${escapeHtml(item.name)}">
            <span class="startup-item-icon">${item.enabled ? '✅' : '⛔'}</span>
            <div class="startup-item-info">
                <div class="startup-item-name">${escapeHtml(item.name)}</div>
                <div class="startup-item-source">${escapeHtml(item.source)} — ${escapeHtml(item.path)}</div>
            </div>
            <label class="toggle">
                <input type="checkbox" ${item.enabled ? 'checked' : ''} data-startup="${escapeHtml(item.name)}">
                <span class="toggle-slider"></span>
            </label>
        </div>`).join('');
}

async function toggleStartup(name, enabled) {
    const item = state.startupItems.find(i => i.name === name);
    const source = item ? item.source : 'registry';
    try {
        await invoke('toggle_startup_item', { name, enabled, source });
        const item = state.startupItems.find(i => i.name === name);
        if (item) item.enabled = enabled;
        renderStartupList();
        showToast(name + ' ' + (enabled ? t('enabled') : t('disabled')), 'success');
    } catch (e) {
        showToast(t('error_toggle') + ': ' + e, 'error');
        // Revert visual
        renderStartupList();
    }
}

/* ─── Processes Tab ────────────────────────────────────────── */
async function loadProcesses() {
    if (state.busy) return;
    setBusy(true);
    $('#btnLoadProcs').disabled = true;

    try {
        state.processes = await invoke('get_heavy_processes');
        renderProcessList();
    } catch (e) {
        showToast(t('error_load') + ': ' + e, 'error');
    } finally {
        setBusy(false);
        $('#btnLoadProcs').disabled = false;
    }
}

function renderProcessList() {
    const list = $('#processList');
    const status = $('#processStatus');

    if (state.processes.length === 0) {
        list.innerHTML = '';
        status.classList.remove('hidden');
        return;
    }

    status.classList.add('hidden');
    list.innerHTML = state.processes.map(p => {
        const cpuPct = Math.min(p.cpu_percent, 100);
        const ramPct = Math.min((p.memory_mb / 2048) * 100, 100); // scale to 2GB
        const cpuClass = cpuPct > 50 ? 'high' : '';
        const ramClass = ramPct > 50 ? 'high' : '';
        return `
        <div class="process-item" data-pid="${p.pid}">
            <span class="process-item-icon">⚙️</span>
            <div class="process-item-info">
                <div class="process-item-name">${escapeHtml(p.name)}</div>
                <div class="process-item-stats">
                    <span class="process-stat">
                        CPU ${cpuPct.toFixed(1)}%
                        <span class="process-bar"><span class="process-bar-fill cpu ${cpuClass}" style="width:${cpuPct}%"></span></span>
                    </span>
                    <span class="process-stat">
                        RAM ${p.memory_mb} MB
                        <span class="process-bar"><span class="process-bar-fill ram ${ramClass}" style="width:${ramPct}%"></span></span>
                    </span>
                </div>
            </div>
            <button class="btn btn-sm btn-danger btn-kill" data-pid="${p.pid}" title="${t('btn_kill')}">✕</button>
        </div>`;
    }).join('');
}

async function killProcess(pid) {
    if (!confirm(t('confirm_kill'))) return;
    try {
        await invoke('kill_process', { pid });
        state.processes = state.processes.filter(p => p.pid !== pid);
        renderProcessList();
        showToast(t('process_killed'), 'success');
    } catch (e) {
        showToast(t('error_kill') + ': ' + e, 'error');
    }
}

function sortProcesses(mode) {
    if (state.processes.length === 0) return;
    switch (mode) {
        case 'cpu-desc': state.processes.sort((a, b) => b.cpu_percent - a.cpu_percent); break;
        case 'cpu-asc':  state.processes.sort((a, b) => a.cpu_percent - b.cpu_percent); break;
        case 'ram-desc': state.processes.sort((a, b) => b.memory_mb - a.memory_mb); break;
        case 'ram-asc':  state.processes.sort((a, b) => a.memory_mb - b.memory_mb); break;
    }
    // Highlight active sort button
    $$('.sort-btn').forEach(b => b.classList.toggle('active', b.dataset.sort === mode));
    renderProcessList();
}

/* ─── Turbo Tab ────────────────────────────────────────────── */
async function toggleTurboMode() {
    if (state.busy) return;
    const btn = $('#btnTurboToggle');
    const statusText = $('#turboStatusText');
    const icon = $('.turbo-main-icon');

    state.busy = true;
    btn.disabled = true;

    try {
        const activate = !state.turboActive;
        await invoke('toggle_game_mode', { activate });
        if (activate) {
            await invoke('disable_telemetry').catch(() => {});
        }

        state.turboActive = activate;

        // Visual feedback with animation
        if (activate) {
            btn.classList.add('activating');
            setTimeout(() => { btn.classList.remove('activating'); btn.classList.add('active'); }, 600);
            btn.textContent = t('turbo_deactivate');
            icon.classList.add('pulse-fast');
            statusText.textContent = t('turbo_active');
            statusText.setAttribute('data-i18n', 'turbo_active');
            btn.setAttribute('data-i18n', 'turbo_deactivate');
            showToast(t('turbo_active'), 'success');
        } else {
            btn.classList.remove('active');
            btn.classList.add('deactivating');
            setTimeout(() => btn.classList.remove('deactivating'), 500);
            btn.textContent = t('turbo_activate');
            icon.classList.remove('pulse-fast');
            statusText.textContent = t('turbo_disabled');
            statusText.setAttribute('data-i18n', 'turbo_disabled');
            btn.setAttribute('data-i18n', 'turbo_activate');
            showToast(t('turbo_disabled'), 'success');
        }
    } catch (e) {
        showToast(t('error_turbo') + ' : ' + e, 'error');
    } finally {
        state.busy = false;
        btn.disabled = false;
    }
}

async function scanBrowserCaches() {
    const btn = $('#btnScanBrowsers');
    if (btn) btn.disabled = true;
    try {
        const report = await invoke('scan_browser_caches');
        const list = $('#browserCacheList');
        if (!list) return;
        if (report.items.length === 0) {
            list.innerHTML = '<p style="color:var(--text-secondary);font-size:.85rem">' + t('no_browser_cache') + '</p>';
            return;
        }
        list.innerHTML = report.items.map((item, idx) => `
            <div class="cleanup-item">
                <label class="browser-cache-check">
                    <input type="checkbox" checked data-browser-idx="${idx}" data-browser-path="${escapeHtml(item.path)}">
                    <span class="cleanup-item-icon">🌐</span>
                </label>
                <div class="cleanup-item-info">
                    <span class="cleanup-item-name">${escapeHtml(item.description)}</span>
                    <span class="cleanup-item-size">${formatBytes(item.size_bytes)}</span>
                </div>
            </div>
        `).join('');
        list.innerHTML += `<p style="margin-top:8px;font-size:.85rem;color:var(--text-secondary)">${t('total')}: ${formatBytes(report.total_bytes)}</p>`;
        list.innerHTML += `<button id="btnCleanBrowsers" class="btn btn-sm btn-accent" style="margin-top:10px">🧹 ${t('btn_clean_selected')}</button>`;
        $('#btnCleanBrowsers').addEventListener('click', cleanSelectedBrowserCaches);
    } catch (e) {
        showToast(t('error') + ': ' + e, 'error');
    } finally {
        if (btn) btn.disabled = false;
    }
}

async function cleanSelectedBrowserCaches() {
    const checkboxes = $$('#browserCacheList input[data-browser-path]:checked');
    const paths = Array.from(checkboxes).map(cb => cb.dataset.browserPath);
    if (paths.length === 0) {
        showToast(t('no_selection'), 'error');
        return;
    }
    try {
        const freed = await invoke('run_cleanup', { paths });
        showToast(formatBytes(freed) + ' ' + t('freed'), 'success');
        // Re-scan to refresh
        scanBrowserCaches();
    } catch (e) {
        showToast(t('error_clean') + ': ' + e, 'error');
    }
}

async function runBloatwarePurge() {
    const btn = $('#btnPurgeBloat');
    const results = $('#bloatwareResults');
    btn.disabled = true;

    try {
        const now = Date.now();
        const cacheFresh = state.bloatwareCache.items && (now - state.bloatwareCache.scannedAt) < 120000;
        let bloatwares = [];

        if (cacheFresh) {
            bloatwares = state.bloatwareCache.items;
            results.textContent = t('bloatware_ready');
        } else {
            results.textContent = t('scanning');
            bloatwares = await invoke('list_bloatwares');
            state.bloatwareCache = { items: bloatwares, scannedAt: now };
        }

        const installed = bloatwares.filter(b => b.installed);
        const missing = bloatwares.filter(b => !b.installed);
        if (installed.length === 0 && missing.length === 0) {
            results.textContent = t('no_bloatware');
            btn.disabled = false;
            return;
        }

        // Categorize bloatwares
        const categories = {
            games: ['king.com', 'Disney', 'CandyCrush', 'Solitaire'],
            thirdparty: ['SpotifyAB', 'BytedancePte', 'Clipchamp'],
        };

        function getBloatCategory(pkg) {
            for (const [cat, patterns] of Object.entries(categories)) {
                if (patterns.some(p => pkg.includes(p))) return cat;
            }
            return 'microsoft';
        }

        const grouped = { microsoft: [], games: [], thirdparty: [] };
        for (const b of bloatwares) {
            const cat = getBloatCategory(b.package);
            grouped[cat].push(b);
        }

        // Build modal content with categories
        const overlay = document.createElement('div');
        overlay.className = 'modal-overlay';
        overlay.id = 'bloatwareOverlay';

        let checklistHtml = '';
        const catLabels = {
            microsoft: t('bloat_cat_microsoft'),
            games: t('bloat_cat_games'),
            thirdparty: t('bloat_cat_thirdparty'),
        };
        for (const [cat, items] of Object.entries(grouped)) {
            if (items.length === 0) continue;
            checklistHtml += `<div class="bloat-category"><h4 class="bloat-category-title">${catLabels[cat] || cat}</h4>`;
            checklistHtml += items.map(b => `
                <label class="bloatware-check-item">
                    <input type="checkbox" ${b.installed ? 'checked' : ''} data-bloat-pkg="${escapeHtml(b.package)}" data-bloat-installed="${b.installed ? '1' : '0'}">
                    <span class="bloatware-check-label">${escapeHtml(b.label)}</span>
                    <span class="bloatware-check-pkg">${escapeHtml(b.package)}</span>
                    <span class="bloatware-check-pkg" style="margin-left:8px;opacity:.85">${b.installed ? t('bloat_status_installed') : t('bloat_status_removed')}</span>
                </label>
            `).join('');
            checklistHtml += '</div>';
        }

        overlay.innerHTML = `
            <div class="modal bloatware-modal">
                <div class="modal-header">
                    <h2>${t('bloatware_select_title')}</h2>
                    <button class="btn-icon btn-close" id="btnCloseBloatModal">✕</button>
                </div>
                <div class="modal-body">
                    <p style="font-size:.85rem;color:var(--text-secondary);margin-bottom:12px">
                        ${t('bloatware_select_desc')}
                    </p>
                    <div style="display:flex;gap:8px;margin-bottom:12px">
                        <button class="btn btn-sm btn-secondary" id="btnBloatSelectAll">${t('btn_select_all')}</button>
                        <button class="btn btn-sm btn-secondary" id="btnBloatDeselectAll">${t('btn_deselect_all')}</button>
                    </div>
                    <div class="bloatware-checklist">
                        ${checklistHtml}
                    </div>
                    <div style="display:flex;gap:8px;margin-top:16px;justify-content:flex-end">
                        <button class="btn btn-sm btn-neutral" id="btnBloatCancel">${t('btn_decline')}</button>
                        <button class="btn btn-sm btn-secondary" id="btnBloatRestore">↩️ ${t('btn_restore_selected')}</button>
                        <button class="btn btn-sm btn-danger" id="btnBloatConfirm">🗑️ ${t('btn_purge_selected')}</button>
                    </div>
                </div>
            </div>
        `;
        document.body.appendChild(overlay);
        results.textContent = t('bloatware_ready');

        const closeModal = () => overlay.remove();
        overlay.querySelector('#btnCloseBloatModal').addEventListener('click', closeModal);
        overlay.querySelector('#btnBloatCancel').addEventListener('click', () => {
            results.textContent = t('bloatware_cancelled');
            closeModal();
        });
        overlay.addEventListener('click', (e) => { if (e.target === overlay) closeModal(); });

        overlay.querySelector('#btnBloatSelectAll').addEventListener('click', () => {
            overlay.querySelectorAll('input[data-bloat-pkg]').forEach(cb => cb.checked = true);
        });
        overlay.querySelector('#btnBloatDeselectAll').addEventListener('click', () => {
            overlay.querySelectorAll('input[data-bloat-pkg]').forEach(cb => cb.checked = false);
        });
        overlay.addEventListener('click', (e) => {
            if (e.target === overlay) {
                results.textContent = t('bloatware_cancelled');
                closeModal();
            }
        });

        overlay.querySelector('#btnBloatConfirm').addEventListener('click', async () => {
            const selected = Array.from(overlay.querySelectorAll('input[data-bloat-pkg]:checked'))
                .map(cb => cb.dataset.bloatPkg);
            if (selected.length === 0) {
                showToast(t('no_selection'), 'error');
                return;
            }
            closeModal();
            results.textContent = t('purging');
            try {
                const message = await invoke('purge_bloatwares', { selection: selected });
                results.textContent = message;
                state.bloatwareCache.scannedAt = 0;
                showToast(message, 'success');
            } catch (err) {
                results.textContent = '';
                showToast(t('error') + ': ' + err, 'error');
            }
        });

        overlay.querySelector('#btnBloatRestore').addEventListener('click', async () => {
            const selected = Array.from(overlay.querySelectorAll('input[data-bloat-pkg]:checked'))
                .map(cb => cb.dataset.bloatPkg);
            if (selected.length === 0) {
                showToast(t('no_selection'), 'error');
                return;
            }
            closeModal();
            results.textContent = t('restoring');
            try {
                const message = await invoke('restore_bloatwares', { selection: selected });
                results.textContent = message;
                state.bloatwareCache.scannedAt = 0;
                showToast(message, 'success');
            } catch (err) {
                results.textContent = '';
                showToast(t('error') + ': ' + err, 'error');
            }
        });
    } catch (e) {
        results.textContent = '';
        showToast(t('error') + ': ' + e, 'error');
    } finally {
        btn.disabled = false;
    }
}

async function loadPredictiveGain() {
    try {
        const gain = await invoke('get_predicted_cleanup_gain');
        const el = $('#predictiveValue');
        if (gain > 0) {
            el.textContent = t('gain_estimated').replace('{0}', formatBytes(gain));
        } else {
            el.textContent = t('gain_no_data');
        }
    } catch (_) {
        const el = $('#predictiveValue');
        if (el) el.textContent = t('gain_no_data');
    }
}

/* ─── Auto-Pilot ───────────────────────────────────────────── */
async function runAutoPilot() {
    if (state.busy) return;
    setBusy(true);

    // Particules en violet électrique pendant l'auto-pilote
    if (auraParticles) {
        auraParticles.setThemeColor('turbo');
        auraParticles.setScanning(true);
    }

    // Play startup sound
    playAutoPilotStart();

    const statusEl = $('#autopilotStatus');
    statusEl.classList.remove('hidden');
    statusEl.textContent = t('autopilot_starting');

    try {
        // Auto-snapshot before operations
        if (state.config && state.config.auto_snapshot) {
            statusEl.textContent = t('snapshot_before_op');
            try { await invoke('create_snapshot', { label: 'Auto-Pilot' }); } catch (_) { /* best-effort */ }
        }

        // 1. Run auto-pilot (updates + cleanup + residues)
        statusEl.textContent = t('autopilot_step_updates');
        const score = await invoke('run_autopilot');

        // 2. Additional cleanup scan
        statusEl.textContent = t('autopilot_step_cleanup');
        try { await invoke('scan_cleanup'); } catch (_e) { /* non-blocking */ }

        // NOTE: Bloatware purge removed from AutoPilot (v2) — manual only via Turbo tab

        renderHealthScore(score);
        statusEl.textContent = t('autopilot_done') + ' — ' + score.total + '%';
        showToast(t('autopilot_done'), 'success');
        playAutoPilotSuccess();
        scanUpdates();
    } catch (e) {
        statusEl.textContent = t('autopilot_error');
        showToast(t('error') + ': ' + e, 'error');
    } finally {
        if (auraParticles) auraParticles.setScanning(false);
        setBusy(false);
    }
}

function playAutoPilotStart() {
    try {
        const ctx = new (window.AudioContext || window.webkitAudioContext)();
        const notes = [
            { freq: 440.00, start: 0,    dur: 0.1  },  // A4
            { freq: 523.25, start: 0.08, dur: 0.1  },  // C5
            { freq: 659.25, start: 0.16, dur: 0.15 },  // E5
        ];
        const master = ctx.createGain();
        master.gain.value = 0.06;
        master.connect(ctx.destination);
        for (const n of notes) {
            const osc = ctx.createOscillator();
            const gain = ctx.createGain();
            osc.type = 'sine';
            osc.frequency.value = n.freq;
            gain.gain.setValueAtTime(0, ctx.currentTime + n.start);
            gain.gain.linearRampToValueAtTime(1, ctx.currentTime + n.start + 0.02);
            gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + n.start + n.dur);
            osc.connect(gain);
            gain.connect(master);
            osc.start(ctx.currentTime + n.start);
            osc.stop(ctx.currentTime + n.start + n.dur + 0.05);
        }
    } catch (_) { /* silent */ }
}

function playAutoPilotSuccess() {
    try {
        const ctx = new (window.AudioContext || window.webkitAudioContext)();
        const notes = [
            { freq: 659.25, start: 0,    dur: 0.12 },  // E5
            { freq: 783.99, start: 0.10, dur: 0.12 },  // G5
            { freq: 1046.5, start: 0.20, dur: 0.15 },  // C6
            { freq: 1318.5, start: 0.32, dur: 0.25 },  // E6 (plus haute)
            { freq: 1568.0, start: 0.50, dur: 0.35 },  // G6 (finale triomphale)
        ];
        const master = ctx.createGain();
        master.gain.value = 0.08;
        master.connect(ctx.destination);
        for (const n of notes) {
            const osc = ctx.createOscillator();
            const gain = ctx.createGain();
            osc.type = 'sine';
            osc.frequency.value = n.freq;
            gain.gain.setValueAtTime(0, ctx.currentTime + n.start);
            gain.gain.linearRampToValueAtTime(1, ctx.currentTime + n.start + 0.03);
            gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + n.start + n.dur);
            osc.connect(gain);
            gain.connect(master);
            osc.start(ctx.currentTime + n.start);
            osc.stop(ctx.currentTime + n.start + n.dur + 0.05);
        }
    } catch (_) { /* Audio non supporté — silencieux */ }
}

/* ─── Settings Modal ───────────────────────────────────────── */
function openSettings() {
    $('#settingsOverlay').classList.remove('hidden');
    syncSettingsUI();
}

function closeSettings() {
    $('#settingsOverlay').classList.add('hidden');
}

function syncSettingsUI() {
    if (!state.config) return;
    highlightOption('[data-theme-choice]', state.config.theme);
    highlightOption('[data-lang-choice]', state.config.language);
    highlightOption('[data-sched]', state.config.scheduler_interval);
    highlightOption('[data-autoclean]', state.config.auto_clean_interval || 'disabled');
    highlightOption('[data-startup-mode]', state.config.startup_mode || 'visible');

    $('#snapshotToggle').checked = state.config.auto_snapshot;
    $('#snapshotStatus').textContent = state.config.auto_snapshot ? t('enabled') : t('disabled');

    $('#aiToggle').checked = state.config.ai_enabled;
    $('#aiStatus').textContent = state.config.ai_enabled ? t('ai_enabled') : t('ai_disabled');
    $('#aiConfig').classList.toggle('hidden', !state.config.ai_enabled);
    $('#aiApiKey').value = state.config.ai_api_key || '';
    $('#aiEndpointInput').value = state.config.ai_endpoint || '';

    // Detect provider from saved endpoint
    const ep = (state.config.ai_endpoint || '').toLowerCase();
    let detectedProvider = 'custom';
    for (const [key, preset] of Object.entries(AI_PRESETS)) {
        if (ep && ep.startsWith(preset.endpoint.toLowerCase())) { detectedProvider = key; break; }
    }
    const sel = $('#aiProviderSelect');
    if (sel) sel.value = detectedProvider;
    // Lock endpoint for presets
    const isPreset = detectedProvider !== 'custom';
    const endpointInput = $('#aiEndpointInput');
    if (endpointInput) endpointInput.readOnly = isPreset;

    // Load models for detected provider, then set saved model
    loadAIModels(detectedProvider).then(() => {
        const modelSel = $('#aiModelSelect');
        if (modelSel && state.config.ai_model) {
            // If saved model exists in list, select it
            if ([...modelSel.options].some(o => o.value === state.config.ai_model)) {
                modelSel.value = state.config.ai_model;
            } else if (state.config.ai_model) {
                // Add it as custom option
                const opt = document.createElement('option');
                opt.value = state.config.ai_model;
                opt.textContent = state.config.ai_model;
                modelSel.appendChild(opt);
                modelSel.value = state.config.ai_model;
            }
        }
    });

    // Telemetry granular toggles (checked = telemetry ACTIVE, so invert for "disable")
    const tw = $('#telemetryWindows');
    const to = $('#telemetryOffice');
    const tn = $('#telemetryNvidia');
    const tb = $('#telemetryBrowsers');
    const tt = $('#telemetryTracking');
    if (tw) tw.checked = !state.config.telemetry_windows;
    if (to) to.checked = !state.config.telemetry_office;
    if (tn) tn.checked = !state.config.telemetry_nvidia;
    if (tb) tb.checked = !state.config.telemetry_browsers;
    if (tt) tt.checked = !state.config.telemetry_tracking;

    // Backup dir
    syncBackupDirUI();

    // Close to Tray
    $('#closeToTrayToggle').checked = state.config.close_to_tray || false;
    $('#closeToTrayStatus').textContent = state.config.close_to_tray ? t('enabled') : t('disabled');

    // App updater startup check
    $('#autoUpdateToggle').checked = state.config.auto_update_on_startup !== false;
    $('#autoUpdateStatus').textContent = state.config.auto_update_on_startup !== false ? t('enabled') : t('disabled');
}

function highlightOption(selector, value) {
    $$(selector).forEach(btn => {
        const val = btn.dataset.themeChoice || btn.dataset.langChoice || btn.dataset.sched || btn.dataset.startupMode || btn.dataset.autoclean;
        btn.classList.toggle('active', val === value);
    });
}

async function saveConfigKey(key, value) {
    try {
        await invoke('set_config_value', { key, value });
        state.config[key] = value;
    } catch (e) { console.warn('Config save failed', key, e); }
}

function openAppUpdateModal(info) {
    state.appUpdateInfo = info;
    $('#appUpdateOverlay').classList.remove('hidden');
    $('#appUpdateVersion').textContent = `${t('app_update_available')} ${info.current_version} → ${info.version || '?'}`;
    $('#appUpdateNotes').textContent = info.release_notes || t('app_update_no_notes');
    $('#appUpdateProgressWrap').classList.add('hidden');
    $('#appUpdateProgressBar').style.width = '0%';
    $('#appUpdateProgressText').textContent = '0%';
}

function closeAppUpdateModal(force = false) {
    if (state.appUpdateInstalling && !force) {
        showToast(t('app_update_installing_lock'), 'warning');
        return;
    }
    $('#appUpdateOverlay').classList.add('hidden');
}

async function manualCheckAppUpdate(showNoUpdateToast = false) {
    if (STRICT_PRIVACY_MODE) {
        if (showNoUpdateToast) showToast(t('privacy_updates_disabled') || 'Mode confidentialité stricte: mises à jour réseau désactivées', 'warning');
        return {
            available: false,
            current_version: state.config?.app_version || '',
            version: null,
            release_notes: null,
        };
    }

    try {
        const info = await invoke('check_app_update');
        if (info.available) {
            await invoke('set_tray_update_available', { available: true, version: info.version });
            openAppUpdateModal(info);
        } else {
            await invoke('set_tray_update_available', { available: false, version: null });
            if (showNoUpdateToast) showToast(t('app_update_none'), 'success');
        }
        return info;
    } catch (e) {
        if (showNoUpdateToast) showToast(t('error') + ': ' + e, 'error');
        return null;
    }
}

async function installAppUpdateNow() {
    if (state.appUpdateInstalling) return;
    state.appUpdateInstalling = true;
    $('#appUpdateProgressWrap').classList.remove('hidden');
    $('#btnInstallAppUpdate').disabled = true;
    $('#btnLaterAppUpdate').disabled = true;
    $('#btnCloseAppUpdate').disabled = true;
    try {
        await invoke('install_app_update');
    } catch (e) {
        state.appUpdateInstalling = false;
        $('#btnInstallAppUpdate').disabled = false;
        $('#btnLaterAppUpdate').disabled = false;
        $('#btnCloseAppUpdate').disabled = false;
        showToast(t('error') + ': ' + e, 'error');
    }
}

async function startupCheckAppUpdate() {
    if (STRICT_PRIVACY_MODE) return;
    if (!state.config || state.config.auto_update_on_startup === false) return;
    await manualCheckAppUpdate(false);
}

/* ─── AI ───────────────────────────────────────────────────── */
async function openAIHelp(context, contextType) {
    if (STRICT_PRIVACY_MODE && !isLocalEndpoint(state.config?.ai_endpoint || '')) {
        showToast(t('ai_local_only') || 'Mode confidentialité stricte: IA distante désactivée', 'warning');
        return;
    }

    // Check availability
    const available = await invoke('ai_is_available');
    if (!available) {
        if (!state.config.ai_consent_given) {
            openConsent(context, contextType);
        } else {
            showToast(t('ai_not_configured'), 'error');
        }
        return;
    }

    // Show AI modal
    $('#aiOverlay').classList.remove('hidden');
    $('#aiLoading').classList.remove('hidden');
    $('#aiResult').classList.add('hidden');
    $('#aiError').classList.add('hidden');

    try {
        const request = { context, context_type: contextType };
        const result = await invoke('ai_analyze', { request });
        $('#aiResultContent').textContent = result;
        $('#aiLoading').classList.add('hidden');
        $('#aiResult').classList.remove('hidden');
    } catch (e) {
        $('#aiLoading').classList.add('hidden');
        $('#aiError').classList.remove('hidden');
        // Show real error message
        const errEl = $('#aiErrorDetail');
        if (errEl) errEl.textContent = typeof e === 'string' ? e : (e.message || t('ai_error'));
    }
}

function openConsent(pendingContext, pendingType) {
    $('#consentOverlay').classList.remove('hidden');
    // Store pending request
    state._pendingAI = { context: pendingContext, type: pendingType };
}

async function acceptConsent() {
    $('#consentOverlay').classList.add('hidden');
    await saveConfigKey('ai_consent_given', true);
    state.config.ai_consent_given = true;

    // If AI not enabled, enable it
    if (!state.config.ai_enabled) {
        await saveConfigKey('ai_enabled', true);
        state.config.ai_enabled = true;
    }

    // Sync toggle UI: check the toggle, update status text, show config panel
    const toggle = $('#aiToggle');
    if (toggle) toggle.checked = true;
    const status = $('#aiStatus');
    if (status) status.textContent = t('ai_enabled');
    const configPanel = $('#aiConfig');
    if (configPanel) configPanel.classList.remove('hidden');

    // Proceed with pending AI request
    if (state._pendingAI && state._pendingAI.context) {
        openAIHelp(state._pendingAI.context, state._pendingAI.type);
        state._pendingAI = null;
    }
}

async function saveAIConfig() {
    const provider = $('#aiProviderSelect')?.value || 'custom';
    const preset = AI_PRESETS[provider];
    const endpoint = $('#aiEndpointInput').value.trim() || (preset ? preset.endpoint : '');
    const model = $('#aiModelSelect')?.value?.trim() || (preset ? preset.model : '');
    const apiKey = $('#aiApiKey').value.trim();
    const enabled = $('#aiToggle').checked;
    const consent = state.config.ai_consent_given;

    if (!endpoint) {
        showToast(t('ai_no_endpoint') || 'Choisissez un fournisseur ou saisissez un endpoint', 'warning');
        return;
    }

    try {
        await invoke('configure_ai', {
            enabled,
            endpoint,
            apiKey,
            consentGiven: consent,
        });
        // Also save model separately
        await invoke('set_config_value', { key: 'ai_model', value: model });
        state.config.ai_enabled = enabled;
        state.config.ai_endpoint = endpoint;
        state.config.ai_api_key = apiKey;
        state.config.ai_model = model;
        showToast(t('saved'), 'success');
    } catch (e) {
        showToast(t('error') + ': ' + e, 'error');
    }
}

/* ─── Remote Dashboard ─────────────────────────────────────── */
function openRemoteModal() {
    $('#remoteOverlay').classList.remove('hidden');
    refreshRemoteStatus();
}

async function refreshRemoteStatus() {
    try {
        const status = await invoke('get_remote_status');
        $('#remoteLoading').classList.add('hidden');
        if (status.running) {
            $('#remoteOff').classList.add('hidden');
            $('#remoteOn').classList.remove('hidden');
            $('#remoteURL').textContent = status.url;
        } else {
            $('#remoteOff').classList.remove('hidden');
            $('#remoteOn').classList.add('hidden');
        }
    } catch (_) {}
}

async function startRemote() {
    if (STRICT_PRIVACY_MODE) {
        showToast(t('privacy_remote_disabled') || 'Mode confidentialité stricte: dashboard distant désactivé', 'warning');
        return;
    }

    // Show loading animation
    $('#remoteOff').classList.add('hidden');
    $('#remoteLoading').classList.remove('hidden');
    $('#remoteOn').classList.add('hidden');

    try {
        const info = await invoke('start_remote');
        $('#remoteLoading').classList.add('hidden');
        $('#remoteOn').classList.remove('hidden');
        $('#remoteQR').innerHTML = info.qr_svg;
        $('#remoteURL').textContent = info.url;
        showToast(t('remote_started'), 'success');
    } catch (e) {
        $('#remoteLoading').classList.add('hidden');
        $('#remoteOff').classList.remove('hidden');
        showToast(t('error') + ': ' + e, 'error');
    }
}

async function stopRemote() {
    try {
        await invoke('stop_remote');
        $('#remoteLoading').classList.add('hidden');
        $('#remoteOff').classList.remove('hidden');
        $('#remoteOn').classList.add('hidden');
        $('#remoteQR').innerHTML = '';
        showToast(t('remote_stopped'), 'success');
    } catch (e) {
        showToast(t('error') + ': ' + e, 'error');
    }
}

/* ─── Admin ────────────────────────────────────────────────── */
async function checkAdmin() {
    const admin = await invoke('is_admin');
    if (!admin) {
        $('#adminBanner').classList.remove('hidden');
        // Grey out scheduler options when not admin
        const schedOptions = $('#schedOptions');
        const schedNotice = $('#schedAdminNotice');
        if (schedOptions) {
            schedOptions.querySelectorAll('[data-sched]').forEach(btn => {
                if (btn.dataset.sched !== 'disabled') {
                    btn.disabled = true;
                    btn.classList.add('disabled');
                }
            });
        }
        if (schedNotice) schedNotice.classList.remove('hidden');
    }
}

/* ─── Scheduler ────────────────────────────────────────────── */
async function applySchedule(interval) {
    const enabled = interval !== 'disabled';
    try {
        await invoke('set_schedule', { enabled, interval });
        await saveConfigKey('scheduler_enabled', enabled);
        await saveConfigKey('scheduler_interval', interval);
        highlightOption('[data-sched]', interval);
        showToast(t('saved'), 'success');
    } catch (e) {
        showToast(t('error') + ': ' + e, 'error');
    }
}

async function applyAutoCleanSchedule(interval) {
    const enabled = interval !== 'disabled';
    try {
        await invoke('set_auto_clean_schedule', { enabled, interval });
        await saveConfigKey('auto_clean_enabled', enabled);
        await saveConfigKey('auto_clean_interval', interval);
        highlightOption('[data-autoclean]', interval);
        showToast(t('saved'), 'success');
    } catch (e) {
        showToast(t('error') + ': ' + e, 'error');
    }
}

/* ─── Events from Rust ─────────────────────────────────────── */
async function setupListeners() {
    await listen('update-progress', (event) => {
        const d = event.payload;
        if (d.status === 'running') {
            updateProgress(d.message, 0, 0);
        }
        if (d.status === 'done') {
            const item = $(`.update-item[data-id="${CSS.escape(d.id)}"]`);
            if (item) item.classList.add('installed');
        }
    });

    await listen('autopilot-progress', (event) => {
        const d = event.payload;
        const statusEl = $('#autopilotStatus');
        statusEl.classList.remove('hidden');
        statusEl.textContent = d.message || d.step || '';
    });

    await listen('remote-action', (event) => {
        const action = event.payload;
        if (action === 'scan') scanUpdates();
        else if (action === 'clean') runCleanup();
        else if (action === 'autopilot') runAutoPilot();
        else if (action === 'turbo') toggleTurboMode();
        else if (action === 'coolboost') runCoolBoost();
    });

    await listen('app-update-available', (event) => {
        const info = event.payload;
        if (info && info.available) openAppUpdateModal(info);
    });

    await listen('app-update-progress', (event) => {
        const payload = event.payload || {};
        const phase = payload.phase || '';
        const percent = typeof payload.percent === 'number' ? payload.percent : 0;
        if (phase === 'starting' || phase === 'downloading' || phase === 'installing') {
            $('#appUpdateProgressWrap').classList.remove('hidden');
        }
        $('#appUpdateProgressBar').style.width = `${Math.max(0, Math.min(100, percent))}%`;
        if (phase === 'installing') {
            $('#appUpdateProgressText').textContent = t('app_update_installing');
        } else if (phase === 'done') {
            $('#appUpdateProgressText').textContent = t('app_update_restarting');
        } else {
            $('#appUpdateProgressText').textContent = `${percent}%`;
        }
    });

    // Maintenance — mise à jour de l'output en temps réel
    await listen('maintenance-progress', (event) => {
        const d = event.payload || {};
        const taskKey = d.task ? d.task.replace(/_/g, '-') : null;
        if (!taskKey) return;
        const outEl = $(`#out-${taskKey}`);
        if (outEl && d.output) outEl.textContent = d.output;
    });
}

/* ─── Event Bindings ───────────────────────────────────────── */
function bindEvents() {
    // Tab navigation
    $$('.tab').forEach(tab => {
        tab.addEventListener('click', () => switchTab(tab.dataset.tab));
    });

    // Header buttons
    $('#btnRemote').addEventListener('click', openRemoteModal);
    $('#btnSettings').addEventListener('click', openSettings);
    $('#btnTheme').addEventListener('click', () => {
        const next = document.documentElement.getAttribute('data-theme') === 'dark' ? 'light' : 'dark';
        setTheme(next);
        saveConfigKey('theme', next);
    });
    $('#btnLang').addEventListener('click', async () => {
        const next = state.config.language === 'fr' ? 'en' : 'fr';
        await saveConfigKey('language', next);
        state.config.language = next;
        $('#langIcon').textContent = next === 'fr' ? '🇫🇷' : '🇺🇸';
        await loadTranslations(next);
    });

    // Auto-Pilot
    $('#btnAutoPilot').addEventListener('click', runAutoPilot);

    // Cool Boost
    $('#btnCoolBoost').addEventListener('click', runCoolBoost);

    // Updates tab
    $('#btnScan').addEventListener('click', scanUpdates);
    $('#btnUpdateAll').addEventListener('click', installAllUpdates);
    $('#updateList').addEventListener('click', (e) => {
        const installBtn = e.target.closest('.btn-install');
        if (installBtn) return installUpdate(installBtn.dataset.id);
        const aiBtn = e.target.closest('.btn-ai-help');
        if (aiBtn) return openAIHelp(aiBtn.dataset.context, 'update_error');
    });

    // Cleanup tab
    $('#btnScanClean').addEventListener('click', scanCleanup);
    $('#btnRunClean').addEventListener('click', runCleanup);
    $('#btnScanResidues').addEventListener('click', scanResidues);

    // Startup tab
    $('#btnLoadStartup').addEventListener('click', loadStartupItems);
    $('#startupList').addEventListener('change', (e) => {
        if (e.target.dataset.startup) {
            toggleStartup(e.target.dataset.startup, e.target.checked);
        }
    });

    // Processes tab
    $('#btnLoadProcs').addEventListener('click', loadProcesses);
    $('#processList').addEventListener('click', (e) => {
        const killBtn = e.target.closest('.btn-kill');
        if (killBtn) killProcess(parseInt(killBtn.dataset.pid, 10));
    });
    $$('.sort-btn').forEach(btn => {
        btn.addEventListener('click', () => sortProcesses(btn.dataset.sort));
    });

    // Turbo tab
    $('#btnTurboToggle').addEventListener('click', toggleTurboMode);
    if ($('#btnScanBrowsers')) $('#btnScanBrowsers').addEventListener('click', scanBrowserCaches);
    if ($('#btnScanBrowserGranular')) $('#btnScanBrowserGranular').addEventListener('click', scanBrowserGranular);
    if ($('#btnCleanBrowserGranular')) $('#btnCleanBrowserGranular').addEventListener('click', cleanBrowserGranular);
    $('#btnPurgeBloat').addEventListener('click', runBloatwarePurge);

    // Maintenance Avancée & Moteurs
    $('#btnMaintUpdateGit').addEventListener('click', () =>
        runMaintenanceTask('update_git', 'maintenance_update_git', 'btnMaintUpdateGit'));
    $('#btnMaintUpdateApps').addEventListener('click', () =>
        runMaintenanceTask('update_apps', 'maintenance_update_apps', 'btnMaintUpdateApps'));
    $('#btnMaintRepairSystem').addEventListener('click', () =>
        runMaintenanceTask('repair_system', 'maintenance_repair_system', 'btnMaintRepairSystem'));
    $('#btnMaintCleanSystem').addEventListener('click', () =>
        runMaintenanceTask('clean_system', 'maintenance_clean_system', 'btnMaintCleanSystem'));

    // Settings modal
    $('#btnCloseSettings').addEventListener('click', closeSettings);
    $('#settingsOverlay').addEventListener('click', (e) => {
        if (e.target === e.currentTarget) closeSettings();
    });

    // Theme choices
    $$('[data-theme-choice]').forEach(btn => {
        btn.addEventListener('click', () => {
            const val = btn.dataset.themeChoice;
            setTheme(val);
            saveConfigKey('theme', val);
        });
    });

    // Language choices
    $$('[data-lang-choice]').forEach(btn => {
        btn.addEventListener('click', async () => {
            const val = btn.dataset.langChoice;
            await saveConfigKey('language', val);
            state.config.language = val;
            $('#langIcon').textContent = val === 'fr' ? '🇫🇷' : '🇺🇸';
            await loadTranslations(val);
            syncSettingsUI();
        });
    });

    // Aura Néo web links
    const openExternal = async (url) => {
        if (STRICT_PRIVACY_MODE) {
            showToast(t('privacy_links_disabled') || 'Mode confidentialité stricte: liens externes désactivés', 'warning');
            return;
        }
        try { await invoke('open_url', { url }); } catch (e) { console.warn('open_url failed', e); }
    };
    $('#btnLinkWebsite').addEventListener('click', () => openExternal('https://www.auraneo.fr'));
    $('#btnLinkDocs').addEventListener('click', () => openExternal('https://www.auraneo.fr/aura-update'));

    // Scheduler choices
    $$('[data-sched]').forEach(btn => {
        btn.addEventListener('click', () => applySchedule(btn.dataset.sched));
    });

    // Auto-clean scheduler choices
    $$('[data-autoclean]').forEach(btn => {
        btn.addEventListener('click', () => applyAutoCleanSchedule(btn.dataset.autoclean));
    });

    // Snapshot toggle
    $('#snapshotToggle').addEventListener('change', (e) => {
        const val = e.target.checked;
        saveConfigKey('auto_snapshot', val);
        $('#snapshotStatus').textContent = val ? t('enabled') : t('disabled');
    });

    // Close to Tray toggle
    $('#closeToTrayToggle').addEventListener('change', (e) => {
        const val = e.target.checked;
        saveConfigKey('close_to_tray', val);
        $('#closeToTrayStatus').textContent = val ? t('enabled') : t('disabled');
    });

    $('#autoUpdateToggle').addEventListener('change', (e) => {
        const val = e.target.checked;
        saveConfigKey('auto_update_on_startup', val);
        state.config.auto_update_on_startup = val;
        $('#autoUpdateStatus').textContent = val ? t('enabled') : t('disabled');
    });

    // AI toggle
    $('#aiToggle').addEventListener('change', (e) => {
        const enabled = e.target.checked;
        if (enabled && !state.config.ai_consent_given) {
            e.target.checked = false;
            openConsent(null, null);
            return;
        }
        saveConfigKey('ai_enabled', enabled);
        state.config.ai_enabled = enabled;
        $('#aiStatus').textContent = enabled ? t('ai_enabled') : t('ai_disabled');
        $('#aiConfig').classList.toggle('hidden', !enabled);
    });

    $('#btnSaveAI').addEventListener('click', saveAIConfig);

    // AI provider presets dropdown
    const providerSel = $('#aiProviderSelect');
    if (providerSel) {
        providerSel.addEventListener('change', () => {
            const key = providerSel.value;
            const preset = AI_PRESETS[key];
            const endpointInput = $('#aiEndpointInput');
            const apiKeyInput = $('#aiApiKey');
            if (preset) {
                endpointInput.value = preset.endpoint;
                endpointInput.readOnly = true;
                if (!preset.needsKey) {
                    apiKeyInput.value = '';
                    apiKeyInput.placeholder = t('ai_no_key_needed') || 'Pas de clé nécessaire';
                } else if (key === 'auraneo') {
                    apiKeyInput.placeholder = 'publicKey:secretKey (depuis admin.auraneo.fr/apps)';
                } else {
                    apiKeyInput.placeholder = t('api_key_placeholder') || 'Votre clé API';
                }
            } else {
                // Custom: clear and unlock
                endpointInput.value = '';
                endpointInput.readOnly = false;
                apiKeyInput.placeholder = t('api_key_placeholder') || 'Votre clé API';
            }
            // Load models for the new provider
            loadAIModels(key);
        });
    }

    // Refresh models button
    const refreshBtn = $('#btnRefreshModels');
    if (refreshBtn) {
        refreshBtn.addEventListener('click', () => {
            const key = $('#aiProviderSelect')?.value || 'custom';
            loadAIModels(key, true);
        });
    }

    // AI modal
    $('#btnCloseAI').addEventListener('click', () => $('#aiOverlay').classList.add('hidden'));
    $('#aiOverlay').addEventListener('click', (e) => {
        if (e.target === e.currentTarget) e.currentTarget.classList.add('hidden');
    });

    // Consent modal
    $('#btnConsentDecline').addEventListener('click', () => {
        $('#consentOverlay').classList.add('hidden');
        state._pendingAI = null;
    });
    $('#btnConsentAccept').addEventListener('click', acceptConsent);

    $('#btnInstallAppUpdate').addEventListener('click', installAppUpdateNow);
    $('#btnLaterAppUpdate').addEventListener('click', () => closeAppUpdateModal(false));
    $('#btnCloseAppUpdate').addEventListener('click', () => closeAppUpdateModal(false));
    $('#appUpdateOverlay').addEventListener('click', (e) => {
        if (e.target === e.currentTarget) closeAppUpdateModal(false);
    });

    // Remote modal
    $('#btnCloseRemote').addEventListener('click', () => $('#remoteOverlay').classList.add('hidden'));
    $('#remoteOverlay').addEventListener('click', (e) => {
        if (e.target === e.currentTarget) e.currentTarget.classList.add('hidden');
    });
    $('#btnStartRemote').addEventListener('click', startRemote);
    $('#btnStopRemote').addEventListener('click', stopRemote);

    // Startup mode choices
    $$('[data-startup-mode]').forEach(btn => {
        btn.addEventListener('click', () => {
            const val = btn.dataset.startupMode;
            saveConfigKey('startup_mode', val);
            state.config.startup_mode = val;
            highlightOption('[data-startup-mode]', val);
        });
    });

    // Telemetry granular toggles
    const telemetryHandler = async (el, category, configKey) => {
        if (!el) return;
        el.addEventListener('change', async () => {
            const disable = el.checked; // checked = user wants to disable telemetry
            showToast(t('telemetry_applying'), 'success');
            try {
                await invoke('disable_telemetry_granular', { category, disable });
                await saveConfigKey(configKey, !disable); // config stores "is telemetry active"
                showToast(t('telemetry_applied'), 'success');
            } catch (e) {
                showToast(t('error') + ': ' + e, 'error');
                el.checked = !el.checked; // revert
            }
        });
    };
    telemetryHandler($('#telemetryWindows'), 'windows', 'telemetry_windows');
    telemetryHandler($('#telemetryOffice'), 'office', 'telemetry_office');
    telemetryHandler($('#telemetryNvidia'), 'nvidia', 'telemetry_nvidia');
    telemetryHandler($('#telemetryBrowsers'), 'browsers', 'telemetry_browsers');
    telemetryHandler($('#telemetryTracking'), 'tracking', 'telemetry_tracking');

    // Backups
    const btnLoadBackups = $('#btnLoadBackups');
    if (btnLoadBackups) btnLoadBackups.addEventListener('click', loadBackups);
    const btnCreateSnapshot = $('#btnCreateSnapshot');
    if (btnCreateSnapshot) btnCreateSnapshot.addEventListener('click', createManualSnapshot);
    const btnCreateLocalBackup = $('#btnCreateLocalBackup');
    if (btnCreateLocalBackup) btnCreateLocalBackup.addEventListener('click', createLocalBackup);
    const btnPickBackupDir = $('#btnPickBackupDir');
    if (btnPickBackupDir) btnPickBackupDir.addEventListener('click', pickBackupDir);

    // Temp alert
    const btnCloseTempAlert = $('#btnCloseTempAlert');
    if (btnCloseTempAlert) btnCloseTempAlert.addEventListener('click', closeTempAlert);
    const btnTempAlertClean = $('#btnTempAlertClean');
    if (btnTempAlertClean) btnTempAlertClean.addEventListener('click', tempAlertClean);
    const btnTempAlertDismiss = $('#btnTempAlertDismiss');
    if (btnTempAlertDismiss) btnTempAlertDismiss.addEventListener('click', closeTempAlert);

    // Restore warning modal
    const btnCloseRestore = $('#btnCloseRestore');
    if (btnCloseRestore) btnCloseRestore.addEventListener('click', closeRestoreWarning);
    const btnRestoreCancel = $('#btnRestoreCancel');
    if (btnRestoreCancel) btnRestoreCancel.addEventListener('click', closeRestoreWarning);
    const restoreOverlay = $('#restoreOverlay');
    if (restoreOverlay) restoreOverlay.addEventListener('click', (e) => { if (e.target === e.currentTarget) closeRestoreWarning(); });
    const btnRestoreConfirm = $('#btnRestoreConfirm');
    if (btnRestoreConfirm) {
        btnRestoreConfirm.addEventListener('click', async () => {
            if (!pendingRestoreId) return;
            showToast(t('restore_warning_title') + '…', 'success');
            closeRestoreWarning();
            // Windows: rstrui.exe triggers the System Restore UI
            try { await invoke('open_url', { url: 'https://support.microsoft.com/windows/system-restore' }); } catch (_) {}
        });
    }

    // Licence modal
    const btnLicenceFR = $('#btnLicenceFR');
    if (btnLicenceFR) btnLicenceFR.addEventListener('click', () => openLicence('fr'));
    const btnLicenceEN = $('#btnLicenceEN');
    if (btnLicenceEN) btnLicenceEN.addEventListener('click', () => openLicence('en'));
    const btnCloseLicence = $('#btnCloseLicence');
    if (btnCloseLicence) btnCloseLicence.addEventListener('click', () => $('#licenceOverlay').classList.add('hidden'));
    const licenceOverlay = $('#licenceOverlay');
    if (licenceOverlay) licenceOverlay.addEventListener('click', (e) => { if (e.target === e.currentTarget) e.currentTarget.classList.add('hidden'); });

    // Backup dir
    const btnChooseBackupDir = $('#btnChooseBackupDir');
    if (btnChooseBackupDir) btnChooseBackupDir.addEventListener('click', chooseBackupDir);
    const btnResetBackupDir = $('#btnResetBackupDir');
    if (btnResetBackupDir) btnResetBackupDir.addEventListener('click', resetBackupDir);

    // Crash report modal
    const btnCrashDismiss = $('#btnCrashDismiss');
    if (btnCrashDismiss) btnCrashDismiss.addEventListener('click', dismissCrash);
    const btnCrashSend = $('#btnCrashSend');
    if (btnCrashSend) btnCrashSend.addEventListener('click', sendCrashReport);
}

/* ─── Backups / Snapshots ───────────────────────────────────── */
async function loadBackups() {
    const list = $('#backupsList');
    if (!list) return;
    list.innerHTML = `<p style="color:var(--text-secondary);font-size:.85rem">${t('backups_loading')}</p>`;
    try {
        const snapshots = await invoke('list_snapshots');
        if (snapshots.length === 0) {
            list.innerHTML = `<p style="color:var(--text-secondary);font-size:.85rem">${t('backups_empty')}</p>`;
            return;
        }
        list.innerHTML = snapshots.map(s => `
            <div class="backup-item">
                <span class="backup-icon">💾</span>
                <div class="backup-info">
                    <div class="backup-desc">${escapeHtml(s.description)}</div>
                    <div class="backup-date">${escapeHtml(s.date)}</div>
                </div>
                <button class="btn btn-sm btn-neutral btn-restore-point" data-snapshot-id="${escapeHtml(s.id)}" data-snapshot-desc="${escapeHtml(s.description)}">🔄</button>
            </div>
        `).join('');

        // Attach restore handlers
        list.querySelectorAll('.btn-restore-point').forEach(btn => {
            btn.addEventListener('click', () => openRestoreWarning(btn.dataset.snapshotId, btn.dataset.snapshotDesc));
        });
    } catch (e) {
        list.innerHTML = `<p style="color:var(--danger);font-size:.85rem">${t('error')}: ${escapeHtml(String(e))}</p>`;
    }
}

async function createManualSnapshot() {
    const btn = $('#btnCreateSnapshot');
    if (btn) btn.disabled = true;
    showToast(t('snapshot_before_op'), 'success');
    try {
        await invoke('create_snapshot', { label: 'Manuel — Aura Update' });
        showToast(t('snapshot_created'), 'success');
        loadBackups();
    } catch (e) {
        showToast(t('error') + ': ' + e, 'error');
    } finally {
        if (btn) btn.disabled = false;
    }
}

/* ─── Custom Backup Directory ──────────────────────────────── */
async function initBackupDir() {
    try {
        if (state.config && state.config.backup_dir) {
            $('#backupDirInput').value = state.config.backup_dir;
        } else {
            const def = await invoke('get_default_backup_dir');
            $('#backupDirInput').value = def;
        }
    } catch (_) {}
}

async function pickBackupDir() {
    try {
        const { open } = window.__TAURI__.dialog || window.__TAURI_PLUGIN_DIALOG__;
        const selected = await open({ directory: true, multiple: false, title: 'Dossier de sauvegarde' });
        if (selected) {
            await invoke('set_config_value', { key: 'backup_dir', value: selected });
            $('#backupDirInput').value = selected;
            state.config.backup_dir = selected;
            showToast(t('backup_dir_set') || 'Dossier configuré', 'success');
        }
    } catch (e) {
        showToast(t('error') + ': ' + e, 'error');
    }
}

async function createLocalBackup() {
    const btn = $('#btnCreateLocalBackup');
    if (btn) btn.disabled = true;
    try {
        const backupDir = (state.config && state.config.backup_dir) || '';
        const msg = await invoke('create_local_backup', { backupDir, label: 'Manuel — Aura Update' });
        showToast(msg, 'success');
        loadLocalBackups();
    } catch (e) {
        showToast(t('error') + ': ' + e, 'error');
    } finally {
        if (btn) btn.disabled = false;
    }
}

async function loadLocalBackups() {
    const list = $('#localBackupsList');
    if (!list) return;
    try {
        const backupDir = (state.config && state.config.backup_dir) || '';
        const backups = await invoke('list_local_backups', { backupDir });
        if (backups.length === 0) {
            list.innerHTML = '<p style="color:var(--text-muted);font-size:.8rem;">Aucun backup local</p>';
            return;
        }
        list.innerHTML = backups.map(b => `
            <div class="backup-item">
                <span class="backup-icon">📁</span>
                <div class="backup-info">
                    <div class="backup-name">${escapeHtml(b.description)}</div>
                    <div class="backup-date">${escapeHtml(b.date)}</div>
                </div>
            </div>`).join('');
    } catch (e) {
        list.innerHTML = `<p style="color:var(--danger);font-size:.85rem">${escapeHtml(String(e))}</p>`;
    }
}

/* ─── Auto-snapshot before heavy cleanup ───────────────────── */
async function safeSnapshot(label) {
    if (state.config && state.config.auto_snapshot) {
        try { await invoke('create_snapshot', { label }); } catch (_) { /* best-effort */ }
    }
}

/* ─── Hardware Auto-Detection ──────────────────────────────── */
function applyHardwareDetection(vitals) {
    // Hide battery section if no battery detected
    if (vitals.battery_percent == null) {
        const battItem = $('#vitalBattery');
        if (battItem) battItem.style.display = 'none';
    }
}

/* ─── System Specs (Dashboard Premium) ─────────────────────── */
async function loadSystemSpecs() {
    try {
        const specs = await invoke('get_system_specs');

        // Nettoyage intelligent des noms
        const cleanCpu = specs.cpu
            .replace(/Intel\(R\) Core\(TM\) /g, '')
            .replace(/ CPU @.*/g, '')
            .replace(/AMD Ryzen \d /g, 'Ryzen ');

        const cleanGpu = specs.gpu.replace(/NVIDIA GeForce /g, '');

        $('#specOs').textContent = specs.os;
        $('#specCpu').textContent = cleanCpu;
        $('#specGpu').textContent = cleanGpu;
        $('#specRam').textContent = specs.ram;
    } catch (e) {
        console.error('Specs Error:', e);
    }
}

/* ─── Splash melody (Web Audio API) ────────────────────────── */
function playSplashMelody() {
    try {
        const ctx = new (window.AudioContext || window.webkitAudioContext)();
        const notes = [
            { freq: 523.25, start: 0,    dur: 0.15 },  // C5
            { freq: 659.25, start: 0.12, dur: 0.15 },  // E5
            { freq: 783.99, start: 0.24, dur: 0.2  },  // G5
            { freq: 1046.5, start: 0.4,  dur: 0.35 },  // C6 (longer)
        ];
        const master = ctx.createGain();
        master.gain.value = 0.08;
        master.connect(ctx.destination);
        for (const n of notes) {
            const osc = ctx.createOscillator();
            const gain = ctx.createGain();
            osc.type = 'sine';
            osc.frequency.value = n.freq;
            gain.gain.setValueAtTime(0, ctx.currentTime + n.start);
            gain.gain.linearRampToValueAtTime(1, ctx.currentTime + n.start + 0.03);
            gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + n.start + n.dur);
            osc.connect(gain);
            gain.connect(master);
            osc.start(ctx.currentTime + n.start);
            osc.stop(ctx.currentTime + n.start + n.dur + 0.05);
        }
    } catch (_) { /* Audio not supported or blocked — silent */ }
}

/* ─── Onboarding Tutorial ──────────────────────────────────── */
const tutoSteps = [
    { id: '#healthSection',            key: 'tuto_health'   },
    { id: '#btnAutoPilot',             key: 'tuto_pilot'    },
    { id: '.tab-bar',                  key: 'tuto_tabs'     },
    { id: '.tab[data-tab="turbo"]',    key: 'tuto_turbo'    },
    { id: '.topbar-right',             key: 'tuto_settings' },
];

let tutoCurrentStep = 0;
let tutoOverlay = null;

async function startTutorial() {
    if (!state.config || !state.config.first_run) return;
    tutoCurrentStep = 0;

    // Create overlay
    tutoOverlay = document.createElement('div');
    tutoOverlay.className = 'tuto-overlay';
    document.body.appendChild(tutoOverlay);

    showTutoStep(tutoCurrentStep);
}

function showTutoStep(index) {
    const step = tutoSteps[index];
    const el = $(step.id);
    if (!el) { nextTutoStep(); return; }

    // Cleanup previous
    $$('.tuto-highlight').forEach(i => i.classList.remove('tuto-highlight'));
    const oldBubble = $('.tuto-bubble');
    if (oldBubble) oldBubble.remove();

    // Highlight target
    el.classList.add('tuto-highlight');
    el.scrollIntoView({ behavior: 'smooth', block: 'center' });

    // Create bubble
    const bubble = document.createElement('div');
    bubble.className = 'tuto-bubble';

    const rect = el.getBoundingClientRect();
    const spaceBelow = window.innerHeight - rect.bottom;

    if (spaceBelow > 160) {
        bubble.style.top = (rect.bottom + 15) + 'px';
    } else {
        bubble.style.top = Math.max(10, rect.top - 160) + 'px';
    }
    bubble.style.left = Math.max(10, Math.min(
        window.innerWidth - 320,
        rect.left + (rect.width / 2) - 150
    )) + 'px';

    bubble.innerHTML = `
        <div class="tuto-step-indicator">${index + 1} / ${tutoSteps.length}</div>
        <p>${t(step.key)}</p>
        <div>
            <button class="btn-tuto-next" id="btnTutoNext">
                ${index === tutoSteps.length - 1 ? t('tuto_finish') : t('btn_next')}
            </button>
            <button class="btn-tuto-skip" id="btnTutoSkip">${t('tuto_skip')}</button>
        </div>
    `;
    document.body.appendChild(bubble);

    // Bind events via JS (CSP blocks inline onclick)
    bubble.querySelector('#btnTutoNext').addEventListener('click', () => window.nextTutoStep());
    bubble.querySelector('#btnTutoSkip').addEventListener('click', () => window.skipTutorial());
}

window.nextTutoStep = () => {
    tutoCurrentStep++;
    if (tutoCurrentStep < tutoSteps.length) {
        showTutoStep(tutoCurrentStep);
    } else {
        endTutorial();
    }
};

window.skipTutorial = () => {
    endTutorial();
};

async function endTutorial() {
    // Cleanup
    const bubble = $('.tuto-bubble');
    if (bubble) bubble.remove();
    $$('.tuto-highlight').forEach(i => i.classList.remove('tuto-highlight'));
    if (tutoOverlay) { tutoOverlay.remove(); tutoOverlay = null; }

    // Persist: never show again
    try {
        await invoke('set_config_value', { key: 'first_run', value: false });
        state.config.first_run = false;
    } catch (e) { console.warn('Could not save first_run:', e); }

    showToast(t('tuto_done'), 'success');
}

/* ─── Init ─────────────────────────────────────────────────── */
function splashStatus(msg) {
    const el = document.getElementById('splashStatus');
    if (el) el.textContent = msg;
}

async function init() {
    const splash = document.getElementById('splash');
    const startTime = Date.now();

    // Play melody as soon as possible
    playSplashMelody();

    try {
        splashStatus(t('splash_loading'));

        // Load config & platform
        const [config, platform, version] = await Promise.all([
            invoke('get_config'),
            invoke('get_platform'),
            invoke('get_app_version'),
        ]);

        state.config = config;
        state.platform = platform;
        $('#appVersion').textContent = 'v' + version;

        // Apply theme
        setTheme(config.theme || 'dark');

        // Load translations
        splashStatus(t('splash_translations'));
        await loadTranslations(config.language || 'fr');
        $('#langIcon').textContent = (config.language || 'fr') === 'fr' ? '🇫🇷' : '🇺🇸';

        // Bind events
        bindEvents();
        await setupListeners();

        // Init particle system
        auraParticles = new ParticleSystem('particleCanvas');

        // Check admin status
        checkAdmin();

        // ── Auto-scan during splash ──────────────────────────
        splashStatus(t('scanning'));

        // Wrap each invoke with a timeout to prevent splash freeze
        const withTimeout = (promise, ms) =>
            Promise.race([promise, new Promise((_, reject) => setTimeout(() => reject('timeout'), ms))]);

        const autoScanResults = await Promise.allSettled([
            withTimeout(invoke('get_health_score'), 15000),
            STRICT_PRIVACY_MODE ? Promise.resolve([]) : withTimeout(invoke('check_updates'), 130000),
            withTimeout(invoke('scan_cleanup'), 15000),
            withTimeout(invoke('get_startup_items'), 10000),
            withTimeout(invoke('get_heavy_processes'), 10000),
            withTimeout(invoke('get_system_vitals'), 10000),
        ]);

        // Health score
        if (autoScanResults[0].status === 'fulfilled') {
            renderHealthScore(autoScanResults[0].value);
        }
        // Updates
        if (autoScanResults[1].status === 'fulfilled') {
            state.updates = autoScanResults[1].value;
            renderUpdateList();
        }
        // Cleanup
        if (autoScanResults[2].status === 'fulfilled') {
            state.cleanup = autoScanResults[2].value;
            renderCleanupList();
        }
        // Startup items
        if (autoScanResults[3].status === 'fulfilled') {
            state.startupItems = autoScanResults[3].value;
            renderStartupList();
        }
        // Processes
        if (autoScanResults[4].status === 'fulfilled') {
            state.processes = autoScanResults[4].value;
            renderProcessList();
        }
        // System Vitals
        if (autoScanResults[5].status === 'fulfilled') {
            displayVitals(autoScanResults[5].value);
            applyHardwareDetection(autoScanResults[5].value);
        }

        // System info badges (non-blocking — runs after main scan)
        loadSystemSpecs();
        initBackupDir();
        if (!STRICT_PRIVACY_MODE) startupCheckAppUpdate();

        splashStatus(t('ready'));

        // Real-time temperature refresh every 10s (CPU-friendly)
        setInterval(refreshVitals, 10000);

    } catch (e) {
        console.error('Init error:', e);
        splashStatus(t('splash_error'));
    }

    // Minimum splash display time (1.5s) then fade out
    const elapsed = Date.now() - startTime;
    if (elapsed < 1500) {
        await new Promise(r => setTimeout(r, 1500 - elapsed));
    }

    if (splash) {
        splash.classList.add('fade-out');
        setTimeout(() => {
            splash.remove();
            startTutorial();
            checkPendingCrash();
            checkTempAlert();
        }, 650);
    }
}

/* ─── Temp Files Alert (> 1 GB) ────────────────────────────── */
const ONE_GB = 1073741824;

async function checkTempAlert() {
    try {
        const totalBytes = await invoke('check_temp_size');
        if (totalBytes >= ONE_GB) {
            $('#tempAlertSize').textContent = formatBytes(totalBytes);
            $('#tempAlertOverlay').classList.remove('hidden');
        }
    } catch (_) {}
}

function closeTempAlert() {
    $('#tempAlertOverlay').classList.add('hidden');
}

async function tempAlertClean() {
    closeTempAlert();
    switchTab('cleanup');
    try {
        const report = await invoke('scan_cleanup');
        state.cleanup = report;
        renderCleanupList();
        if (!report || !Array.isArray(report.items) || report.items.length === 0) {
            showToast(t('no_temp_to_clean') || t('no_bloatware'), 'success');
            return;
        }
        const paths = report.items.map(i => i.path);
        const freed = await invoke('run_cleanup', { paths });
        showToast((t('temp_alert_cleaned') || 'Nettoyage terminé') + ': ' + formatBytes(freed), 'success');
        state.cleanup = { items: [], total_bytes: 0 };
        renderCleanupList();
        refreshHealth();
    } catch (e) {
        showToast(t('error_clean') + ': ' + e, 'error');
    }
}

/* ─── Crash Report ─────────────────────────────────────────── */
async function checkPendingCrash() {
    try {
        const data = await invoke('check_pending_crash');
        if (data) {
            $('#crashOverlay').classList.remove('hidden');
        }
    } catch (_) {}
}

async function dismissCrash() {
    try { await invoke('clear_crash_report'); } catch (_) {}
    $('#crashOverlay').classList.add('hidden');
}

async function sendCrashReport() {
    const msg = ($('#crashUserMessage').value || '').trim();
    try {
        await invoke('send_crash_report', { userMessage: msg });
        showToast(t('crash_sent'), 'success');
    } catch (_) {
        showToast(t('crash_send_error'), 'error');
    }
    $('#crashOverlay').classList.add('hidden');
}

/* ─── Restore Warning Modal ────────────────────────────────── */
let pendingRestoreId = null;

function openRestoreWarning(snapshotId, description) {
    pendingRestoreId = snapshotId;
    $('#restoreOverlay').classList.remove('hidden');
}

function closeRestoreWarning() {
    pendingRestoreId = null;
    $('#restoreOverlay').classList.add('hidden');
}

/* ─── Licence Modal ────────────────────────────────────────── */
const LICENCE_FR = `CONTRAT DE LICENCE UTILISATEUR (CLUF) - AURA-UPDATE

Veuillez lire attentivement ce contrat de licence avant d'utiliser le logiciel Aura-Update.

1. PROPRIÉTÉ INTELLECTUELLE
Le logiciel "Aura-Update" est la propriété exclusive de la société "Aura Néo". Il est protégé par les lois françaises sur le droit d'auteur et les traités internationaux relatifs à la propriété intellectuelle.

2. CONCESSION DE LICENCE
Aura Néo vous concède une licence personnelle, non exclusive, non transférable et gratuite pour installer et utiliser le Logiciel sur vos machines (Windows et Linux) à des fins personnelles ou professionnelles.

3. RESTRICTIONS
Il est strictement interdit de copier, cloner, distribuer, modifier, désassembler ou décompiler le code source sans autorisation écrite explicite.

4. DISTRIBUTION
Le Logiciel est distribué gratuitement depuis le site officiel : https://www.auraneo.fr/aura-update/

5. GARANTIE ET RESPONSABILITÉ
Le Logiciel est fourni "TEL QUEL", sans garantie d'aucune sorte.

© 2025-2026 Aura Néo. Tous droits réservés.`;

const LICENCE_EN = `USER LICENSE AGREEMENT (EULA) - AURA-UPDATE

Please read this license agreement carefully before using the Aura-Update software.

1. INTELLECTUAL PROPERTY
The software "Aura-Update" is the exclusive property of the company "Aura Néo". It is protected by French copyright laws and international intellectual property treaties.

2. LICENSE GRANT
Aura Néo grants you a personal, non-exclusive, non-transferable, and free license to install and use the Software on your machines (Windows and Linux) for personal or professional purposes.

3. RESTRICTIONS
You are strictly prohibited from copying, cloning, distributing, modifying, disassembling, or reverse engineering the source code without explicit written permission.

4. DISTRIBUTION
The Software is distributed for free from the official website: https://www.auraneo.fr/aura-update/

5. WARRANTY AND LIABILITY
The Software is provided "AS IS", without warranty of any kind.

© 2025-2026 Aura Néo. All rights reserved.`;

function openLicence(lang) {
    const content = lang === 'fr' ? LICENCE_FR : LICENCE_EN;
    $('#licenceContent').textContent = content;
    $('#licenceOverlay').classList.remove('hidden');
}

/* ─── Backup Directory Picker ──────────────────────────────── */
async function chooseBackupDir() {
    try {
        const { open } = window.__TAURI__.dialog;
        const selected = await open({ directory: true, multiple: false, title: t('settings_backup_dir') });
        if (selected) {
            await saveConfigKey('backup_dir', selected);
            state.config.backup_dir = selected;
            syncBackupDirUI();
        }
    } catch (e) {
        showToast(t('error') + ': ' + e, 'error');
    }
}

async function resetBackupDir() {
    await saveConfigKey('backup_dir', '');
    state.config.backup_dir = '';
    syncBackupDirUI();
}

async function syncBackupDirUI() {
    const pathEl = $('#backupDirPath');
    const resetBtn = $('#btnResetBackupDir');
    const diskEl = $('#diskFreeSpace');

    if (state.config && state.config.backup_dir) {
        pathEl.textContent = state.config.backup_dir;
        resetBtn.classList.remove('hidden');
    } else {
        pathEl.textContent = t('backup_dir_default');
        resetBtn.classList.add('hidden');
    }

    // Check free disk space
    try {
        const freeBytes = await invoke('get_disk_free_space', { path: state.config.backup_dir || null });
        const freeGB = (freeBytes / (1024 * 1024 * 1024)).toFixed(1);
        diskEl.textContent = t('disk_free_space').replace('{0}', freeGB + ' GB');
        if (freeBytes < 5 * 1024 * 1024 * 1024) { // < 5 GB
            diskEl.textContent = t('disk_low_warning');
            diskEl.style.color = 'var(--danger)';
        } else {
            diskEl.style.color = '';
        }
    } catch (_) {
        diskEl.textContent = '';
    }
}

globalThis.manualCheckAppUpdate = manualCheckAppUpdate;

/* ═══════════════════════════════════════════════════════════
   MAINTENANCE AVANCÉE & MOTEURS
   ═══════════════════════════════════════════════════════════ */

/**
 * Lance une tâche de maintenance et gère l'affichage de la progression.
 * @param {string} taskId   - identifiant interne (ex: "update_git")
 * @param {string} command  - nom de la commande Tauri
 * @param {string} btnId    - id du bouton déclencheur
 */
async function runMaintenanceTask(taskId, command, btnId) {
    const btn = $(`#${btnId}`);
    const progEl = $(`#prog-${taskId.replace(/_/g, '-')}`);
    const outEl = $(`#out-${taskId.replace(/_/g, '-')}`);
    const card = btn.closest('.maintenance-card');

    // Désactiver le bouton pendant l'exécution
    btn.disabled = true;
    btn.textContent = '⏳';
    card.classList.remove('done', 'error');

    // Afficher la zone de progression
    progEl.classList.remove('hidden');
    outEl.textContent = t('maintenance_running') || 'En cours…';

    try {
        const result = await invoke(command);
        outEl.textContent = result || t('maintenance_done') || 'Terminé.';
        card.classList.add('done');
        showToast(t('maintenance_done') || 'Opération terminée.', 'success');
    } catch (err) {
        outEl.textContent = String(err);
        card.classList.add('error');
        showToast(String(err), 'error');
    } finally {
        btn.disabled = false;
        btn.textContent = t('btn_run') || 'Exécuter';
        // Stopper l'animation indeterminate
        const fill = progEl.querySelector('.maintenance-progress-fill');
        if (fill) fill.classList.remove('indeterminate');
    }
}

init();
