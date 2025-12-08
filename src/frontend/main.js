const { app, BrowserWindow, ipcMain } = require('electron');
const path = require('path');
const { exec, spawn } = require('child_process');
const fs = require('fs');
const os = require('os');
const updateManager = require('../managers/UpdateManager');
const logger = require('../managers/LogManager');
const locManager = require('../managers/LocManager');

let mainWindow;

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1200,
    height: 800,
    icon: path.join(__dirname, '..', '..', 'ui', 'favicon.ico'),
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      nodeIntegration: false,
      contextIsolation: true,
      sandbox: false 
    },
    show: false,
    backgroundColor: '#0b0f13'
  });

  mainWindow.loadFile(path.join(__dirname, '..', '..', 'ui', 'index.html'));

  mainWindow.on('ready-to-show', () => {
    mainWindow.show();
    logger.info("Fenêtre principale affichée");
  });
}

// --- Gestion des API IPC ---

ipcMain.handle('get-translations', () => {
    return locManager.getTranslations();
});

ipcMain.handle('set-language', (event, lang) => {
    return locManager.setLanguage(lang);
});

ipcMain.handle('is-admin', async () => {
    return new Promise((resolve) => {
        if (process.platform === 'win32') {
            exec('net session', (err) => {
                resolve(!err);
            });
        } else {
            exec('id -u', (err, stdout) => {
                resolve(stdout && parseInt(stdout.trim()) === 0);
            });
        }
    });
});

ipcMain.handle('elevate', () => {
    logger.info("Demande d'élévation de privilèges (Méthode VBS)...");
    
    if (process.platform === 'win32') {
        const targetExe = process.execPath;
        const targetArgs = process.argv.slice(1).map(a => `"${path.resolve(a)}"`).join(' ');
        const workingDir = process.cwd();
        
        // Création d'un script VBS temporaire pour lancer le processus en Admin
        // C'est souvent plus fiable que PowerShell Start-Process direct
        const vbsPath = path.join(os.tmpdir(), `aura_elevate_${Date.now()}.vbs`);
        const vbsContent = `
            Set UAC = CreateObject("Shell.Application")
            UAC.ShellExecute "${targetExe}", "${targetArgs.replace(/"/g, '""')}", "${workingDir}", "runas", 1
        `;

        try {
            fs.writeFileSync(vbsPath, vbsContent, { encoding: 'utf8' });
            
            // Exécution du VBS via wscript (non bloquant)
            const child = spawn('wscript.exe', [vbsPath], {
                detached: true,
                stdio: 'ignore'
            });
            child.unref();

            // Nettoyage différé du fichier VBS
            setTimeout(() => {
                try { if(fs.existsSync(vbsPath)) fs.unlinkSync(vbsPath); } catch(e){}
            }, 5000);

            // Fermeture de l'app actuelle
            setTimeout(() => app.quit(), 1000);
            return { success: true, message: "Redémarrage en cours..." };

        } catch (e) {
            logger.error("Erreur création VBS: " + e.message);
            return { success: false, message: "Erreur interne: " + e.message };
        }
    } else {
        return { success: false, message: "Non supporté automatiquement sur Linux. Lancez avec sudo." };
    }
});

ipcMain.handle('check-updates', async () => {
    try {
        const updates = await updateManager.getUpdates();
        return updates;
    } catch (e) {
        logger.error("Erreur check-updates: " + e.message);
        return [];
    }
});

ipcMain.handle('update-package', (event, pkg) => {
    updateManager.updatePackage(pkg, (progress) => {
        mainWindow.webContents.send('update-progress', {
            id: pkg.id,
            ...progress
        });
    });
    return { started: true };
});

ipcMain.handle('update-all', async () => {
    try {
        const updates = await updateManager.getUpdates();
        let i = 0;
        
        const processNext = () => {
            if (i >= updates.length) {
                mainWindow.webContents.send('update-progress', { id: 'all', status: 'finished', message: 'Tout est terminé' });
                return;
            }
            const pkg = updates[i];
            
            const percentStart = (i / updates.length) * 100;
            mainWindow.webContents.send('update-progress', { id: 'all', status: 'running', message: `Mise à jour de ${pkg.name}...`, percent: percentStart });

            updateManager.updatePackage(pkg, (prog) => {
                if (prog.status === 'finished' || prog.status === 'error') {
                    i++;
                    const percentEnd = (i / updates.length) * 100;
                    mainWindow.webContents.send('update-progress', { id: 'all', status: 'running', message: `${pkg.name} terminé.`, percent: percentEnd });
                    
                    processNext();
                }
            });
        };

        if (updates.length > 0) {
            processNext();
        } else {
            mainWindow.webContents.send('update-progress', { id: 'all', status: 'finished', message: 'Aucune mise à jour à faire.' });
        }
        
        return { started: true };

    } catch (e) {
        logger.error("Erreur update-all: " + e);
        return { error: e.message };
    }
});

app.whenReady().then(createWindow);

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});

app.on('activate', () => {
  if (BrowserWindow.getAllWindows().length === 0) createWindow();
});