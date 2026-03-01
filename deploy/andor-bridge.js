#!/usr/bin/env node
/**
 * AndOR ↔ OpenClaw Bridge
 * 
 * Receives messages from AndOR chat server, forwards to OpenClaw, sends responses back.
 * 
 * Usage:
 *   node andor-bridge.js
 * 
 * Environment:
 *   PORT - Bridge server port (default: 3456)
 *   OPENCLAW_GATEWAY_URL - OpenClaw gateway URL (default: http://127.0.0.1:18789)
 *   OPENCLAW_GATEWAY_TOKEN - Gateway auth token
 *   ANDOR_SERVER_URL - AndOR server URL (default: https://aesir.tailb0b4db.ts.net:8080)
 *   BRIDGE_NAME - Bot display name (default: Codi)
 */

const http = require('http');
const https = require('https');
const { URL } = require('url');

// Config
const PORT = process.env.PORT || 3456;
const OPENCLAW_URL = process.env.OPENCLAW_GATEWAY_URL || 'http://127.0.0.1:18789';
const OPENCLAW_TOKEN = process.env.OPENCLAW_GATEWAY_TOKEN || 'ec2f73e5d4c0aa44b37349b35135a530d58903558d379ff5';
const ANDOR_URL = process.env.ANDOR_SERVER_URL || 'https://aesir.tailb0b4db.ts.net:8080';
const BRIDGE_NAME = process.env.BRIDGE_NAME || 'Codi';

console.log(`[Bridge] Starting AndOR ↔ OpenClaw bridge`);
console.log(`[Bridge] Listening on port ${PORT}`);
console.log(`[Bridge] OpenClaw: ${OPENCLAW_URL}`);
console.log(`[Bridge] AndOR: ${ANDOR_URL}`);

// Track activity
let stats = {
  started: new Date().toISOString(),
  messagesReceived: 0,
  messagesSent: 0,
  errors: 0,
  lastMessage: null,
  lastError: null,
  processing: false,
  processingSince: null
};

/**
 * Make an HTTP request and return the response body
 */
function httpRequest(urlStr, options = {}, body = null) {
  return new Promise((resolve, reject) => {
    const url = new URL(urlStr);
    const isHttps = url.protocol === 'https:';
    const lib = isHttps ? https : http;
    
    // Reject unauthorized for self-signed certs (Tailscale often uses these)
    const reqOptions = {
      hostname: url.hostname,
      port: url.port || (isHttps ? 443 : 80),
      path: url.pathname + url.search,
      method: options.method || 'GET',
      headers: {
        'Content-Type': 'application/json',
        ...options.headers
      },
      rejectUnauthorized: false  // Allow self-signed certs
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
    req.setTimeout(120000, () => {  // 2 minute timeout for slow model responses
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
 * Send a message to OpenClaw and get a response
 */
async function askOpenClaw(message, conversationId, senderName) {
  console.log(`[OpenClaw] Sending message from ${senderName} in ${conversationId}`);
  
  const response = await httpRequest(`${OPENCLAW_URL}/v1/chat/completions`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${OPENCLAW_TOKEN}`,
      'x-openclaw-agent-id': 'main',
      'x-openclaw-session-key': `andor:${conversationId}`
    }
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

  // Extract the response text
  if (response.choices && response.choices[0]?.message?.content) {
    return response.choices[0].message.content;
  }
  
  throw new Error('Unexpected OpenClaw response format');
}

/**
 * Send a message back to AndOR server
 */
async function sendToAndOR(conversationId, content) {
  console.log(`[AndOR] Sending response to ${conversationId}`);
  
  await httpRequest(`${ANDOR_URL}/api/openclaw/message`, {
    method: 'POST'
  }, {
    conversationId,
    content,
    senderName: BRIDGE_NAME,
    senderId: 'openclaw-bot'
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

      // Validate required fields
      if (data.type === 'message' && data.data) {
        const { messageId, content, senderId, senderName, conversationId, timestamp } = data.data;

        stats.messagesReceived++;
        stats.lastMessage = `${senderName}: ${content.substring(0, 50)}${content.length > 50 ? '...' : ''}`;

        // Don't respond to our own messages
        if (senderId === 'openclaw-bot' || senderName === BRIDGE_NAME) {
          console.log(`[Webhook] Ignoring own message`);
          res.writeHead(200);
          res.end(JSON.stringify({ status: 'ignored', reason: 'own_message' }));
          return;
        }

        // Process the message
        res.writeHead(200);
        res.end(JSON.stringify({ status: 'received' }));

        // Get response from OpenClaw (async)
        stats.processing = true;
        stats.processingSince = new Date().toISOString();
        try {
          const response = await askOpenClaw(content, conversationId, senderName);
          
          // Send response back to AndOR
          await sendToAndOR(conversationId, response);
        } catch (error) {
          console.error(`[Error] Failed to process message:`, error.message);
          stats.errors++;
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
    andor: ANDOR_URL
  }));
}

// Dashboard endpoint
function handleDashboard(req, res) {
  const fs = require('fs');
  const path = require('path');
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

// Status page with activity
function handleStatus(req, res) {
  const processingTime = stats.processing && stats.processingSince 
    ? Math.floor((Date.now() - new Date(stats.processingSince).getTime()) / 1000) 
    : 0;
  
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
    .idle { color: #666; }
  </style>
</head>
<body>
  <h1>🧩 AndOR ↔ OpenClaw Bridge</h1>
  <div class="card ${stats.processing ? 'processing' : ''}">
    <div class="stat">
      <span class="label">Status</span>
      <span class="${stats.processing ? 'processing-indicator' : 'value pulse'}">
        ${stats.processing ? '⚡ PROCESSING (' + processingTime + 's)' : '● Running'}
      </span>
    </div>
    <div class="stat"><span class="label">Started</span><span class="value">${stats.started}</span></div>
    <div class="stat"><span class="label">Uptime</span><span class="value">${Math.floor((Date.now() - new Date(stats.started).getTime()) / 1000)}s</span></div>
  </div>
  <div class="card">
    <div class="stat"><span class="label">Messages Received</span><span class="value">${stats.messagesReceived}</span></div>
    <div class="stat"><span class="label">Messages Sent</span><span class="value">${stats.messagesSent}</span></div>
    <div class="stat"><span class="label">Errors</span><span class="value ${stats.errors > 0 ? 'error' : ''}">${stats.errors}</span></div>
  </div>
  <div class="card">
    <div class="stat"><span class="label">Last Message</span><span class="value" style="font-size: 12px; max-width: 350px; overflow: hidden; text-overflow: ellipsis;">${stats.lastMessage || 'none'}</span></div>
    <div class="stat"><span class="label">Last Error</span><span class="value error">${stats.lastError || 'none'}</span></div>
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
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(stats));
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
});

// Graceful shutdown
process.on('SIGTERM', () => {
  console.log('[Bridge] Shutting down...');
  server.close(() => process.exit(0));
});
