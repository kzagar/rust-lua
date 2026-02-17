#!/bin/bash

# Source secrets if they exist
if [ -f ~/.secrets ]; then
    source ~/.secrets
fi

# Configuration
ROUTER_HOST="${ROUTER_HOST:-http://192.168.0.1}"
USERNAME="${TELEKOM_5G_USER:-admin}"
PASSWORD="${TELEKOM_5G_PASS:-yours3cret}"
COOKIE_FILE="/tmp/router_cookies.txt"
TOKEN_FILE="/tmp/router_token.txt"

# Helper function for JSON parsing
get_json_value() {
    local json="$1"
    local key="$2"
    echo "$json" | grep -o "\"$key\": *\"[^\"]*\"" | cut -d'"' -f4
}

# Login and get token
login() {
    echo "Logging in to $ROUTER_HOST..."
    response=$(curl -s -c "$COOKIE_FILE" -H "Content-Type: application/json" \
        -X POST "$ROUTER_HOST/web/v1/user/login" \
        -d "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\"}")
    
    token=$(echo "$response" | grep -o '"Authorization":"[^"]*"' | cut -d'"' -f4)
    
    if [ -z "$token" ]; then
        echo "Login failed. Response: $response"
        exit 1
    fi
    
    echo "$token" > "$TOKEN_FILE"
    echo "Login successful. Token saved."
}

# Ensure we have a token
get_token() {
    if [ ! -f "$TOKEN_FILE" ]; then
        login
    fi
    cat "$TOKEN_FILE"
}

# List all port forwardings
list_rules() {
    token=$(get_token)
    echo "Listing port forwarding rules..."
    curl -s -X GET \
        -H "Authorization: $token" \
        "$ROUTER_HOST/web/v1/setting/firewall/portforwarding" | \
    python3 -c "import sys, json; print(json.dumps(json.load(sys.stdin), indent=2))"
}

# Add a port forwarding rule
add_rule() {
    local app_name="$1"
    local port_from="$2"
    local protocol="$3"
    local ip_address="$4"
    local port_to="$5"
    local enable="$6"
    
    if [ -z "$app_name" ] || [ -z "$port_from" ] || [ -z "$protocol" ] || [ -z "$ip_address" ] || [ -z "$port_to" ]; then
        echo "Usage: $0 add <app_name> <port_from> <protocol: TCP/UDP> <ip_address> <port_to> <enable: true/false>"
        exit 1
    fi
    
    if [ -z "$enable" ]; then
        enable="true"
    fi
    
    token=$(get_token)
    echo "Adding rule for $app_name..."
    
    curl -s -X POST \
        -H "Authorization: $token" \
        -H "Content-Type: application/json" \
        -d "{\"PortForwardings\":[{\"Application\":\"$app_name\",\"PortFrom\":\"$port_from\",\"Protocol\":\"$protocol\",\"IpAddress\":\"$ip_address\",\"PortTo\":\"$port_to\",\"Enable\":$enable,\"IndexId\":\"\",\"OperateType\":\"insert\"}]}" \
        "$ROUTER_HOST/web/v1/setting/firewall/portforwarding"
    echo
}

# Delete a port forwarding rule
delete_rule() {
    local index_id="$1"
    
    if [ -z "$index_id" ]; then
        echo "Usage: $0 delete <index_id>"
        echo "Use 'list' command to find IndexId."
        exit 1
    fi
    
    token=$(get_token)
    echo "Deleting rule ID $index_id..."
    
    curl -s -X DELETE \
        -H "Authorization: $token" \
        -H "Content-Type: application/json" \
        -d "{\"PortForwardings\":[{\"IndexId\":\"$index_id\",\"OperateType\":\"delete\"}]}" \
        "$ROUTER_HOST/web/v1/setting/firewall/portforwarding"
    echo
}

# Main command handling
case "$1" in
    "login")
        login
        ;;
    "list")
        list_rules
        ;;
    "add")
        shift
        add_rule "$@"
        ;;
    "delete")
        shift
        delete_rule "$@"
        ;;
    *)
        echo "Usage: $0 {login|list|add|delete}"
        echo "  list"
        echo "  add <app_name> <port_from> <protocol> <ip_address> <port_to> [enable]"
        echo "  delete <index_id>"
        exit 1
        ;;
esac
