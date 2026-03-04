#!/usr/bin/env node
/**
 * Seed Team Agent Users in AndOR Hub Database
 * 
 * Creates user accounts for each team role agent.
 * These accounts work for both chat AND git access.
 * 
 * Usage:
 *   node seed-team-agents.js /path/to/local_chat.db
 * 
 * Or with environment variable:
 *   ANDOR_DB=/path/to/local_chat.db node seed-team-agents.js
 */

const Database = require('better-sqlite3');
const crypto = require('crypto');
const path = require('path');
const fs = require('fs');

// Team agent definitions - must match templates/agents.yaml
const TEAM_AGENTS = [
  {
    id: 'agent-alex-pm',
    username: 'alex-pm',
    display_name: 'Alex (PM)',
    password: 'alex-pm-secret',  // Change in production!
    emoji: '📋',
    triggers: ['alex', 'pm', 'project manager'],
    role: 'pm'
  },
  {
    id: 'agent-dan-dev',
    username: 'dan-dev',
    display_name: 'Dan (Dev)',
    password: 'dan-dev-secret',
    emoji: '💻',
    triggers: ['dan', 'dev', 'developer'],
    role: 'developer'
  },
  {
    id: 'agent-sam-qa',
    username: 'sam-qa',
    display_name: 'Sam (QA)',
    password: 'sam-qa-secret',
    emoji: '🔍',
    triggers: ['sam', 'qa', 'tester'],
    role: 'qa'
  },
  {
    id: 'agent-diana-design',
    username: 'diana-design',
    display_name: 'Diana (Design)',
    password: 'diana-design-secret',
    emoji: '🎨',
    triggers: ['diana', 'design', 'designer', 'ux'],
    role: 'designer'
  },
  {
    id: 'agent-taylor-devops',
    username: 'taylor-devops',
    display_name: 'Taylor (DevOps)',
    password: 'taylor-devops-secret',
    emoji: '🚀',
    triggers: ['taylor', 'devops', 'infra', 'sre'],
    role: 'devops'
  },
  {
    id: 'agent-morgan-security',
    username: 'morgan-security',
    display_name: 'Morgan (Security)',
    password: 'morgan-security-secret',
    emoji: '🔒',
    triggers: ['morgan', 'security', 'sec'],
    role: 'security'
  },
  {
    id: 'agent-jordan-architect',
    username: 'jordan-architect',
    display_name: 'Jordan (Architect)',
    password: 'jordan-architect-secret',
    emoji: '🏛️',
    triggers: ['jordan', 'architect', 'architecture'],
    role: 'architect'
  }
];

// Simple password hash (matches AndOR Hub's auth.ts)
function hashPassword(password) {
  return crypto.createHash('sha256').update(password).digest('hex');
}

// Generate recovery code
function generateRecoveryCode() {
  return crypto.randomBytes(8).toString('hex').toUpperCase();
}

async function main() {
  const dbPath = process.env.ANDOR_DB || process.argv[2];
  
  if (!dbPath) {
    console.error('Usage: node seed-team-agents.js <path-to-local_chat.db>');
    console.error('   or: ANDOR_DB=/path/to/db node seed-team-agents.js');
    process.exit(1);
  }
  
  if (!fs.existsSync(dbPath)) {
    console.error(`Database not found: ${dbPath}`);
    process.exit(1);
  }
  
  console.log(`Seeding team agents to: ${dbPath}`);
  
  const db = new Database(dbPath);
  
  // Prepare statements
  const insertUser = db.prepare(`
    INSERT OR REPLACE INTO users (id, username, display_name, password_hash, recovery_code)
    VALUES (?, ?, ?, ?, ?)
  `);
  
  const getUser = db.prepare('SELECT * FROM users WHERE username = ?');
  
  // Seed each agent
  const results = [];
  
  for (const agent of TEAM_AGENTS) {
    const existing = getUser.get(agent.username);
    
    if (existing) {
      console.log(`  [exists] ${agent.username} (${agent.display_name})`);
      results.push({ ...agent, status: 'exists' });
    } else {
      const passwordHash = hashPassword(agent.password);
      const recoveryCode = generateRecoveryCode();
      
      insertUser.run(
        agent.id,
        agent.username,
        agent.display_name,
        passwordHash,
        recoveryCode
      );
      
      console.log(`  [created] ${agent.username} (${agent.display_name})`);
      console.log(`           Password: ${agent.password}`);
      console.log(`           Recovery: ${recoveryCode}`);
      results.push({ ...agent, status: 'created', recoveryCode });
    }
  }
  
  db.close();
  
  // Output agents.json for the bridge
  const agentsJson = {};
  for (const agent of TEAM_AGENTS) {
    agentsJson[agent.role] = {
      agentId: agent.id,
      displayName: agent.display_name,
      triggers: agent.triggers,
      emoji: agent.emoji,
      password: agent.password,
      description: `${agent.role} role agent`
    };
  }
  
  const agentsJsonPath = path.join(path.dirname(dbPath), 'team-agents.json');
  fs.writeFileSync(agentsJsonPath, JSON.stringify(agentsJson, null, 2));
  console.log(`\nWrote bridge config to: ${agentsJsonPath}`);
  
  // Summary
  console.log('\n=== Summary ===');
  console.log(`Created: ${results.filter(r => r.status === 'created').length}`);
  console.log(`Existing: ${results.filter(r => r.status === 'exists').length}`);
  console.log('\nAgents can now:');
  console.log('  - Chat in AndOR Hub (trigger by @mention or channel name)');
  console.log('  - Push/pull from git repos (use username + password)');
}

main().catch(err => {
  console.error('Error:', err);
  process.exit(1);
});
