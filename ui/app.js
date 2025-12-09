// Eléments UI
const statusEl = document.getElementById('status');
const statusTitle = document.getElementById('statusTitle');
const statusDesc = document.getElementById('statusDesc');
const statusIcon = document.getElementById('statusIcon');
const listEl = document.getElementById('list');
const refreshBtn = document.getElementById('refresh');
const updateAllBtn = document.getElementById('updateAll');
const adminModeBtn = document.getElementById('adminMode');
const adminWarning = document.getElementById('adminWarning');
const elevateBtn = document.getElementById('elevateBtn');
const progressSection = document.getElementById('progress');
const progressBar = document.getElementById('progressBar');
const progressLabel = document.getElementById('progressLabel');
const progressPercent = document.getElementById('progressPercent');
const progressMessage = document.getElementById('progressMessage');
const langToggle = document.getElementById('langToggle');

// Textes traduits
let translations = {};
let currentLang = 'fr';
let isAdmin = false;

// Initialisation
window.onload = async () => {
    // 1. Récupération et affichage de la version
    try {
        const version = await window.api.getAppVersion();
        document.getElementById('headerSub').textContent = `v${version}`;
    } catch (e) {
        console.error("Erreur version:", e);
    }

    // Charger la langue sauvegardée ou par défaut
    translations = await window.api.getTranslations();
    
    // Détection basique de la langue courante via une clé spécifique
    // (Une méthode plus propre serait que getTranslations retourne aussi la langue)
    if (translations.btn_refresh === "🔄 Check for Updates") currentLang = 'en';
    
    applyTranslations();
    await checkAdmin();
    refreshUpdates();
};

// Gestion Langue
langToggle.onclick = async () => {
    const newLang = currentLang === 'fr' ? 'en' : 'fr';
    translations = await window.api.setLanguage(newLang);
    currentLang = newLang;
    applyTranslations();
    // Rafraichir les textes dynamiques (Admin, Status)
    checkAdmin();
    // Rafraichir la liste si elle est affichée pour traduire les boutons
    if (listEl.childElementCount > 0) {
        // On relance juste l'affichage (sans rescanner le réseau)
        // Mais comme on ne stocke pas les updates en global ici (dans cette version simplifiée), 
        // on fait un refreshUpdates() complet ou on manipule le DOM.
        // Option simple : changer le texte des boutons existants
        document.querySelectorAll('.item button').forEach(btn => {
            btn.textContent = translations.btn_update;
        });
    }
};

function applyTranslations() {
    langToggle.textContent = currentLang === 'fr' ? '🇫🇷' : '🇺🇸';
    document.documentElement.lang = currentLang;

    document.getElementById('appTitle').textContent = translations.app_title;
    document.getElementById('headerTitle').textContent = translations.app_title;
    
    refreshBtn.textContent = translations.btn_refresh;
    updateAllBtn.querySelector('span').textContent = translations.btn_update_all;
    elevateBtn.textContent = translations.btn_elevate;
    
    document.getElementById('warnTitle').textContent = translations.admin_warning_title;
    document.getElementById('warnDesc').textContent = translations.admin_warning_desc;
    
    document.getElementById('adminText').textContent = isAdmin ? translations.admin_active : translations.admin_inactive;

    // Status (si pas de recherche active)
    if (statusIcon.textContent === '👋') {
        statusTitle.textContent = translations.status_welcome;
        statusDesc.textContent = translations.status_welcome_desc;
    } else if (statusIcon.textContent === '✅') {
        statusTitle.textContent = translations.status_uptodate;
    }
}

// Vérification Admin
async function checkAdmin() {
    isAdmin = await window.api.isAdmin();
    if (isAdmin) {
        adminModeBtn.classList.add('active');
        adminWarning.style.display = 'none';
        document.getElementById('adminText').textContent = translations.admin_active;
    } else {
        adminModeBtn.classList.remove('active');
        adminWarning.style.display = 'flex';
        document.getElementById('adminText').textContent = translations.admin_inactive;
    }
}

// Rafraichir la liste
async function refreshUpdates() {
    statusIcon.textContent = '🔄';
    statusTitle.textContent = translations.status_checking;
    statusDesc.textContent = '';
    statusEl.style.display = 'block';
    
    listEl.innerHTML = '';
    updateAllBtn.disabled = true;

    const updates = await window.api.checkUpdates();

    if (!updates || updates.length === 0) {
        statusIcon.textContent = '✅';
        statusTitle.textContent = translations.status_uptodate;
        return;
    }

    statusEl.style.display = 'none';
    updateAllBtn.disabled = false;

    updates.forEach(pkg => {
        const item = document.createElement('div');
        item.className = 'item';
        item.innerHTML = `
            <div class="meta">
                <b>${pkg.name}</b>
                <span class="small">${pkg.id} • v${pkg.currentVersion} ➜ v${pkg.newVersion}</span>
            </div>
            <button onclick="launchUpdate('${pkg.id}', '${pkg.manager}')">${translations.btn_update}</button>
        `;
        listEl.appendChild(item);
    });
}

// Lancer une update unitaire
window.launchUpdate = async (id, manager) => {
    showProgress(true, `${translations.progress_init} (${id})`, true);
    await window.api.updatePackage({ id, manager });
};

// Tout mettre à jour
updateAllBtn.onclick = async () => {
    if(!confirm(translations.confirm_update_all)) return;
    showProgress(true, translations.progress_init, false);
    await window.api.updateAll();
};

refreshBtn.onclick = refreshUpdates;
elevateBtn.onclick = () => window.api.elevate();
adminModeBtn.onclick = () => { if(!isAdmin) window.api.elevate(); };

// Gestion de la progression (Events IPC)
window.api.onProgress((data) => {
    if (data.status === 'finished') {
        if (data.id === 'all' || !data.id) {
            showProgress(false);
            refreshUpdates();
        } else {
            progressMessage.textContent = `${data.id}: ${translations.progress_finished}`;
        }
    } else if (data.status === 'error') {
        progressMessage.textContent = `Error: ${data.message}`;
        progressMessage.style.color = 'var(--error)';
        setTimeout(() => showProgress(false), 4000);
    } else {
        // Running
        if (data.percent !== undefined) {
             showProgress(true, null, false);
             progressBar.style.width = data.percent + '%';
             progressPercent.textContent = Math.round(data.percent) + '%';
             progressPercent.style.display = 'inline';
        }
        
        if (data.message) {
            progressMessage.textContent = data.message;
        }
    }
});

function showProgress(show, message, indeterminate = false) {
    if (show) {
        progressSection.style.display = 'block';
        if (message) progressMessage.textContent = message;
        
        if (indeterminate) {
            progressBar.classList.add('indeterminate');
            progressBar.style.width = '100%';
            progressPercent.style.display = 'none';
        } else {
            progressBar.classList.remove('indeterminate');
        }
        progressMessage.style.color = 'var(--muted)';
    } else {
        progressSection.style.display = 'none';
        progressBar.style.width = '0%';
        progressBar.classList.remove('indeterminate');
    }
}
