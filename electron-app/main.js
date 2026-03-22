const { app, BrowserWindow, ipcMain } = require('electron');
const WebSocket = require('ws');
const nacl = require('tweetnacl');
const path = require('path');
const fs = require('fs');
const crypto = require('crypto');

// SHA256 hash helper
function sha256(bytes) {
    return crypto.createHash('sha256').update(bytes).digest('hex');
}

let mainWindow = null;
let ws = null;
let reconnectTimer = null;
let deviceKeys = null;
let currentUrl = null;

// Base64 encode/decode helpers
const BASE64 = {
    encode: (bytes) => Buffer.from(bytes).toString('base64'),
    decode: (str) => Buffer.from(str, 'base64')
};

// Load or create device keys
function loadOrCreateDeviceKeys() {
    const keysPath = path.join(app.getPath('userData'), 'device_keys.json');
    
    try {
        if (fs.existsSync(keysPath)) {
            const data = JSON.parse(fs.readFileSync(keysPath, 'utf8'));
            // privateKey is 32 bytes (seed), same as ed25519-dalek SigningKey
            const seedBytes = BASE64.decode(data.privateKey);
            // Use fromSeed to expand to full keypair
            const keyPair = nacl.sign.keyPair.fromSeed(seedBytes);
            return {
                deviceId: data.deviceId,
                secretKey: keyPair.secretKey, // 64 bytes (seed || public)
                publicKey: keyPair.publicKey  // 32 bytes
            };
        }
    } catch (e) {
        console.error('Error loading device keys:', e);
    }
    
    // Generate new keys - use random 32-byte seed
    const seed = nacl.randomBytes(32);
    const keyPair = nacl.sign.keyPair.fromSeed(seed);
    
    // Device ID is SHA256 hash of public key (hex encoded)
    const deviceId = sha256(keyPair.publicKey);
    
    const data = {
        deviceId: deviceId,
        privateKey: BASE64.encode(seed), // Store just the 32-byte seed
        publicKey: BASE64.encode(keyPair.publicKey)
    };
    
    fs.writeFileSync(keysPath, JSON.stringify(data, null, 2));
    
    return {
        deviceId: deviceId,
        secretKey: keyPair.secretKey,
        publicKey: keyPair.publicKey
    };
}

function createWindow() {
    mainWindow = new BrowserWindow({
        width: 1200,
        height: 800,
        minWidth: 800,
        minHeight: 600,
        webPreferences: {
            nodeIntegration: false,
            contextIsolation: true,
            preload: path.join(__dirname, 'preload.js')
        },
        backgroundColor: '#0f0f1a'
    });

    mainWindow.loadFile(path.join(__dirname, 'dashboard.html'));
}

function connectWebSocket(url) {
    if (ws) {
        ws.close();
        ws = null;
    }
    
    if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
    }

    currentUrl = url;
    console.log('[WS] Connecting to:', url);

    ws = new WebSocket(url, {
        headers: {
            'Origin': 'http://localhost:3000'
        }
    });
    
    ws.on('open', () => {
        console.log('[WS] Connected');
        mainWindow?.webContents.send('ws-status', { connected: true });
    });
    
    ws.on('message', (data) => {
        const text = data.toString();
        console.log('[WS] RX:', text.substring(0, 300));

        try {
            const msg = JSON.parse(text);

            // Handle auth challenge
            if (msg.event === 'connect.challenge') {
                const nonce = msg.payload?.nonce || msg.params?.nonce || '';
                const challengeTimestamp = msg.payload?.ts || msg.params?.ts || Date.now();
                console.log('[WS] Challenge - nonce:', nonce, 'ts:', challengeTimestamp);
                handleChallenge(nonce, challengeTimestamp);
                return;
            }

            // Handle auth success
            if (msg.ok === true && msg.id?.startsWith('cp-')) {
                console.log('[WS] ✓ Authenticated successfully!');
                mainWindow?.webContents.send('ws-authenticated', true);
                return;
            }

            // Handle auth failure
            if (msg.ok === false) {
                console.log('[WS] ✗ Auth failed:', msg.error?.code, msg.error?.message);
                mainWindow?.webContents.send('ws-error', msg.error?.message || 'Authentication failed');
                return;
            }

            // Forward all other messages to renderer
            mainWindow?.webContents.send('ws-message', msg);
        } catch (e) {
            console.error('[WS] Parse error:', e);
        }
    });
    
    ws.on('close', () => {
        console.log('[WS] Disconnected');
        mainWindow?.webContents.send('ws-status', { connected: false });
        mainWindow?.webContents.send('ws-authenticated', false);
        // Reconnect after 3 seconds
        reconnectTimer = setTimeout(() => {
            if (mainWindow && currentUrl) {
                connectWebSocket(currentUrl);
            }
        }, 3000);
    });
    
    ws.on('error', (err) => {
        console.error('[WS] Error:', err);
        mainWindow?.webContents.send('ws-error', err.message);
    });
}

