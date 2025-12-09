const { app, BrowserWindow, ipcMain, Menu, shell, dialog } = require('electron');
const path = require('path');
const { exec, spawn } = require('child_process');
const fs = require('fs');
const os = require('os');
const updateManager = require('../managers/UpdateManager');
const logger = require('../managers/LogManager');
const locManager = require('../managers/LocManager');

let mainWindow;

// --- Dictionnaires de traduction pour le Menu ---
const MENU_TEXTS = {
    fr: {
        file: 'Fichier', logs: 'Ouvrir dossier des Logs', quit: 'Quitter',
        tools: 'Outils', cleanTemp: 'Nettoyer Fichiers Temporaires (Système)', cleanApp: 'Nettoyer Cache & Logs (Application)',
        view: 'Affichage', reload: 'Recharger', forceReload: 'Forcer Rechargement', devTools: 'Outils de développement', resetZoom: 'Taille normale', zoomIn: 'Zoom avant', zoomOut: 'Zoom arrière', fullScreen: 'Plein écran',
        help: 'Aide', webSite: 'Site Web Aura Neo', webPage: 'Page Aura Update', contact: 'Contacter le Support', about: 'Info / Nouveautés'
    },
    en: {
        file: 'File', logs: 'Open Logs Folder', quit: 'Quit',
        tools: 'Tools', cleanTemp: 'Clean Temporary Files (System)', cleanApp: 'Clean Cache & Logs (App)',
        view: 'View', reload: 'Reload', forceReload: 'Force Reload', devTools: 'Toggle Developer Tools', resetZoom: 'Actual Size', zoomIn: 'Zoom In', zoomOut: 'Zoom Out', fullScreen: 'Toggle Full Screen',
        help: 'Help', webSite: 'Aura Neo Website', webPage: 'Aura Update Page', contact: 'Contact Support', about: 'Info / What\'s New'
    }
};

// --- Logique de Nettoyage ---

async function cleanDirectory(dirPath, isSystemTemp = false) {
    let deletedCount = 0;
    let errorCount = 0;

    if (!fs.existsSync(dirPath)) return { deleted: 0, errors: 0 };

    try {
        const files = await fs.promises.readdir(dirPath);
        for (const file of files) {
            const curPath = path.join(dirPath, file);
            try {
                if (isSystemTemp) {
                    const stats = await fs.promises.stat(curPath);
                    const now = new Date().getTime();
                    const endTime = new Date(stats.mtime).getTime() + 3600000; // 1h de sécurité
                    if (now < endTime) continue;
                }
                await fs.promises.rm(curPath, { recursive: true, force: true });
                deletedCount++;
            } catch (e) { errorCount++; }
        }
    } catch (e) { logger.error(`Erreur lecture dossier ${dirPath}: ${e.message}`); }
    return { deleted: deletedCount, errors: errorCount };
}

async function performSystemClean() {
    const tempDir = os.tmpdir();
    logger.info(`[Clean] Démarrage nettoyage système: ${tempDir}`);
    const result = await cleanDirectory(tempDir, true);
    
    dialog.showMessageBox(mainWindow, {
        type: 'info', title: 'System Clean',
        message: `Nettoyage terminé.\nFichiers supprimés: ${result.deleted}\nIgnorés: ${result.errors}`,
        buttons: ['OK']
    });
}

async function performAppClean() {
    const logDir = path.dirname(logger.logFile);
    try { fs.truncateSync(logger.logFile, 0); } catch(e) {}
    await cleanDirectory(logDir);

    const appDataPath = app.getPath('userData');
    await cleanDirectory(path.join(appDataPath, 'Cache'));
    await cleanDirectory(path.join(appDataPath, 'GPUCache'));

    dialog.showMessageBox(mainWindow, {
        type: 'info', title: 'App Clean',
        message: 'Logs et cache vidés.', buttons: ['OK']
    });
}

// --- Menu Dynamique ---

function rebuildMenu(lang = 'fr') {
    const t = MENU_TEXTS[lang] || MENU_TEXTS['en']; // Fallback EN

    const template = [
        {
            label: t.file,
            submenu: [
                { 
                    label: t.logs, 
                    click: () => shell.openPath(path.dirname(logger.logFile)) 
                },
                { type: 'separator' },
                { label: t.quit, role: 'quit' }
            ]
        },
        {
            label: t.tools,
            submenu: [
                { label: t.cleanTemp, click: () => performSystemClean() },
                { label: t.cleanApp, click: () => performAppClean() }
            ]
        },
        {
            label: t.view,
            submenu: [
                { role: 'reload', label: t.reload },
                { role: 'forceReload', label: t.forceReload },
                { role: 'toggleDevTools', label: t.devTools },
                { type: 'separator' },
                { role: 'resetZoom', label: t.resetZoom },
                { role: 'zoomIn', label: t.zoomIn },
                { role: 'zoomOut', label: t.zoomOut },
                { type: 'separator' },
                { role: 'togglefullscreen', label: t.fullScreen }
            ]
        },
        {
            label: t.help,
            submenu: [
                { label: t.webSite, click: async () => { await shell.openExternal('https://www.auraneo.fr/'); } },
                { label: t.webPage, click: async () => { await shell.openExternal('https://www.auraneo.fr/aura-update/'); } },
                { label: t.contact, click: async () => { await shell.openExternal('mailto:contact@auraneo.fr'); } },
                { type: 'separator' },
                { 
                    label: t.about, 
                    click: () => showAboutDialog()
                }
            ]
        }
    ];

    const menu = Menu.buildFromTemplate(template);
    Menu.setApplicationMenu(menu);
}

