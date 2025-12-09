const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('api', {
    getTranslations: () => ipcRenderer.invoke('get-translations'),
    setLanguage: (lang) => ipcRenderer.invoke('set-language', lang),
    checkUpdates: () => ipcRenderer.invoke('check-updates'),
    updatePackage: (pkg) => ipcRenderer.invoke('update-package', pkg),
    updateAll: () => ipcRenderer.invoke('update-all'),
    onProgress: (callback) => ipcRenderer.on('update-progress', (event, data) => callback(data)),
    isAdmin: () => ipcRenderer.invoke('is-admin'),
    elevate: () => ipcRenderer.invoke('elevate'),
    getAppVersion: () => ipcRenderer.invoke('get-app-version')
});