const { exec } = require('child_process');
const os = require('os');
const logger = require('./LogManager');

class UpdateManager {
    constructor() {
        this.platform = os.platform(); // 'win32' ou 'linux'
        this.manager = null;
        this.sourceUpdated = false; 
        this.detectManager();
    }

    /**
     * Détecte le gestionnaire de paquets disponible sur le système
     */
    async detectManager() {
        if (this.platform === 'win32') {
            this.manager = 'winget';
        } else {
            // Vérification en cascade pour Linux (Debian -> Fedora -> Arch)
            if (await this.checkCommand('apt-get')) this.manager = 'apt';
            else if (await this.checkCommand('dnf')) this.manager = 'dnf';
            else if (await this.checkCommand('pacman')) this.manager = 'pacman';
        }
        logger.info(`[UpdateManager] Plateforme: ${this.platform}, Gestionnaire: ${this.manager || 'Aucun'}`);
    }

    checkCommand(cmd) {
        return new Promise(resolve => {
            exec(cmd + ' --version', (err) => resolve(!err));
        });
    }

    /**
     * Point d'entrée principal pour récupérer les mises à jour
     */
    async getUpdates() {
        logger.info("[UpdateManager] Recherche de mises à jour complète...");
        
        if (!this.manager) await this.detectManager();

        if (this.platform === 'win32') {
            // Sur Windows, on lance les deux recherches en parallèle pour gagner du temps
            // 1. Winget (Apps + Drivers)
            // 2. Windows Update (OS + Security)
            try {
                const [wingetUpdates, sysUpdates] = await Promise.all([
                    this.getWingetUpdatesWrapper(),
                    this.getWindowsSystemUpdates()
                ]);
                
                // Fusion des résultats
                const allUpdates = [...sysUpdates, ...wingetUpdates];
                logger.info(`[UpdateManager] Total trouvé: ${allUpdates.length} (Winget: ${wingetUpdates.length}, System: ${sysUpdates.length})`);
                return allUpdates;
            } catch (error) {
                logger.error("[UpdateManager] Erreur lors de la recherche Windows: " + error);
                return [];
            }
        } else {
            return this.getLinuxUpdates();
        }
    }

    // Wrapper pour gérer l'update des sources Winget une seule fois
    async getWingetUpdatesWrapper() {
        if (!this.sourceUpdated) {
            logger.info("[UpdateManager] Actualisation sources Winget...");
            try {
                // Timeout 15s pour éviter le blocage si pas d'internet
                await new Promise(r => exec('winget source update', { timeout: 15000 }, r));
                this.sourceUpdated = true;
            } catch(e) { logger.warn("[UpdateManager] Timeout source update"); }
        }
        return this.getWingetUpdates();
    }

    getWingetUpdates() {
        return new Promise((resolve) => {
            // CHCP 65001 : Force l'encodage UTF-8 (Vital pour parser correctement sur tous les PC)
            // --include-unknown : Vital pour voir les drivers (Intel, Nvidia, etc.) qui n'ont pas toujours de version propre
            const cmd = 'chcp 65001 >NUL && winget upgrade --include-unknown --disable-interactivity --accept-source-agreements';
            
            exec(cmd, { encoding: 'utf8', maxBuffer: 10 * 1024 * 1024 }, (error, stdout, stderr) => {
                const updates = [];
                // Nettoyage des retours à la ligne Windows/Unix
                const lines = stdout.replace(/\r\n/g, '\n').split('\n');

                // Regex Universelle : Cherche 3 colonnes ou plus séparées par 2 espaces ou plus.
                const lineRegex = /^(.+?)\s{2,}([^\s]+)\s{2,}([^\s]+)\s{2,}([^\s]+)/;

                lines.forEach((line) => {
                    const l = line.trim();
                    if (!l || l.startsWith('-')) return;

                    const match = l.match(lineRegex);
                    if (match) {
                        const name = match[1].trim();
                        const id = match[2].trim();
                        const current = match[3].trim();
                        const available = match[4].trim();

                        if (id.toLowerCase() === 'id') return;

                        if (name && id) {
                            updates.push({
                                name: name,
                                id: id,
                                currentVersion: current,
                                newVersion: available,
                                manager: 'winget'
                            });
                        }
                    }
                });
                
                resolve(updates);
            });
        });
    }

