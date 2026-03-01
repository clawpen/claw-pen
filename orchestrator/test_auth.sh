#!/bin/bash
# Test script for Claw Pen Orchestrator authentication
# Run this after starting the orchestrator

set -e

BASE_URL="http://localhost:3000"
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=== Claw Pen Auth Test Suite ===${NC}"
echo ""

# Test 1: Health check (public)
echo -e "${YELLOW}Test 1: Health check (public endpoint)${NC}"
HEALTH=$(curl -s "$BASE_URL/health")
if [ "$HEALTH" == "OK" ]; then
    echo -e "${GREEN}✓ Health check passed${NC}"
else
    echo -e "${RED}✗ Health check failed${NC}"
fi
echo ""

# Test 2: Auth status (public)
echo -e "${YELLOW}Test 2: Auth status (public endpoint)${NC}"
STATUS=$(curl -s "$BASE_URL/auth/status")
echo "Status: $STATUS"
echo ""

# Test 3: Unauthenticated request to protected endpoint
echo -e "${YELLOW}Test 3: Unauthenticated request to protected endpoint${NC}"
RESP=$(curl -s -o /dev/null -w "%{http_code}" "$BASE_URL/api/agents")
if [ "$RESP" == "401" ]; then
    echo -e "${GREEN}✓ Unauthenticated request correctly rejected (401)${NC}"
else
    echo -e "${RED}✗ Expected 401, got $RESP${NC}"
fi
echo ""

# Test 4: Login with wrong password
echo -e "${YELLOW}Test 4: Login with wrong password${NC}"
RESP=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE_URL/auth/login" \
    -H "Content-Type: application/json" \
    -d '{"password": "wrongpassword"}')
if [ "$RESP" == "401" ]; then
    echo -e "${GREEN}✓ Wrong password correctly rejected (401)${NC}"
else
    echo -e "${RED}✗ Expected 401, got $RESP${NC}"
fi
echo ""

# Test 5: Login with correct password
echo -e "${YELLOW}Test 5: Login with correct password${NC}"
echo -n "Enter admin password: "
read -s PASSWORD
echo ""

LOGIN_RESP=$(curl -s -X POST "$BASE_URL/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"password\": \"$PASSWORD\"}")

if echo "$LOGIN_RESP" | grep -q "access_token"; then
    echo -e "${GREEN}✓ Login successful${NC}"
    ACCESS_TOKEN=$(echo "$LOGIN_RESP" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)
    REFRESH_TOKEN=$(echo "$LOGIN_RESP" | grep -o '"refresh_token":"[^"]*"' | cut -d'"' -f4)
    echo "Access token: ${ACCESS_TOKEN:0:50}..."
else
    echo -e "${RED}✗ Login failed: $LOGIN_RESP${NC}"
    exit 1
fi
echo ""

# Test 6: Authenticated request
echo -e "${YELLOW}Test 6: Authenticated request to protected endpoint${NC}"
RESP=$(curl -s -o /dev/null -w "%{http_code}" "$BASE_URL/api/agents" \
    -H "Authorization: Bearer $ACCESS_TOKEN")
if [ "$RESP" == "200" ]; then
    echo -e "${GREEN}✓ Authenticated request successful (200)${NC}"
else
    echo -e "${RED}✗ Expected 200, got $RESP${NC}"
fi
echo ""

# Test 7: Refresh token
echo -e "${YELLOW}Test 7: Refresh token${NC}"
REFRESH_RESP=$(curl -s -X POST "$BASE_URL/api/auth/refresh" \
    -H "Content-Type: application/json" \
    -d "{\"refresh_token\": \"$REFRESH_TOKEN\"}")

if echo "$REFRESH_RESP" | grep -q "access_token"; then
    echo -e "${GREEN}✓ Token refresh successful${NC}"
    NEW_ACCESS_TOKEN=$(echo "$REFRESH_RESP" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)
    echo "New access token: ${NEW_ACCESS_TOKEN:0:50}..."
else
    echo -e "${RED}✗ Token refresh failed: $REFRESH_RESP${NC}"
fi
echo ""

# Test 8: Authenticated system stats
echo -e "${YELLOW}Test 8: Authenticated system stats${NC}"
STATS=$(curl -s "$BASE_URL/api/system/stats" \
    -H "Authorization: Bearer $ACCESS_TOKEN")
if echo "$STATS" | grep -q "total_memory_mb"; then
    echo -e "${GREEN}✓ System stats retrieved${NC}"
    echo "$STATS" | head -c 200
    echo "..."
else
    echo -e "${RED}✗ Failed to get system stats${NC}"
fi
echo ""

echo -e "${YELLOW}=== Test Suite Complete ===${NC}"
