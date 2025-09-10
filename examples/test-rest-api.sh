#!/bin/bash

# Test script for LINO REST API
# This script tests the REST API functionality with various LINO queries

BASE_URL="http://localhost:5000"
SERVER_PID=""

# Function to start the server
start_server() {
    echo "Starting LINO REST API server..."
    dotnet run --project ../Foundation.Data.Doublets.Cli -- serve --port 5000 > server.log 2>&1 &
    SERVER_PID=$!
    echo "Server started with PID: $SERVER_PID"
    sleep 5  # Give server time to start
}

# Function to stop the server
stop_server() {
    if [ ! -z "$SERVER_PID" ]; then
        echo "Stopping server with PID: $SERVER_PID"
        kill $SERVER_PID 2>/dev/null || true
        wait $SERVER_PID 2>/dev/null || true
    fi
}

# Function to test API endpoint
test_api() {
    local method="$1"
    local endpoint="$2"
    local data="$3"
    local description="$4"
    
    echo "Testing: $description"
    echo "Method: $method, Endpoint: $endpoint"
    
    if [ "$method" = "GET" ]; then
        response=$(curl -s -w "%{http_code}" "$BASE_URL$endpoint")
    else
        response=$(curl -s -w "%{http_code}" -X "$method" -H "Content-Type: application/json" -d "$data" "$BASE_URL$endpoint")
    fi
    
    http_code="${response: -3}"
    body="${response%???}"
    
    echo "HTTP Code: $http_code"
    echo "Response: $body"
    echo "---"
}

# Cleanup function
cleanup() {
    stop_server
    rm -f server.log
}

# Set trap for cleanup
trap cleanup EXIT

# Start the server
start_server

# Test 1: Get all links (empty database)
test_api "GET" "/api/links" "" "Get all links from empty database"

# Test 2: Create links
test_api "POST" "/api/links" '{"query":"() ((1 1) (2 2))"}' "Create two links"

# Test 3: Get all links (should show created links)
test_api "GET" "/api/links" "" "Get all links after creation"

# Test 4: Update links
test_api "PUT" "/api/links" '{"query":"((1: 1 1)) ((1: 1 2))"}' "Update first link"

# Test 5: Execute arbitrary query
test_api "POST" "/api/links/query" '{"query":"((($i: $s $t)) (($i: $s $t)))", "trace": true}' "Execute read query with trace"

# Test 6: Delete links
test_api "DELETE" "/api/links" '{"query":"((1 2)) ()"}' "Delete link"

echo "All tests completed!"