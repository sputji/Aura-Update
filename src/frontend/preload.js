const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('api', {
    // Updates & Admin
    checkUpdates: () => ipcRenderer.invoke('check-updates'),
    updatePackage: (pkg) => ipcRenderer.invoke('update-package', pkg),
    updateAll: () => ipcRenderer.invoke('update-all'),
    isAdmin: () => ipcRenderer.invoke('is-admin'),
    elevate: () => ipcRenderer.invoke('elevate'),
    
    // Localization
    getTranslations: () => ipcRenderer.invoke('get-translations'),
    setLanguage: (lang) => ipcRenderer.invoke('set-language', lang),
    
    // Events
    onLog: (callback) => ipcRenderer.on('log-message', (event, data) => callback(data)),
    onProgress: (callback) => ipcRenderer.on('update-progress', (event, data) => callback(data))
});