const { contextBridge, ipcRenderer } = require('electron');

// Expose protected methods that allow the renderer process to use
// the ipcRenderer without exposing the entire object
contextBridge.exposeInMainWorld('electronAPI', {
  // Storage API
  storage: {
    getItem: (key) => ipcRenderer.invoke('storage-get', key),
    setItem: (key, value) => ipcRenderer.invoke('storage-set', key, value),
    removeItem: (key) => ipcRenderer.invoke('storage-remove', key),
    clear: () => ipcRenderer.invoke('storage-clear')
  },
  
  // HTTP request API (bypasses CORS)
  httpRequest: (options) => ipcRenderer.invoke('http-request', options),
  
  // Platform info
  platform: process.platform,
  isElectron: true
});

console.log('HITOP Electron preload script loaded');
