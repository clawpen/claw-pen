#!/usr/bin/env node
// Test WebSocket auth directly
const WebSocket = require('ws');
const nacl = require('tweetnacl');
const crypto = require('crypto');

const BASE64 = {
    encode: (bytes) => Buffer.from(bytes).toString('base64'),
    decode: (str) => Buffer.from(str, 'base64')
};

// SHA256 hash helper
function sha256(bytes) {
    return crypto.createHash('sha256').update(bytes).digest('hex');
}

// Generate test keys - use seed format like ed25519-dalek
const seed = nacl.randomBytes(32);
const keyPair = nacl.sign.keyPair.fromSeed(seed);
// Device ID is SHA256 hash of public key
const deviceId = sha256(keyPair.publicKey);

const port = '18790';
const ws = new WebSocket(`ws://localhost:${port}/ws`, {
    headers: {
        'Origin': `http://localhost:${port}`
    }
});

ws.on('open', () => {
    console.log('Connected, waiting for challenge...');
});

ws.on('message', (data) => {
    const msg = JSON.parse(data.toString());
    console.log('Received:', JSON.stringify(msg, null, 2).substring(0, 200));
    
    if (msg.event === 'connect.challenge') {
        const nonce = msg.payload?.nonce;
        console.log('\n=== Challenge received ===');
        console.log('Nonce:', nonce);
        console.log('Device ID:', deviceId);
        console.log('Public Key:', BASE64.encode(keyPair.publicKey));
        
        const signedAt = Date.now();
        const scopes = 'operator.admin,operator.approvals,operator.pairing';
        const message = `v2|${deviceId}|openclaw-control-ui|webchat|operator|${scopes}|${signedAt}||${nonce}`;
        
        console.log('\n=== Signing message ===');
        console.log('Message:', message);
        
        const signature = nacl.sign.detached(Buffer.from(message, 'utf8'), keyPair.secretKey);
        
        const response = {
            type: 'req',
            id: 'cp-' + Date.now(),
            method: 'connect',
            params: {
                minProtocol: 3,
                maxProtocol: 3,
                client: {
                    id: 'openclaw-control-ui',
                    version: '1.0.0',
                    platform: 'desktop',
                    mode: 'webchat'
                },
                role: 'operator',
                scopes: ['operator.admin', 'operator.approvals', 'operator.pairing'],
                device: {
                    id: deviceId,
                    publicKey: BASE64.encode(keyPair.publicKey),
                    signature: BASE64.encode(signature),
                    signedAt: signedAt,
                    nonce: nonce
                },
                caps: [],
                commands: []
            }
        };
        
        console.log('\n=== Sending response ===');
        console.log(JSON.stringify(response, null, 2));
        ws.send(JSON.stringify(response));
    }
    
    if (msg.ok === true) {
        console.log('\n=== AUTHENTICATED! ===');
    }
    
    if (msg.ok === false) {
        console.log('\n=== AUTH FAILED ===');
        console.log('Error:', JSON.stringify(msg.error, null, 2));
    }
});

ws.on('error', (err) => {
    console.error('WebSocket error:', err);
});

ws.on('close', () => {
    console.log('Connection closed');
    process.exit(0);
});

setTimeout(() => {
    console.log('Timeout - no response');
    process.exit(1);
}, 10000);
