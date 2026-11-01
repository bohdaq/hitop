curl -X POST 'https://api.example.test/upload' \
  -H 'X-API-Key: k-123' \
  --form-string 'title=@not-a-file' \
  -F 'avatar=@"photo.png"'
