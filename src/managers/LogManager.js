const fs = require('fs');
const path = require('path');
const { app } = require('electron');

class LogManager {
    constructor() {
        // Chemin relatif à l'exécutable ou au projet
        // En prod, process.execPath est le .exe. En dev, c'est electron.exe.
        // On remonte pour se mettre à la racine du projet
        const basePath = app.isPackaged 
            ? path.dirname(process.execPath) 
            : path.resolve(__dirname, '..', '..');

        this.logDir = path.join(basePath, 'logs');
        this.logFile = path.join(this.logDir, 'app.log');

        this.ensureDirectory();
        this.info('LogManager initialisé. Chemin: ' + this.logFile);
    }

    ensureDirectory() {
        if (!fs.existsSync(this.logDir)) {
            fs.mkdirSync(this.logDir, { recursive: true });
        }
    }

    write(level, message) {
        const timestamp = new Date().toISOString().replace('T', ' ').split('.')[0];
        const line = `[${timestamp}] [${level}] ${message}\n`;
        
        console.log(line.trim()); // Affiche aussi dans la console de debug Electron

        try {
            fs.appendFileSync(this.logFile, line, { encoding: 'utf8' });
        } catch (e) {
            console.error("Impossible d'écrire dans les logs:", e);
        }
    }

    info(msg) { this.write('INFO', msg); }
    error(msg) { this.write('ERROR', msg); }
    warn(msg) { this.write('WARN', msg); }
}

module.exports = new LogManager();
