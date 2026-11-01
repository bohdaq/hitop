curl -X POST 'https://api.example.test/graphql' \
  -H 'Authorization: Bearer {{token}}' \
  -H 'Content-Type: application/json' \
  --data-raw '{"query":"query User($id: ID!) {\n  user(id: $id) { name }\n}\n","variables":{"id": "7"},"operationName":"User"}'
