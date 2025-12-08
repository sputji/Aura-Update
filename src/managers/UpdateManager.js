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

    detectManager() {
        if (this.platform === 'win32') {
            this.manager = 'winget';
        } else {
            this.checkCommand('apt-get').then(exists => { if(exists) this.manager = 'apt'; });
            this.checkCommand('dnf').then(exists => { if(exists) this.manager = 'dnf'; });
            this.checkCommand('pacman').then(exists => { if(exists) this.manager = 'pacman'; });
        }
        logger.info(`Plateforme: ${this.platform}, Gestionnaire: ${this.manager}`);
    }

    checkCommand(cmd) {
        return new Promise(resolve => {
            exec(cmd + ' --version', (err) => resolve(!err));
        });
    }

    async getUpdates() {
        logger.info("Recherche de mises à jour...");
        
        if (this.platform === 'win32') {
            if (!this.sourceUpdated) {
                logger.info("Mise à jour des sources Winget...");
                try {
                    await new Promise(r => exec('winget source update', r));
                    this.sourceUpdated = true;
                } catch(e) { logger.warn("Echec update sources: " + e); }
            }
            return this.getWingetUpdates();
        } else {
            return this.getLinuxUpdates();
        }
    }

    getWingetUpdates() {
        return new Promise((resolve, reject) => {
            const cmd = 'winget upgrade --include-unknown --disable-interactivity --accept-source-agreements';
            
            exec(cmd, { encoding: 'utf8', maxBuffer: 1024 * 1024 * 5 }, (error, stdout, stderr) => {
                const updates = [];
                const lines = stdout.split('\n');
                
                // Détection plus permissive de l'en-tête
                let headerIndex = -1;
                let idxId = -1;
                let idxVersion = -1;
                let idxAvailable = -1;
                
                // On cherche une ligne contenant au moins "Nom" (ou Name) et "Id"
                for(let i=0; i<Math.min(lines.length, 10); i++) { // On cherche dans les 10 premières lignes
                    const line = lines[i].toLowerCase();
                    if ((line.includes('nom') || line.includes('name')) && (line.includes(' id'))) {
                        headerIndex = i;
                        const header = lines[i];
                        
                        // Utilisation de indexOf pour la robustesse
                        idxId = header.toLowerCase().indexOf(' id'); 
                        if (idxId !== -1) idxId += 1; // +1 pour passer l'espace
                        
                        // On cherche Version ou Current
                        idxVersion = header.search(/(version|current)/i);
                        
                        // On cherche Disponible ou Available
                        idxAvailable = header.search(/(disponible|available)/i);
                        
                        break;
                    }
                }

                lines.forEach((line, index) => {
                    if (index <= headerIndex) return; // Skip headers
                    const l = line.trimEnd(); // On garde les espaces de début pour le parsing par colonne
                    if (!l || l.startsWith('---')) return;
                    
                    let name, id, current, available;

                    // Si on a détecté les colonnes, on découpe précisément
                    if (idxId > 0 && idxVersion > 0) {
                        // Nom: du début jusqu'à ID
                        name = line.substring(0, idxId).trim();
                        // ID: de ID jusqu'à Version
                        id = line.substring(idxId, idxVersion).trim();
                        
                        // Si l'ID contient des "...", il est tronqué, on ne peut rien faire de fiable
                        if (id.includes('…') || id.endsWith('...')) {
                            // On tente de le récupérer via regex en dernier recours
                            const match = l.match(/\s+([^\s]+)\s+\d+\./); // cherche un mot suivi d'une version
                            if (match) id = match[1];
                            else return;
                        }

                        // Version et Available
                        if (idxAvailable > 0) {
                            current = line.substring(idxVersion, idxAvailable).trim();
                            available = line.substring(idxAvailable).trim().split(' ')[0];
                        } else {
                            const rest = line.substring(idxVersion).trim().split(/\s{2,}/);
                            current = rest[0];
                            available = rest[1] || "";
                        }
                    } else {
                        // Fallback Regex (Ancienne méthode)
                        // Nom (peut contenir espaces)  ID (sans espace)  Version  Available
                        const match = l.match(/^(.+?)\s{2,}([^\s]+)\s{2,}([^\s]+)\s{2,}([^\s]+)/);
                        if (match) {
                            name = match[1].trim();
                            id = match[2].trim();
                            current = match[3].trim();
                            available = match[4].trim();
                        }
                    }

                    if (name && id && id.toLowerCase() !== 'id' && !name.startsWith('---')) {
                        updates.push({
                            name: name,
                            id: id,
                            currentVersion: current || "?",
                            newVersion: available || "latest",
                            manager: 'winget'
                        });
                    }
                });
                
                logger.info(`Winget: ${updates.length} mises à jour trouvées.`);
                resolve(updates);
            });
        });
    }

    getLinuxUpdates() {
        return new Promise((resolve) => {
            if (this.manager === 'apt') {
                exec('apt list --upgradable', (err, stdout) => {
                    if (err) { resolve([]); return; }
                    const updates = [];
                    const lines = stdout.split('\n');
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
                    resolve(updates);
                });
            } else {
                resolve([]);
            }
        });
    }

    updatePackage(pkg, callback) {
        let cmd = '';
        if (pkg.manager === 'winget') {
            cmd = `winget upgrade --id "${pkg.id}" --accept-package-agreements --accept-source-agreements --silent --disable-interactivity --force --include-unknown`;
        } else if (pkg.manager === 'apt') {
            cmd = `apt-get install --only-upgrade -y ${pkg.id}`;
        }

        logger.info(`Lancement mise à jour: ${pkg.id} (Force Mode)`);
        
        const child = exec(cmd);
        
        child.stdout.on('data', (data) => {
            const msg = data.toString().replace(/[\x00-\x1F\x7F-\x9F]/g, "").trim();
            if (msg) callback({ status: 'running', message: msg.substring(0, 50) + "..." });
        });

        child.stderr.on('data', (data) => {
             const msg = data.toString().trim();
             if (msg) callback({ status: 'running', message: "Traitement..." });
        });

        child.on('close', (code) => {
            if (code === 0) {
                logger.info(`Mise à jour réussie: ${pkg.id}`);
                callback({ status: 'finished', message: 'OK' });
            } else {
                logger.error(`Echec mise à jour ${pkg.id}. Code: ${code}`);
                if (code === 2316632084 || code === -1978335212) {
                    callback({ status: 'error', message: `Erreur Hash (Réessayez)` });
                } else {
                    callback({ status: 'error', message: `Erreur (Code ${code})` });
                }
            }
        });
    }
}

module.exports = new UpdateManager();
