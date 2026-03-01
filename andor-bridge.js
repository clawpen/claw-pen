#!/usr/bin/env node
/**
 * AndOR ↔ OpenClaw Multi-Agent Bridge
 * 
 * Receives messages from AndOR chat server, routes to appropriate OpenClaw agent,
 * sends responses back.
 * 
 * Usage:
 *   node andor-bridge.js
 * 
 * Environment:
 *   PORT - Bridge server port (default: 3456)
 *   OPENCLAW_GATEWAY_URL - OpenClaw gateway URL (default: http://127.0.0.1:18789)
 *   OPENCLAW_GATEWAY_TOKEN - Gateway auth token
 *   ANDOR_SERVER_URL - AndOR server URL (default: https://aesir.tailb0b4db.ts.net:8080)
 * 
 * Agent Configuration (via environment or config file):
 *   AGENTS_CONFIG - JSON string or path to agents.json
 * 
 * Example AGENTS_CONFIG:
 *   {
 *     "codi": { "agentId": "main", "displayName": "Codi", "triggers": ["codi", "openclaw"] },
 *     "kimi": { "agentId": "kimiclaw4", "displayName": "KimiClaw4", "triggers": ["kimi", "kimiclaw4"] }
 *   }
 */

const http = require('http');
const https = require('https');
const fs = require('fs');
const path = require('path');
const { URL } = require('url');

// Config
const PORT = process.env.PORT || 3456;
const OPENCLAW_URL = process.env.OPENCLAW_GATEWAY_URL || 'http://127.0.0.1:18789';
const OPENCLAW_TOKEN = process.env.OPENCLAW_GATEWAY_TOKEN || '';
const ANDOR_URL = process.env.ANDOR_SERVER_URL || 'https://aesir.tailb0b4db.ts.net:8080';

// Default agents configuration
const DEFAULT_AGENTS = {
  codi: {
    agentId: 'main',
    displayName: 'Codi',
    triggers: ['codi', 'openclaw'],
    emoji: '🧩'
  }
};

// Load agents config from env or file
function loadAgentsConfig() {
  const configStr = process.env.AGENTS_CONFIG;
  
  if (!configStr) {
    // Try loading from file
    const configPath = path.join(__dirname, 'agents.json');
    if (fs.existsSync(configPath)) {
      try {
        return JSON.parse(fs.readFileSync(configPath, 'utf8'));
      } catch (e) {
        console.warn(`[Bridge] Failed to load agents.json: ${e.message}`);
      }
    }
    return DEFAULT_AGENTS;
  }
  
  // Check if it's a file path
  if (configStr.endsWith('.json') && fs.existsSync(configStr)) {
    return JSON.parse(fs.readFileSync(configStr, 'utf8'));
  }
  
  // Parse as JSON string
  try {
    return JSON.parse(configStr);
  } catch (e) {
    console.warn(`[Bridge] Failed to parse AGENTS_CONFIG: ${e.message}`);
    return DEFAULT_AGENTS;
  }
}

const AGENTS = loadAgentsConfig();

// Build trigger -> agent mapping for fast lookup
const TRIGGER_MAP = {};
for (const [key, agent] of Object.entries(AGENTS)) {
  for (const trigger of agent.triggers || [key]) {
    TRIGGER_MAP[trigger.toLowerCase()] = key;
  }
}

console.log(`[Bridge] Starting AndOR ↔ OpenClaw Multi-Agent Bridge`);
console.log(`[Bridge] Listening on port ${PORT}`);
console.log(`[Bridge] OpenClaw: ${OPENCLAW_URL}`);
console.log(`[Bridge] AndOR: ${ANDOR_URL}`);
console.log(`[Bridge] Agents loaded: ${Object.keys(AGENTS).join(', ')}`);

// Stats per agent
let stats = {
  started: new Date().toISOString(),
  messagesReceived: 0,
  messagesSent: 0,
  errors: 0,
  lastMessage: null,
  lastError: null,
  processing: false,
  processingSince: null,
  perAgent: {}
};

// Initialize per-agent stats
for (const key of Object.keys(AGENTS)) {
  stats.perAgent[key] = { received: 0, sent: 0, errors: 0 };
}

/**
 * Make an HTTP request and return the response body
 */
