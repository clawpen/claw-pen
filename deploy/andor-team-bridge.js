#!/usr/bin/env node
/**
 * AndOR ↔ OpenClaw Team Bridge
 * 
 * Multi-agent bridge that routes messages to appropriate team role agents.
 * Each agent has its own personality, triggers, and OpenClaw session.
 * 
 * Usage:
 *   node andor-team-bridge.js
 * 
 * Environment:
 *   PORT - Bridge server port (default: 3456)
 *   OPENCLAW_GATEWAY_URL - OpenClaw gateway URL (default: http://127.0.0.1:18789)
 *   ANDOR_SERVER_URL - AndOR server URL (default: https://aesir.tailb0b4db.ts.net:8080)
 *   TEAM_AGENTS_CONFIG - Path to team-agents.json (default: ./team-agents.json)
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
const TEAM_AGENTS_CONFIG = process.env.TEAM_AGENTS_CONFIG || path.join(__dirname, 'team-agents.json');

// Default agents (fallback if config not found)
const DEFAULT_AGENTS = {
  codi: {
    agentId: 'main',
    displayName: 'Codi',
    triggers: ['codi', 'openclaw'],
    emoji: '🧩',
    personality: 'You are Codi, a helpful AI assistant.'
  }
};

// Role personalities (from templates/agents.yaml)
const ROLE_PERSONALITIES = {
  pm: `You are Alex, a project manager. You focus on:
- Clear requirements and acceptance criteria
- Prioritization and scope management
- Timeline and milestone tracking
- Stakeholder communication
- Unblocking team members
Be concise, action-oriented, and always ask "what do we need to ship?"`,

  developer: `You are Dan, a senior developer. You focus on:
- Clean, maintainable code
- Architecture and design patterns
- Performance and scalability
- Code review and best practices
- Technical debt management
Be pragmatic, explain trade-offs, and prefer simple solutions over clever ones.`,

  qa: `You are Sam, a QA engineer. You focus on:
- Test coverage and test strategies
- Edge cases and boundary conditions
- Regression prevention
- Bug reproduction and reporting
- Quality metrics and monitoring
Always ask "what could go wrong?" and think about failure modes.`,

  designer: `You are Diana, a UX designer. You focus on:
- User experience and user flows
- Visual design and consistency
- Accessibility and inclusivity
- Design systems and components
- User research and feedback
Always advocate for the user and ask "how will this feel to use?"`,

  devops: `You are Taylor, a DevOps engineer. You focus on:
- CI/CD pipelines and automation
- Infrastructure as code
- Monitoring and observability
- Security and compliance
- Cost optimization
Always think about "how does this run in production?" and prefer automation.`,

  security: `You are Morgan, a security engineer. You focus on:
- Threat modeling and risk assessment
- Security hardening and best practices
- Vulnerability identification
- Compliance and audit trails
- Incident response planning
Always ask "what's the worst case?" and think like an attacker.`,

  architect: `You are Jordan, a software architect. You focus on:
- System design and integration patterns
- Scalability and performance
- Technology selection and evaluation
- Technical vision and roadmaps
- Cross-cutting concerns (logging, caching, etc.)
Think big picture but stay practical. Document decisions and their rationale.`
};

// Load agents config
function loadAgentsConfig() {
  // Try specified path first
  if (fs.existsSync(TEAM_AGENTS_CONFIG)) {
    try {
      const config = JSON.parse(fs.readFileSync(TEAM_AGENTS_CONFIG, 'utf8'));
      console.log(`[Bridge] Loaded ${Object.keys(config).length} agents from ${TEAM_AGENTS_CONFIG}`);
      return config;
    } catch (e) {
      console.warn(`[Bridge] Failed to load ${TEAM_AGENTS_CONFIG}: ${e.message}`);
    }
  }
  
  // Try team-agents.json in same directory
  const localPath = path.join(__dirname, 'team-agents.json');
  if (fs.existsSync(localPath)) {
    try {
      const config = JSON.parse(fs.readFileSync(localPath, 'utf8'));
      console.log(`[Bridge] Loaded ${Object.keys(config).length} agents from ${localPath}`);
      return config;
    } catch (e) {
      console.warn(`[Bridge] Failed to load ${localPath}: ${e.message}`);
    }
  }
  
  // Fallback to agents.json (old format)
  const oldPath = path.join(__dirname, 'agents.json');
  if (fs.existsSync(oldPath)) {
    try {
      const config = JSON.parse(fs.readFileSync(oldPath, 'utf8'));
      console.log(`[Bridge] Loaded ${Object.keys(config).length} agents from ${oldPath} (legacy format)`);
      return config;
    } catch (e) {
      console.warn(`[Bridge] Failed to load ${oldPath}: ${e.message}`);
    }
  }
  
  console.warn('[Bridge] No agent config found, using defaults');
  return DEFAULT_AGENTS;
}

const AGENTS = loadAgentsConfig();

// Detect mode: "isolated" (one container per role) or "multiplexed" (one container for all)
const BRIDGE_MODE = AGENTS._mode || 'isolated';
const MULTIPLEXED_AGENT = AGENTS._multiplexed;

console.log(`[Bridge] Mode: ${BRIDGE_MODE}${BRIDGE_MODE === 'multiplexed' ? ' (single agent, role switching)' : ' (one container per role)'}`);

// Build trigger -> agent mapping for fast lookup
const TRIGGER_MAP = {};
for (const [key, agent] of Object.entries(AGENTS)) {
  for (const trigger of agent.triggers || [key]) {
    TRIGGER_MAP[trigger.toLowerCase()] = key;
  }
}

console.log(`[Bridge] Starting AndOR ↔ OpenClaw Team Bridge`);
console.log(`[Bridge] Listening on port ${PORT}`);
console.log(`[Bridge] OpenClaw: ${OPENCLAW_URL}`);
console.log(`[Bridge] AndOR: ${ANDOR_URL}`);
console.log(`[Bridge] Team agents: ${Object.entries(AGENTS).map(([k, a]) => `${a.emoji || '🤖'} ${a.displayName}`).join(', ')}`);

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
 * Returns { agentKey, agent, triggeredBy } or null
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
  const displayName = agent.displayName || agentKey;
  const role = agent.role || agentKey;
  
  // Get role-specific personality
  const personality = ROLE_PERSONALITIES[role] || agent.personality || 
    `You are ${displayName}, a helpful AI assistant.`;
  
  // Determine agent ID based on mode
  let agentId;
  let sessionKey;
  
  if (BRIDGE_MODE === 'multiplexed' && MULTIPLEXED_AGENT) {
    // Single container, role switching via system prompt
    agentId = MULTIPLEXED_AGENT.agentId || 'team-multi';
    // Role-based session for context isolation (same agent, different memory)
    sessionKey = `andor:${conversationId}:${role}`;
    console.log(`[OpenClaw] [multiplexed] ${displayName} (${role}) from ${senderName}`);
  } else {
    // Isolated mode: one container per role
    agentId = agent.agentId || agentKey;
    sessionKey = `andor:${conversationId}:${role}`;
    console.log(`[OpenClaw] [isolated] ${displayName} (${role}) from ${senderName}`);
  }
  
  const headers = {
    'Content-Type': 'application/json'
  };
  
  if (OPENCLAW_TOKEN) {
    headers['Authorization'] = `Bearer ${OPENCLAW_TOKEN}`;
  }
  
  // Set agent ID and session key
  headers['x-openclaw-agent-id'] = agentId;
  headers['x-openclaw-session-key'] = sessionKey;
  
  const response = await httpRequest(`${OPENCLAW_URL}/v1/chat/completions`, {
    method: 'POST',
    headers
  }, {
    model: 'openclaw',
    messages: [
      {
        role: 'system',
        content: personality
      },
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
      displayName,
      role
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
    senderId: `openclaw-${displayName.toLowerCase().replace(/[^a-z0-9]/g, '-')}`
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
        const normalizedSender = senderName.toLowerCase().replace(/[^a-z0-9]/g, '-');
        const normalizedAgent = agent.displayName.toLowerCase().replace(/[^a-z0-9]/g, '-');
        if (senderId === `openclaw-${normalizedAgent}` || normalizedSender === normalizedAgent) {
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
    service: 'andor-team-bridge',
    openclaw: OPENCLAW_URL,
    andor: ANDOR_URL,
    agents: Object.keys(AGENTS).map(k => AGENTS[k].displayName)
  }));
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
    triggerMap: TRIGGER_MAP,
    roles: Object.keys(ROLE_PERSONALITIES)
  }));
}

// Status page with activity
function handleStatus(req, res) {
  const processingTime = stats.processing && stats.processingSince 
    ? Math.floor((Date.now() - new Date(stats.processingSince).getTime()) / 1000) 
    : 0;
  
  const agentStatsHtml = Object.entries(stats.perAgent).map(([key, data]) => `
    <div class="stat"><span class="label">${AGENTS[key]?.emoji || '🤖'} ${AGENTS[key]?.displayName || key}</span><span class="value">${data.sent} sent / ${data.received} recv</span></div>
  `).join('');
  
  const agentBadges = Object.values(AGENTS).map(a => 
    `<span class="agent-badge">${a.emoji || '🤖'} ${a.displayName}</span>`
  ).join('');
  
  const html = `
<!DOCTYPE html>
<html>
<head>
  <title>AndOR Team Bridge Status</title>
  <meta http-equiv="refresh" content="2">
  <style>
    body { font-family: system-ui; max-width: 700px; margin: 40px auto; padding: 20px; background: #1a1a2e; color: #eee; }
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
    .agent-badge { display: inline-block; padding: 4px 10px; background: #238636; border-radius: 12px; margin: 2px; font-size: 12px; }
    .card-title { font-size: 12px; color: #888; margin-bottom: 8px; text-transform: uppercase; letter-spacing: 1px; }
  </style>
</head>
<body>
  <h1>🧩 AndOR ↔ OpenClaw Team Bridge</h1>
  <div class="card">
    <div class="card-title">Team Agents</div>
    <div class="stat"><span class="label">Active</span><span class="value">${agentBadges}</span></div>
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
    <div class="card-title">Per-Agent Stats</div>
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
  } else if (url.pathname === '/status' || url.pathname === '/') {
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
  console.log(`[Bridge] Status: http://100.77.212.101:${PORT}/status`);
});

// Graceful shutdown
process.on('SIGTERM', () => {
  console.log('[Bridge] Shutting down...');
  server.close(() => process.exit(0));
});
