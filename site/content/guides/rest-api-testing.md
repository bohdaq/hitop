---
title: "REST API testing"
description: "Complete guide to REST API testing with HITOP. Learn how to test RESTful APIs, validate responses, and automate API workflows."
---

## What is REST API testing?

REST API testing is the process of validating RESTful web services to ensure they function correctly, return expected responses, and handle errors appropriately. It involves sending HTTP requests to API endpoints and verifying the responses.

## Understanding REST APIs

REST (Representational State Transfer) is an architectural style for designing networked applications. RESTful APIs use HTTP methods to perform operations on resources:

-   **GET:** Retrieve data from the server
-   **POST:** Create new resources
-   **PUT:** Update existing resources
-   **PATCH:** Partially update resources
-   **DELETE:** Remove resources
-   **HEAD:** Retrieve headers only
-   **OPTIONS:** Get supported methods

## Why test REST APIs?

REST API testing is crucial for:

-   **Functionality validation:** Ensure endpoints work as expected
-   **Data integrity:** Verify correct data is returned
-   **Error handling:** Test how APIs handle invalid inputs
-   **Performance:** Check response times and throughput
-   **Security:** Validate authentication and authorization
-   **Integration:** Ensure APIs work with other systems

## REST API testing with HITOP

HITOP provides a comprehensive environment for testing REST APIs with features designed specifically for RESTful services.

### Testing GET requests

GET requests retrieve data from the server. With HITOP, you can:

```
GET https://api.example.com/users/123
Headers:
  Authorization: Bearer your-token-here
  Accept: application/json
```

### Testing POST requests

POST requests create new resources. HITOP supports JSON, form data, and raw body formats:

```
POST https://api.example.com/users
Headers:
  Content-Type: application/json
  Authorization: Bearer your-token-here
Body:
{
  "name": "John Doe",
  "email": "john@example.com"
}
```

You can leave the `Content-Type` header out: when a POST, PUT or PATCH body has none, HITOP sends
`application/json` for a body that starts with `{` or `[`, `text/html` for an HTML page,
`application/xml` for other `<...>` and `text/plain` for anything else. The CLI, the MCP server, the
web app, the desktop app and the iOS companion all do it the same way, `hitop snippet` shows the
header, and it is never written into your collection. A header of your own always wins. A URL typed
without a scheme, such as `api.example.com/users`, is sent as `https://api.example.com/users`
(`http://` for `localhost` and `127.0.0.1`).

### Testing PUT and PATCH requests

Update existing resources with PUT (full update) or PATCH (partial update):

```
PATCH https://api.example.com/users/123
Headers:
  Content-Type: application/json
Body:
{
  "email": "newemail@example.com"
}
```

### Testing DELETE requests

Remove resources from the server:

```
DELETE https://api.example.com/users/123
Headers:
  Authorization: Bearer your-token-here
```

## Key aspects of REST API testing

### 1. Status code validation

Verify the API returns appropriate HTTP status codes:

-   **200 OK:** Successful GET, PUT, PATCH
-   **201 Created:** Successful POST
-   **204 No Content:** Successful DELETE
-   **400 Bad Request:** Invalid input
-   **401 Unauthorized:** Authentication required
-   **404 Not Found:** Resource doesn't exist
-   **500 Internal Server Error:** Server error

### 2. Response body validation

Check that the response contains expected data structure and values. HITOP's syntax highlighting makes it easy to inspect JSON responses.

### 3. Header validation

Verify response headers including `Content-Type`, `Cache-Control`, and custom headers.

### 4. Authentication testing

Test various authentication methods:

-   Bearer token authentication
-   Basic authentication
-   API key authentication
-   OAuth 2.0 flows

## Advanced REST API testing

### Collection variables

Use collection variables to switch the server or user in one place:

```
${base_url}/users/${user_id}

Variables:
  base_url: https://api.staging.example.com
  user_id: 123
```

### Script before

In the Script before tab, run JavaScript before the request is sent to set up authentication, generate timestamps, or modify request data.

### Script after

In the Script after tab, validate responses, extract data for subsequent requests, or perform assertions.

### Collection runner

Run multiple API tests sequentially to test complete workflows:

1.  Create user (POST)
2.  Retrieve user (GET)
3.  Update user (PATCH)
4.  Delete user (DELETE)

> ### Best practice
>
> Organize your REST API tests into collections by resource or feature. This makes it easy to run related tests together and share with team members.

## Common REST API testing scenarios

### CRUD operations testing

Test Create, Read, Update, Delete operations for each resource in your API.

### Query parameters

Test filtering, sorting, and pagination:

```
GET https://api.example.com/users?page=2&limit=10&sort=name
```

### Error handling

Verify your API handles errors gracefully:

-   Invalid data formats
-   Missing required fields
-   Unauthorized access
-   Resource not found

### Edge cases

Test boundary conditions, empty responses, large payloads, and special characters.

## REST API testing checklist

-   ✓ Test all HTTP methods (GET, POST, PUT, PATCH, DELETE)
-   ✓ Verify status codes for success and error cases
-   ✓ Validate response body structure and data
-   ✓ Check response headers
-   ✓ Test authentication and authorization
-   ✓ Verify error messages are helpful
-   ✓ Test query parameters and filters
-   ✓ Validate pagination
-   ✓ Test with invalid or malformed data
-   ✓ Check response times

## Why use HITOP for REST API testing?

-   **Complete HTTP support:** All REST methods supported
-   **JSON formatting:** Beautiful syntax highlighting
-   **Collections:** Organize tests by resource
-   **Variables:** Test across environments
-   **Scripting:** Automate complex workflows
-   **Privacy:** Collections and history stay on your device
-   **No account:** Start testing immediately
-   **History:** Track all your requests