function httpRequest(urlStr, options = {}, body = null) {
  return new Promise((resolve, reject) => {
    const url = new URL(urlStr);
    const isHttps = url.protocol === 'https:';
    const lib = isHttps ? https : http;
    
    const reqOptions = {
      hostname: url.hostname,
      port: url.port || (isHttps ? 443 : 80),
      path: url.pathname + url.search,
      method: options.method || 'GET',
      headers: {
        'Content-Type': 'application/json',
        ...options.headers
      },
      rejectUnauthorized: false
    };

    const req = lib.request(reqOptions, (res) => {
      let data = '';
      res.on('data', chunk => data += chunk);
      res.on('end', () => {
        if (res.statusCode >= 200 && res.statusCode < 300) {
          try {
            resolve(data ? JSON.parse(data) : {});
          } catch {
            resolve(data);
          }
        } else {
          reject(new Error(`HTTP ${res.statusCode}: ${data}`));
        }
      });
    });

    req.on('error', reject);
    req.setTimeout(120000, () => {
      req.destroy();
      reject(new Error('Request timeout'));
    });

    if (body) {
      req.write(JSON.stringify(body));
    }
    req.end();
  });
}

/**
 * Determine which agent (if any) should respond to a message
 * Returns { agentKey, agent } or null
 * 
 * Triggers:
 * 1. @mention in message (e.g., @codi, @kimi)
 * 2. Channel name matches agent trigger (e.g., channel "Codi" -> Codi agent)
 * 3. Message starts with trigger word (e.g., "codi help me")
 */
function findAgentForMessage(content, channelName, senderName) {
  const contentLower = content.toLowerCase();
  const channelLower = (channelName || '').toLowerCase();
  
  // Check for @mention
  for (const [trigger, agentKey] of Object.entries(TRIGGER_MAP)) {
    if (contentLower.includes(`@${trigger}`)) {
      return { agentKey, agent: AGENTS[agentKey], triggeredBy: 'mention' };
    }
  }
  
  // Check if channel name matches an agent trigger
  // This allows channels named "Codi" or "KimiClaw4" to auto-activate the agent
  for (const [trigger, agentKey] of Object.entries(TRIGGER_MAP)) {
    if (channelLower === trigger || channelLower.includes(trigger)) {
      return { agentKey, agent: AGENTS[agentKey], triggeredBy: 'channel' };
    }
  }
  
  // Check if message starts with trigger
  for (const [trigger, agentKey] of Object.entries(TRIGGER_MAP)) {
    if (contentLower.startsWith(`${trigger} `) || contentLower === trigger) {
      return { agentKey, agent: AGENTS[agentKey], triggeredBy: 'prefix' };
    }
  }
  
  return null;
}

/**
 * Send a message to OpenClaw and get a response
 */
async function askOpenClaw(message, conversationId, senderName, agentKey, agent) {
  const agentId = agent.agentId || agentKey;
  const displayName = agent.displayName || agentKey;
  
  console.log(`[OpenClaw] Sending to agent ${agentId} (${displayName}) from ${senderName}`);
  
  const headers = {
    'Content-Type': 'application/json'
  };
  
  if (OPENCLAW_TOKEN) {
    headers['Authorization'] = `Bearer ${OPENCLAW_TOKEN}`;
  }
  
  // Set agent ID and session key
  headers['x-openclaw-agent-id'] = agentId;
  headers['x-openclaw-session-key'] = `andor:${conversationId}:${agentKey}`;
  
  const response = await httpRequest(`${OPENCLAW_URL}/v1/chat/completions`, {
    method: 'POST',
    headers
  }, {
    model: 'openclaw',
    messages: [
      {
        role: 'user',
        content: message
      }
    ],
    user: senderName
  });

  if (response.choices && response.choices[0]?.message?.content) {
    return {
      content: response.choices[0].message.content,
      displayName
    };
  }
  
  throw new Error('Unexpected OpenClaw response format');
}

/**
 * Send a message back to AndOR server
 */
async function sendToAndOR(conversationId, content, displayName) {
  console.log(`[AndOR] Sending response from ${displayName} to ${conversationId}`);
  
  await httpRequest(`${ANDOR_URL}/api/openclaw/message`, {
    method: 'POST'
  }, {
    conversationId,
    content,
    senderName: displayName,
    senderId: `openclaw-${displayName.toLowerCase()}`
  });
  
  stats.messagesSent++;
  console.log(`[AndOR] Message sent successfully`);
}

