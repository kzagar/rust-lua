# Telekom 5G Router API Documentation

This document describes how to interact with the Telekom 5G Router (assumed
model using the identified API) via `curl`.

## Base URL

The API is generally accessible at `http://SERVER_ADDRESS/web/v1`.

## Authentication

The router uses Token-based authentication. You must first log in to obtain an
`Authorization` token (Bearer token).

### Login

**Endpoint:** `POST /web/v1/user/login`

**Headers:**

- `Content-Type: application/json`

**Payload:**

```json
{
  "username": "admin",
  "password": "YOUR_PASSWORD"
}
```

**Response:** Returns a JSON object containing the `Authorization` token in
`data.Authorization`. Example: `Bearer eyJhbGci...`

## Port Forwarding

**Endpoint:** `/web/v1/setting/firewall/portforwarding`

### List All Rules

**Method:** `GET` **Headers:**

- `Authorization: Bearer <TOKEN>`

### Add a Rule

**Method:** `POST` **Headers:**

- `Authorization: Bearer <TOKEN>`
- `Content-Type: application/json`

**Payload:**

```json
{
  "PortForwardings": [
    {
      "Application": "AppName",
      "PortFrom": "8080",
      "Protocol": "TCP",
      "IpAddress": "192.168.0.100",
      "PortTo": "80",
      "Enable": true,
      "IndexId": "",
      "OperateType": "insert"
    }
  ]
}
```

### Delete a Rule

**Method:** `DELETE` **Headers:**

- `Authorization: Bearer <TOKEN>`
- `Content-Type: application/json`

**Payload:**

```json
{
  "PortForwardings": [
    {
      "IndexId": "<ID_FROM_LIST_COMMAND>",
      "OperateType": "delete"
    }
  ]
}
```

_Note: You must obtain the `IndexId` from the "List All Rules" response._