// Sign and send connect request
function handleChallenge(nonce, challengeTimestamp) {
    // Send connect request with password authentication
    const response = {
        type: 'req',
        id: 'cp-' + Date.now(),
        method: 'connect',
        params: {
            minProtocol: 3,
            maxProtocol: 3,
            client: {
                id: 'cli',
                version: '1.0.0',
                platform: 'desktop',
                mode: 'webchat'
            },
            role: 'operator',
            scopes: [],
            auth: {
                password: 'claw'
            }
        }
    };

    console.log('[WS] Sending connect request (with password auth)');
    ws.send(JSON.stringify(response));
}

// IPC handlers
ipcMain.handle('connect-websocket', (event, url) => {
    connectWebSocket(url);
    return true;
});

ipcMain.handle('send-message', (event, text) => {
    if (ws && ws.readyState === WebSocket.OPEN) {
        const msg = {
            type: 'req',
            id: 'msg-' + Date.now(),
            method: 'chat.send',
            params: {
                sessionKey: 'main',
                message: text,
                deliver: false,
                idempotencyKey: crypto.randomUUID()
            }
        };
        ws.send(JSON.stringify(msg));
        return true;
    }
    return false;
});

ipcMain.handle('disconnect-websocket', () => {
    if (ws) {
        ws.close();
        ws = null;
    }
    return true;
});

// Fetch agents from orchestrator API
ipcMain.handle('fetch-agents', async () => {
    const http = require('http');
    const fs = require('fs');
    const path = require('path');
    
    // Try to get stored token
    let token = null;
    try {
        const tokenPath = path.join(app.getPath('userData'), 'auth_token.json');
        if (fs.existsSync(tokenPath)) {
            const data = JSON.parse(fs.readFileSync(tokenPath, 'utf8'));
            token = data.token;
        }
    } catch (e) {}
    
    return new Promise((resolve, reject) => {
        const options = {
            hostname: '127.0.0.1',
            port: 8081,
            path: '/api/agents',
            method: 'GET'
        };
        
        if (token) {
            options.headers = { 'Authorization': 'Bearer ' + token };
        }
        
        const req = http.get(options, (res) => {
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', async () => {
                try {
                    let agents = JSON.parse(data);
                    console.log('[API] Loaded', agents.length, 'agents');
                    
                    // Fetch Docker stats for each running agent
                    agents = await Promise.all(agents.map(async (agent) => {
                        if (agent.status === 'running' || agent.status === 'Running') {
                            try {
                                const stats = await getDockerStats(agent.id);
                                agent.resource_usage = stats;
                            } catch (e) {
                                // Stats not available
                            }
                        }
                        return agent;
                    }));
                    
                    resolve(agents);
                } catch (e) {
                    console.error('[API] Parse error:', e);
                    resolve([]);
                }
            });
        });
        req.on('error', (e) => {
            console.error('[API] Failed to fetch agents:', e.message);
            resolve([]);
        });
        req.setTimeout(5000, () => {
            req.destroy();
            console.error('[API] Timeout fetching agents');
            resolve([]);
        });
    });
});

// Get Docker container stats
async function getDockerStats(containerId) {
    const { execSync } = require('child_process');
    try {
        // Get container stats (CPU %, Memory usage)
        const output = execSync(
            `docker stats --no-stream --format "{{.CPUPerc}},{{.MemUsage}}" ${containerId} 2>/dev/null`,
            { timeout: 5000, encoding: 'utf8' }
        ).trim();
        
        if (!output) return null;
        
        const [cpuPerc, memUsage] = output.split(',');
        const memMatch = memUsage?.match(/([\d.]+)(MiB|GiB)/i);
        const memTotalMatch = memUsage?.match(/\/\s*([\d.]+)(MiB|GiB)/i);
        
        return {
            cpu_percent: parseFloat(cpuPerc?.replace('%', '') || 0),
            memory_mb: memMatch ? (memMatch[2] === 'GiB' ? parseFloat(memMatch[1]) * 1024 : parseFloat(memMatch[1])) : 0,
            memory_total_mb: memTotalMatch ? (memTotalMatch[2] === 'GiB' ? parseFloat(memTotalMatch[1]) * 1024 : parseFloat(memTotalMatch[1])) : 0
        };
    } catch (e) {
        return null;
    }
}