/**
 * Handle incoming webhook from AndOR
 */
async function handleWebhook(req, res) {
  if (req.method !== 'POST') {
    res.writeHead(405);
    res.end('Method not allowed');
    return;
  }

  let body = '';
  req.on('data', chunk => body += chunk);
  req.on('end', async () => {
    try {
      const data = JSON.parse(body);
      console.log(`[Webhook] Received:`, JSON.stringify(data, null, 2));

      if (data.type === 'message' && data.data) {
        const { 
          messageId, 
          content, 
          senderId, 
          senderName, 
          conversationId, 
          channelName,
          timestamp 
        } = data.data;

        stats.messagesReceived++;
        stats.lastMessage = `${senderName}: ${content.substring(0, 50)}${content.length > 50 ? '...' : ''}`;

        // Find which agent should respond
        const match = findAgentForMessage(content, channelName, senderName);
        
        if (!match) {
          console.log(`[Webhook] No agent triggered for this message`);
          res.writeHead(200);
          res.end(JSON.stringify({ status: 'no_agent' }));
          return;
        }
        
        const { agentKey, agent, triggeredBy } = match;
        
        // Don't respond to our own messages
        if (senderId === `openclaw-${agent.displayName.toLowerCase()}` || 
            senderName === agent.displayName) {
          console.log(`[Webhook] Ignoring own message`);
          res.writeHead(200);
          res.end(JSON.stringify({ status: 'ignored', reason: 'own_message' }));
          return;
        }

        console.log(`[Webhook] Agent ${agentKey} triggered by ${triggeredBy}`);

        // Acknowledge receipt immediately
        res.writeHead(200);
        res.end(JSON.stringify({ status: 'received', agent: agentKey }));

        // Process the message
        stats.processing = true;
        stats.processingSince = new Date().toISOString();
        stats.perAgent[agentKey].received++;
        
        try {
          const response = await askOpenClaw(content, conversationId, senderName, agentKey, agent);
          
          // Send response back to AndOR
          await sendToAndOR(conversationId, response.content, response.displayName);
          stats.perAgent[agentKey].sent++;
        } catch (error) {
          console.error(`[Error] Failed to process message:`, error.message);
          stats.errors++;
          stats.perAgent[agentKey].errors++;
          stats.lastError = error.message;
        } finally {
          stats.processing = false;
          stats.processingSince = null;
        }
      } else {
        res.writeHead(400);
        res.end(JSON.stringify({ error: 'Invalid message format' }));
      }
    } catch (error) {
      console.error(`[Error] Webhook parse error:`, error.message);
      stats.errors++;
      stats.lastError = error.message;
      res.writeHead(400);
      res.end(JSON.stringify({ error: error.message }));
    }
  });
}

// Health check endpoint
function handleHealth(req, res) {
  res.writeHead(200);
  res.end(JSON.stringify({ 
    status: 'ok', 
    service: 'andor-bridge',
    openclaw: OPENCLAW_URL,
    andor: ANDOR_URL,
    agents: Object.keys(AGENTS)
  }));
}

// Dashboard endpoint
function handleDashboard(req, res) {
  const dashboardPath = path.join(__dirname, 'dashboard.html');
  
  try {
    const html = fs.readFileSync(dashboardPath, 'utf8');
    res.writeHead(200, { 'Content-Type': 'text/html' });
    res.end(html);
  } catch (error) {
    res.writeHead(500, { 'Content-Type': 'text/json' });
    res.end(JSON.stringify({ error: 'Dashboard not found' }));
  }
}

// Stats endpoint
function handleStats(req, res) {
  res.writeHead(200, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify(stats));
}

// Agents list endpoint
function handleAgents(req, res) {
  res.writeHead(200, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify({
    agents: AGENTS,
    triggerMap: TRIGGER_MAP
  }));
}