function showAboutDialog() {
    const appVersion = app.getVersion();
    let changelogText = "Notes de version non trouvées.";

    try {
        const pathsToTry = [
            path.join(app.getAppPath(), 'CHANGELOG.md'),
            path.join(process.resourcesPath, 'CHANGELOG.md'),
            path.join(path.dirname(app.getPath('exe')), 'CHANGELOG.md')
        ];
        
        let content = null;
        for (const p of pathsToTry) {
            if (fs.existsSync(p)) {
                content = fs.readFileSync(p, 'utf8');
                break;
            }
        }

        if (content) {
            // Regex dynamique pour la version actuelle
            const escapedVersion = appVersion.replace(/\./g, '\\.');
            const dynamicRegex = new RegExp(`##\\s+\\[v?${escapedVersion}\\][\\s\\S]*?(?=##\\s+\\[|$)`, 'i');
            const match = content.match(dynamicRegex);
            
            if (match) {
                changelogText = match[0].trim();
            } else {
                // Regex de secours pour le premier bloc trouvé
                const fallbackRegex = /##\s+\[v?[\d\.]+\][\s\S]*?(?=##\s+\[|$)/;
                const firstBlock = content.match(fallbackRegex);
                if (firstBlock) changelogText = "Version courante non trouvée dans le log. Dernier changelog :\n" + firstBlock[0];
            }
        }
    } catch (e) {
        logger.error("Erreur lecture CHANGELOG: " + e.message);
    }

    dialog.showMessageBox(mainWindow, {
        type: 'info',
        title: `À propos`,
        message: `Aura Update v${appVersion}`,
        detail: changelogText,
        buttons: ['OK']
    });
}

// --- Fenêtre Principale ---

function createWindow() {
    rebuildMenu('fr'); // Init en Français par défaut

    mainWindow = new BrowserWindow({
        width: 1200, height: 800,
        icon: path.join(__dirname, '..', '..', 'ui', 'favicon.ico'),
        webPreferences: {
            preload: path.join(__dirname, 'preload.js'),
            nodeIntegration: false, contextIsolation: true, sandbox: false 
        },
        show: false, backgroundColor: '#0b0f13'
    });

    mainWindow.loadFile(path.join(__dirname, '..', '..', 'ui', 'index.html'));
    mainWindow.on('ready-to-show', () => mainWindow.show());
}


// --- IPC ---

ipcMain.handle('get-translations', () => locManager.getTranslations());

ipcMain.handle('set-language', (event, lang) => {
    rebuildMenu(lang);
    return locManager.setLanguage(lang);
});

ipcMain.handle('get-app-version', () => app.getVersion());

ipcMain.handle('is-admin', async () => {
    return new Promise((resolve) => {
        if (process.platform === 'win32') exec('net session', (err) => resolve(!err));
        else exec('id -u', (err, stdout) => resolve(stdout && parseInt(stdout.trim()) === 0));
    });
});

ipcMain.handle('elevate', () => {
    if (process.platform === 'win32') {
        const vbsPath = path.join(os.tmpdir(), `aura_elevate_${Date.now()}.vbs`);
        const vbsContent = `Set UAC = CreateObject("Shell.Application")\nUAC.ShellExecute "${process.execPath}", "${process.argv.slice(1).map(a=>`\"${path.resolve(a)}\"`).join(' ')}", "${process.cwd()}", "runas", 1`;
        try {
            fs.writeFileSync(vbsPath, vbsContent, { encoding: 'utf8' });
            const child = spawn('wscript.exe', [vbsPath], { detached: true, stdio: 'ignore' });
            child.unref();
            setTimeout(() => app.quit(), 1000);
            return { success: true };
        } catch (e) { return { success: false, message: e.message }; }
    }
    return { success: false, message: "Linux: Use sudo" };
});

ipcMain.handle('check-updates', async () => await updateManager.getUpdates());

ipcMain.handle('update-package', (event, pkg) => {
    updateManager.updatePackage(pkg, (progress) => {
        mainWindow.webContents.send('update-progress', { id: pkg.id, ...progress });
    });
    return { started: true };
});

ipcMain.handle('update-all', async () => {
    const updates = await updateManager.getUpdates();
    if (updates.length === 0) {
        mainWindow.webContents.send('update-progress', { id: 'all', status: 'finished', message: 'Aucune mise à jour.' });
        return { started: true };
    }
    let i = 0;
    const processNext = () => {
        if (i >= updates.length) {
            mainWindow.webContents.send('update-progress', { id: 'all', status: 'finished', message: 'Terminé' });
            return;
        }
        const pkg = updates[i];
        mainWindow.webContents.send('update-progress', { id: 'all', status: 'running', message: `Update ${pkg.name}...`, percent: (i/updates.length)*100 });
        updateManager.updatePackage(pkg, (prog) => {
            if (prog.status === 'finished' || prog.status === 'error') {
                i++;
                processNext();
            }
        });
    };
    processNext();
    return { started: true };
});

app.whenReady().then(createWindow);
app.on('window-all-closed', () => { if (process.platform !== 'darwin') app.quit(); });
app.on('activate', () => { if (BrowserWindow.getAllWindows().length === 0) createWindow(); });