curl -X POST 'https://api.example.test/items' \
  -H 'Content-Type: application/json' \
  -u 'ada:p'\''w"d' \
  --data-raw '{
  "name": "it'\''s \"quoted\"",
  "path": "C:\\tmp",
  "shell": "$HOME `id`",
  "city": "Kraków"
}'