// Create agent
ipcMain.handle('create-agent', async (event, config) => {
    const http = require('http');
    const body = JSON.stringify({
        name: config.name,
        template: 'openclaw-agent',
        config: {
            llm_provider: config.provider,
            llm_model: config.model,
            memory_mb: config.memory_mb || 1024,
            cpu_cores: config.cpu_cores || 1
        }
    });
    
    console.log('[API] Creating agent:', body);
    
    return new Promise((resolve, reject) => {
        const req = http.request({
            hostname: '127.0.0.1',
            port: 8081,
            path: '/api/agents',
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Content-Length': Buffer.byteLength(body)
            }
        }, (res) => {
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', () => {
                console.log('[API] Create response:', res.statusCode, data);
                if (res.statusCode >= 200 && res.statusCode < 300) {
                    try { resolve(JSON.parse(data)); }
                    catch (e) { resolve({ success: true }); }
                } else {
                    reject(new Error('Failed to create agent: ' + res.statusCode + ' ' + data));
                }
            });
        });
        req.on('error', (e) => {
            console.error('[API] Create error:', e);
            reject(e);
        });
        req.write(body);
        req.end();
    });
});

// Start agent
ipcMain.handle('start-agent', async (event, id) => {
    const http = require('http');
    const fs = require('fs');
    const path = require('path');
    
    // Get token
    let token = null;
    try {
        const tokenPath = path.join(app.getPath('userData'), 'auth_token.json');
        if (fs.existsSync(tokenPath)) {
            token = JSON.parse(fs.readFileSync(tokenPath, 'utf8')).token;
        }
    } catch (e) {}
    
    return new Promise((resolve, reject) => {
        const options = {
            hostname: '127.0.0.1',
            port: 8081,
            path: '/api/agents/' + id + '/start',
            method: 'POST'
        };
        if (token) options.headers = { 'Authorization': 'Bearer ' + token };
        
        const req = http.request(options, (res) => {
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', () => resolve(data));
        });
        req.on('error', reject);
        req.end();
    });
});

// Stop agent
ipcMain.handle('stop-agent', async (event, id) => {
    const http = require('http');
    const fs = require('fs');
    const path = require('path');
    
    // Get token
    let token = null;
    try {
        const tokenPath = path.join(app.getPath('userData'), 'auth_token.json');
        if (fs.existsSync(tokenPath)) {
            token = JSON.parse(fs.readFileSync(tokenPath, 'utf8')).token;
        }
    } catch (e) {}
    
    return new Promise((resolve, reject) => {
        const options = {
            hostname: '127.0.0.1',
            port: 8081,
            path: '/api/agents/' + id + '/stop',
            method: 'POST'
        };
        if (token) options.headers = { 'Authorization': 'Bearer ' + token };
        
        const req = http.request(options, (res) => {
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', () => resolve(data));
        });
        req.on('error', reject);
        req.end();
    });
});

// Update agent config
ipcMain.handle('update-agent', async (event, config) => {
    const http = require('http');
    
    // First get current agent config
    const currentAgent = await new Promise((resolve, reject) => {
        http.get('http://127.0.0.1:8081/api/agents', (res) => {
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', () => {
                try {
                    const agents = JSON.parse(data);
                    const agent = agents.find(a => a.id === config.id);
                    resolve(agent);
                } catch (e) {
                    reject(e);
                }
            });
        }).on('error', reject);
    });
    
    if (!currentAgent) {
        throw new Error('Agent not found');
    }
    
    // Build update body - merge with existing config
    const envVars = { ...currentAgent.config.env_vars };
    if (config.api_key) {
        // Set the right env var based on provider
        const provider = config.provider || currentAgent.config.llm_provider;
        if (provider === 'zai') envVars.ZAI_API_KEY = config.api_key;
        else if (provider === 'openai') envVars.OPENAI_API_KEY = config.api_key;
        else if (provider === 'anthropic') envVars.ANTHROPIC_API_KEY = config.api_key;
        else if (provider === 'kimi') envVars.KIMI_API_KEY = config.api_key;
        else if (provider === 'google') envVars.GOOGLE_API_KEY = config.api_key;
    }
    
    const body = JSON.stringify({
        name: currentAgent.name,
        template: currentAgent.template || 'openclaw-agent',
        config: {
            llm_provider: config.provider || currentAgent.config.llm_provider,
            llm_model: config.model || currentAgent.config.llm_model,
            memory_mb: config.memory_mb || currentAgent.config.memory_mb,
            cpu_cores: config.cpu_cores || currentAgent.config.cpu_cores,
            env_vars: envVars
        }
    });
    
    console.log('[API] Updating agent:', config.id, body.substring(0, 200));
    
    return new Promise((resolve, reject) => {
        const req = http.request({
            hostname: '127.0.0.1',
            port: 8081,
            path: '/api/agents/' + config.id,
            method: 'PUT',
            headers: {
                'Content-Type': 'application/json',
                'Content-Length': Buffer.byteLength(body)
            }
        }, (res) => {
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', () => {
                console.log('[API] Update response:', res.statusCode, data.substring(0, 100));
                if (res.statusCode >= 200 && res.statusCode < 300) {
                    resolve(true);
                } else {
                    reject(new Error('Failed to update: ' + res.statusCode + ' ' + data));
                }
            });
        });
        req.on('error', (e) => {
            console.error('[API] Update error:', e);
            reject(e);
        });
        req.write(body);
        req.end();
    });
    
    // After updating config, restart the agent to apply changes
    // Note: This requires stopping and recreating the container
    console.log('[API] Agent updated, restart required to apply changes');
});

