# Authentication Guide for Claw Pen Orchestrator

The Claw Pen orchestrator uses JWT (JSON Web Token) based authentication to protect all API endpoints.

## Overview

- **JWT-based authentication** with access and refresh tokens
- **Argon2** password hashing for secure password storage
- **Single admin user** model (simplified for initial implementation)
- **Automatic JWT secret generation** on first run

## Quick Start

### Setting the Initial Password

There are two ways to set the initial admin password:

#### Method 1: CLI (Recommended)

Run the orchestrator with the `--set-password` flag:

```bash
./claw-pen-orchestrator --set-password
```

You'll be prompted to enter and confirm the password.

#### Method 2: Registration Endpoint

1. Enable registration by setting the environment variable:
   ```bash
   export ENABLE_REGISTRATION=true
   ```

2. Start the orchestrator

3. Register the admin user:
   ```bash
   curl -X POST http://localhost:3000/auth/register \
     -H "Content-Type: application/json" \
     -d '{"password": "your-secure-password"}'
   ```

4. **Important**: Disable registration after setup:
   ```bash
   unset ENABLE_REGISTRATION
   ```

### Authenticating

Once you have a password set, obtain a JWT token:

```bash
curl -X POST http://localhost:3000/auth/login \
  -H "Content-Type: application/json" \
  -d '{"password": "your-password"}'
```

Response:
```json
{
  "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "refresh_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "token_type": "Bearer",
  "expires_in": 86400
}
```

### Using the Token

For all API requests, include the access token in the Authorization header:

```bash
curl http://localhost:3000/api/agents \
  -H "Authorization: Bearer <your-access-token>"
```

### WebSocket Authentication

For WebSocket connections, pass the token as a query parameter:

```javascript
const ws = new WebSocket('ws://localhost:3000/api/agents/{id}/chat?token=<your-access-token>');
```

### Refreshing Tokens

Access tokens expire after 24 hours. Use the refresh token to get a new access token:

```bash
curl -X POST http://localhost:3000/api/auth/refresh \
  -H "Content-Type: application/json" \
  -d '{"refresh_token": "<your-refresh-token>"}'
```

## API Endpoints

### Public Endpoints (No Authentication Required)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/auth/login` | POST | Authenticate and get tokens |
| `/auth/register` | POST | Register admin (disabled by default) |
| `/auth/status` | GET | Check auth configuration |

### Protected Endpoints (JWT Required)

All `/api/*` endpoints require authentication:

- `/api/agents/*` - Agent management
- `/api/keys/*` - API key management
- `/api/templates` - Template listing
- `/api/projects/*` - Project management
- `/api/teams/*` - Team management
- `/api/metrics` - Metrics collection
- `/api/system/stats` - System statistics
- `/api/runtime/status` - Runtime status

## Token Lifetime

| Token Type | Lifetime |
|------------|----------|
| Access Token | 24 hours |
| Refresh Token | 7 days |

## Security Notes

1. **JWT Secret**: Generated automatically on first run and stored in `/data/claw-pen/data/jwt_secret` with 0600 permissions

2. **Password Storage**: Passwords are hashed using Argon2 and stored in `/data/claw-pen/data/admin_password` with 0600 permissions

3. **HTTPS**: In production, always use HTTPS to protect tokens in transit

4. **Token Storage**: Store tokens securely on the client side (e.g., in secure cookies or browser storage with appropriate protections)

5. **Registration**: The `/auth/register` endpoint is **disabled by default**. Only enable it temporarily for initial setup.

## Checking Auth Status

```bash
curl http://localhost:3000/auth/status
```

Response:
```json
{
  "auth_enabled": true,
  "has_admin": true,
  "registration_enabled": false
}
```

## Error Responses

### 401 Unauthorized

```json
{
  "error": "Missing authorization header"
}
```

```json
{
  "error": "Invalid or expired token"
}
```

### 403 Forbidden

```json
{
  "error": "Registration is disabled"
}
```

### 409 Conflict

```json
{
  "error": "User already exists"
}
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ENABLE_REGISTRATION` | `false` | Enable the `/auth/register` endpoint |

## Troubleshooting

### "No admin password set" Warning

If you see this warning on startup:
```
⚠️  No admin password set. Use --set-password to set one
```

Run the password setup:
```bash
./claw-pen-orchestrator --set-password
```

### Token Expired

If your token expires, use the refresh token:
```bash
curl -X POST http://localhost:3000/api/auth/refresh \
  -H "Content-Type: application/json" \
  -d '{"refresh_token": "<refresh-token>"}'
```

If the refresh token also expired, you'll need to log in again.

### Lost Password

If you lose the admin password:

1. Stop the orchestrator
2. Delete the password file: `rm /data/claw-pen/data/admin_password`
3. Start the orchestrator
4. Set a new password using `--set-password` or enable registration temporarily