// Status page with activity
function handleStatus(req, res) {
  const processingTime = stats.processing && stats.processingSince 
    ? Math.floor((Date.now() - new Date(stats.processingSince).getTime()) / 1000) 
    : 0;
  
  // Build agent stats
  const agentStatsHtml = Object.entries(stats.perAgent).map(([key, data]) => `
    <div class="stat"><span class="label">${AGENTS[key]?.displayName || key}</span><span class="value">${data.sent} sent / ${data.received} recv</span></div>
  `).join('');
  
  const html = `
<!DOCTYPE html>
<html>
<head>
  <title>AndOR Bridge Status</title>
  <meta http-equiv="refresh" content="2">
  <style>
    body { font-family: system-ui; max-width: 600px; margin: 40px auto; padding: 20px; background: #1a1a2e; color: #eee; }
    .card { background: #16213e; border-radius: 8px; padding: 20px; margin: 10px 0; }
    .stat { display: flex; justify-content: space-between; padding: 8px 0; border-bottom: 1px solid #333; }
    .label { color: #888; }
    .value { font-weight: bold; color: #4ecca3; }
    .error { color: #ff6b6b; }
    h1 { color: #4ecca3; }
    .pulse { animation: pulse 2s infinite; }
    @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }
    .processing { background: #2d4a3e; border: 2px solid #4ecca3; }
    .processing-indicator { color: #4ecca3; font-size: 24px; animation: blink 0.5s infinite; }
    @keyframes blink { 0%, 100% { opacity: 1; } 50% { opacity: 0.3; } }
    .agent-badge { display: inline-block; padding: 2px 8px; background: #238636; border-radius: 12px; margin: 2px; font-size: 12px; }
  </style>
</head>
<body>
  <h1>🧩 AndOR ↔ OpenClaw Bridge</h1>
  <div class="card">
    <div class="stat">
      <span class="label">Agents</span>
      <span class="value">${Object.values(AGENTS).map(a => `<span class="agent-badge">${a.emoji || '🤖'} ${a.displayName}</span>`).join('')}</span>
    </div>
  </div>
  <div class="card ${stats.processing ? 'processing' : ''}">
    <div class="stat">
      <span class="label">Status</span>
      <span class="${stats.processing ? 'processing-indicator' : 'value pulse'}">
        ${stats.processing ? '⚡ PROCESSING (' + processingTime + 's)' : '● Running'}
      </span>
    </div>
    <div class="stat"><span class="label">Uptime</span><span class="value">${Math.floor((Date.now() - new Date(stats.started).getTime()) / 1000)}s</span></div>
  </div>
  <div class="card">
    <div class="stat"><span class="label">Messages Received</span><span class="value">${stats.messagesReceived}</span></div>
    <div class="stat"><span class="label">Messages Sent</span><span class="value">${stats.messagesSent}</span></div>
    <div class="stat"><span class="label">Errors</span><span class="value ${stats.errors > 0 ? 'error' : ''}">${stats.errors}</span></div>
  </div>
  <div class="card">
    <div class="card-title" style="font-size: 12px; color: #888; margin-bottom: 8px;">PER-AGENT STATS</div>
    ${agentStatsHtml}
  </div>
  <p style="color: #666; text-align: center;">Auto-refreshes every 2s</p>
</body>
</html>`;
  res.writeHead(200, { 'Content-Type': 'text/html' });
  res.end(html);
}

// Start server
const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://${req.headers.host}`);
  
  // CORS headers
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
  res.setHeader('Access-Control-Allow-Headers', 'Content-Type');
  
  if (req.method === 'OPTIONS') {
    res.writeHead(204);
    res.end();
    return;
  }
  
  if (url.pathname === '/health') {
    handleHealth(req, res);
  } else if (url.pathname === '/stats') {
    handleStats(req, res);
  } else if (url.pathname === '/agents') {
    handleAgents(req, res);
  } else if (url.pathname === '/dashboard' || url.pathname === '/') {
    handleDashboard(req, res);
  } else if (url.pathname === '/status') {
    handleStatus(req, res);
  } else if (url.pathname === '/webhook') {
    handleWebhook(req, res);
  } else {
    res.writeHead(404);
    res.end(JSON.stringify({ error: 'Not found' }));
  }
});

server.listen(PORT, '0.0.0.0', () => {
  console.log(`[Bridge] Server listening on http://0.0.0.0:${PORT}`);
  console.log(`[Bridge] Webhook URL: http://100.77.212.101:${PORT}/webhook`);
  console.log(`[Bridge] Health: http://100.77.212.101:${PORT}/health`);
  console.log(`[Bridge] Agents: http://100.77.212.101:${PORT}/agents`);
});

// Graceful shutdown
process.on('SIGTERM', () => {
  console.log('[Bridge] Shutting down...');
  server.close(() => process.exit(0));
});