// Fetch API keys status
ipcMain.handle('fetch-api-keys', async () => {
    const http = require('http');
    return new Promise((resolve, reject) => {
        http.get('http://127.0.0.1:8081/api/keys', (res) => {
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', () => {
                try { resolve(JSON.parse(data)); }
                catch (e) { resolve([]); }
            });
        }).on('error', () => resolve([]));
    });
});

// Set API key
ipcMain.handle('set-api-key', async (event, data) => {
    const http = require('http');
    const body = JSON.stringify({ provider: data.provider, key: data.key });
    
    return new Promise((resolve, reject) => {
        const req = http.request({
            hostname: '127.0.0.1',
            port: 8081,
            path: '/api/keys',
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Content-Length': Buffer.byteLength(body)
            }
        }, (res) => {
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', () => resolve(res.statusCode < 300));
        });
        req.on('error', reject);
        req.write(body);
        req.end();
    });
});

// Login to orchestrator
ipcMain.handle('login', async (event, credentials) => {
    const http = require('http');
    const fs = require('fs');
    const path = require('path');
    const body = JSON.stringify({ password: credentials.password });
    
    return new Promise((resolve, reject) => {
        const req = http.request({
            hostname: '127.0.0.1',
            port: 8081,
            path: '/auth/login',
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Content-Length': Buffer.byteLength(body)
            }
        }, (res) => {
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', () => {
                if (res.statusCode === 200) {
                    try {
                        const result = JSON.parse(data);
                        // Store token
                        const tokenPath = path.join(app.getPath('userData'), 'auth_token.json');
                        fs.writeFileSync(tokenPath, JSON.stringify({ token: result.access_token }));
                        resolve({ success: true, token: result.access_token });
                    } catch (e) {
                        resolve({ success: false, error: 'Parse error' });
                    }
                } else {
                    resolve({ success: false, error: 'Invalid password' });
                }
            });
        });
        req.on('error', (e) => resolve({ success: false, error: e.message }));
        req.write(body);
        req.end();
    });
});

// Check auth status
ipcMain.handle('check-auth', async () => {
    const http = require('http');
    const fs = require('fs');
    const path = require('path');
    
    // Try to get stored token
    let token = null;
    try {
        const tokenPath = path.join(app.getPath('userData'), 'auth_token.json');
        if (fs.existsSync(tokenPath)) {
            const data = JSON.parse(fs.readFileSync(tokenPath, 'utf8'));
            token = data.token;
        }
    } catch (e) {}
    
    return new Promise((resolve) => {
        // Check if auth is enabled
        const req = http.get('http://127.0.0.1:8081/auth/status', (res) => {
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', () => {
                try {
                    const status = JSON.parse(data);
                    if (!status.auth_enabled) {
                        resolve({ authenticated: true });
                    } else if (token) {
                        resolve({ authenticated: true });
                    } else {
                        resolve({ authenticated: false });
                    }
                } catch (e) {
                    resolve({ authenticated: false });
                }
            });
        });
        req.on('error', () => resolve({ authenticated: false }));
    });
});

app.whenReady().then(() => {
    deviceKeys = loadOrCreateDeviceKeys();
    console.log('[Device] ID:', deviceKeys.deviceId);
    createWindow();
});

app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') {
        app.quit();
    }
});
