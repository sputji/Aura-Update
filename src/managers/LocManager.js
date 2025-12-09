const fs = require('fs');
const path = require('path');
const { app } = require('electron');

class LocManager {
    constructor() {
        // CORRECTION : En production, les fichiers sont dans l'archive ASAR (app.getAppPath()), 
        // et non dans resources/locales (process.resourcesPath).
        const basePath = path.join(app.getAppPath(), 'locales');
            
        this.localesPath = basePath;
        this.configPath = path.join(app.getPath('userData'), 'config.json');
        
        this.currentLang = 'fr'; // Par défaut
        this.translations = {};
        
        this.loadConfig();
        this.loadTranslations(this.currentLang);
    }

    loadConfig() {
        try {
            if (fs.existsSync(this.configPath)) {
                const conf = JSON.parse(fs.readFileSync(this.configPath, 'utf8'));
                if (conf.lang) this.currentLang = conf.lang;
            }
        } catch (e) { console.error("Erreur chargement config:", e); }
    }

    saveConfig() {
        try {
            fs.writeFileSync(this.configPath, JSON.stringify({ lang: this.currentLang }));
        } catch (e) { console.error("Erreur sauvegarde config:", e); }
    }

    loadTranslations(lang) {
        try {
            const file = path.join(this.localesPath, `${lang}.json`);
            // fs.existsSync fonctionne transparente avec les chemins ASAR dans Electron
            if (fs.existsSync(file)) {
                this.translations = JSON.parse(fs.readFileSync(file, 'utf8'));
                this.currentLang = lang;
                this.saveConfig();
            } else {
                console.warn(`Fichier de langue introuvable: ${file}`);
                // Fallback
                if (lang !== 'en') this.loadTranslations('en');
            }
        } catch (e) {
            console.error(`Erreur chargement langue ${lang}:`, e);
            // Fallback anglais si échec
            if (lang !== 'en') this.loadTranslations('en');
        }
    }

    getTranslations() {
        return this.translations;
    }

    setLanguage(lang) {
        this.loadTranslations(lang);
        return this.translations;
    }
}

module.exports = new LocManager();