    /**
     * Récupère les mises à jour Windows Update via PowerShell et l'objet COM
     */
    getWindowsSystemUpdates() {
        return new Promise((resolve) => {
            logger.info("[UpdateManager] Recherche Windows Updates (COM)...");
            // Script PowerShell optimisé pour interroger l'agent Windows Update natif
            const psScript = `
                try {
                    $s = (New-Object -ComObject Microsoft.Update.Session).CreateUpdateSearcher()
                    $s.ServerSelection = 0 # Default
                    $r = $s.Search("IsInstalled=0")
                    $r.Updates | ForEach-Object { Write-Output ($_.Title + "||" + $_.Identity.UpdateID) }
                } catch { exit 1 }
            `;

            exec(`powershell -NoProfile -Command "${psScript.replace(/\r?\n/g, '; ')}"`, { timeout: 60000 }, (err, stdout) => {
                const updates = [];
                if (!err && stdout) {
                    const lines = stdout.toString().split('\n');
                    lines.forEach(line => {
                        const parts = line.trim().split('||');
                        if (parts.length === 2) {
                            updates.push({
                                name: parts[0], // ex: "2024-01 Mise à jour cumulative..."
                                id: parts[1],   // UUID de l'update
                                currentVersion: 'System',
                                newVersion: 'Update',
                                manager: 'windows-update'
                            });
                        }
                    });
                }
                resolve(updates);
            });
        });
    }

    getLinuxUpdates() {
        return new Promise((resolve) => {
            let cmd = '';
            
            // Adaptation de la commande selon la distribution
            if (this.manager === 'apt') cmd = 'apt list --upgradable';
            else if (this.manager === 'dnf') cmd = 'dnf check-update';
            else if (this.manager === 'pacman') cmd = 'checkupdates';

            if (!cmd) { resolve([]); return; }

            exec(cmd, (err, stdout) => {
                const updates = [];
                const lines = stdout.split('\n');

                if (this.manager === 'apt') {
                    lines.forEach(line => {
                        if (line.includes('/')) {
                            const parts = line.split('/');
                            updates.push({
                                name: parts[0],
                                id: parts[0],
                                currentVersion: '?', 
                                newVersion: 'latest',
                                manager: 'apt'
                            });
                        }
                    });
                } 
                else if (this.manager === 'dnf') {
                    lines.forEach(line => {
                        const l = line.trim();
                        if(!l || l.startsWith('Last metadata')) return;
                        const parts = l.split(/\s+/);
                        if (parts.length >= 2) {
                             updates.push({
                                name: parts[0],
                                id: parts[0],
                                currentVersion: '?',
                                newVersion: parts[1],
                                manager: 'dnf'
                            });
                        }
                    });
                }
                else if (this.manager === 'pacman') {
                    lines.forEach(line => {
                        const l = line.trim();
                        const parts = l.split(/\s+/);
                        if (parts.length >= 4 && parts[2] === '->') {
                            updates.push({
                                name: parts[0],
                                id: parts[0],
                                currentVersion: parts[1],
                                newVersion: parts[3],
                                manager: 'pacman'
                            });
                        }
                    });
                }
                logger.info(`[UpdateManager] Linux (${this.manager}): ${updates.length} mises à jour.`);
                resolve(updates);
            });
        });
    }

    updatePackage(pkg, callback) {
        let cmd = '';
        
        switch (pkg.manager) {
            case 'winget':
                cmd = `winget upgrade --id "${pkg.id}" --accept-package-agreements --accept-source-agreements --silent --disable-interactivity --force --include-unknown`;
                break;
            case 'windows-update':
                // Utilise USOClient pour déclencher l'installation native Windows Update
                // C'est plus sûr que d'essayer d'installer via PowerShell COM qui requiert des droits très élevés et complexes
                cmd = `powershell -Command "usoclient StartInstall"`;
                break;
            case 'apt':
                cmd = `apt-get install --only-upgrade -y ${pkg.id}`;
                break;
            case 'dnf':
                cmd = `dnf upgrade -y ${pkg.id}`;
                break;
            case 'pacman':
                cmd = `pacman -S --noconfirm ${pkg.id}`;
                break;
        }

        if (!cmd) {
            callback({ status: 'error', message: 'Gestionnaire non supporté' });
            return;
        }

        logger.info(`[UpdateManager] Exécution (${pkg.manager}): ${cmd}`);
        
        // Cas spécial Windows Update (Asynchrone système)
        if (pkg.manager === 'windows-update') {
            exec(cmd, (err) => {
                if (!err) {
                    callback({ status: 'finished', message: 'Installation système lancée (Vérifiez Windows Update)' });
                } else {
                    callback({ status: 'error', message: 'Echec lancement Windows Update' });
                }
            });
            return;
        }

        const child = exec(cmd);
        
        child.stdout.on('data', (data) => {
            const msg = data.toString().replace(/[\x00-\x1F\x7F-\x9F]/g, "").trim();
            if (msg.length > 0) callback({ status: 'running', message: msg.substring(0, 60) + "..." });
        });

        child.stderr.on('data', (data) => {
             const msg = data.toString().trim();
             if (msg.length > 0) callback({ status: 'running', message: "Installation..." });
        });

        child.on('close', (code) => {
            if (code === 0) {
                callback({ status: 'finished', message: 'Mise à jour terminée' });
            } else {
                if (pkg.manager === 'winget' && (code === 2316632084 || code === -1978335212)) {
                    callback({ status: 'error', message: `Erreur Hash (Réessayez)` });
                } else {
                    callback({ status: 'error', message: `Erreur (Code ${code})` });
                }
            }
        });
    }
}

module.exports = new UpdateManager();
