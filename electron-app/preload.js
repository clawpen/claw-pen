const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
    // WebSocket
    connectWebSocket: (url) => ipcRenderer.invoke('connect-websocket', url),
    sendMessage: (text) => ipcRenderer.invoke('send-message', text),
    disconnectWebSocket: () => ipcRenderer.invoke('disconnect-websocket'),
    
    // Agents
    fetchAgents: () => ipcRenderer.invoke('fetch-agents'),
    createAgent: (config) => ipcRenderer.invoke('create-agent', config),
    updateAgent: (config) => ipcRenderer.invoke('update-agent', config),
    startAgent: (id) => ipcRenderer.invoke('start-agent', id),
    stopAgent: (id) => ipcRenderer.invoke('stop-agent', id),
    
    // API Keys
    fetchApiKeys: () => ipcRenderer.invoke('fetch-api-keys'),
    setApiKey: (data) => ipcRenderer.invoke('set-api-key', data),
    
    // Auth
    login: (credentials) => ipcRenderer.invoke('login', credentials),
    checkAuth: () => ipcRenderer.invoke('check-auth'),
    
    // Auth
    login: (credentials) => ipcRenderer.invoke('login', credentials),
    checkAuth: () => ipcRenderer.invoke('check-auth'),
    
    // Events
    onWsStatus: (callback) => {
        const handler = (event, data) => callback(data);
        ipcRenderer.on('ws-status', handler);
        return () => ipcRenderer.removeListener('ws-status', handler);
    },
    onWsMessage: (callback) => {
        const handler = (event, data) => callback(data);
        ipcRenderer.on('ws-message', handler);
        return () => ipcRenderer.removeListener('ws-message', handler);
    },
    onWsAuthenticated: (callback) => {
        const handler = (event, data) => callback(data);
        ipcRenderer.on('ws-authenticated', handler);
        return () => ipcRenderer.removeListener('ws-authenticated', handler);
    },
    onWsError: (callback) => {
        const handler = (event, data) => callback(data);
        ipcRenderer.on('ws-error', handler);
        return () => ipcRenderer.removeListener('ws-error', handler);
    }
});
