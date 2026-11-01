curl -X PATCH 'https://api.example.test/tenants/{{tenant}}/note' \
  -H 'X-Trace: t-1' \
  -H 'Authorization: Bearer {{token}}' \
  -H 'Content-Type:' \
  --data-raw 'plain text	with a tab'
