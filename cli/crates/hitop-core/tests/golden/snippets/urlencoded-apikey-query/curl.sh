curl -X PUT 'https://api.example.test/profile?api_key={{key}}' \
  -H 'Content-Type: application/x-www-form-urlencoded' \
  --data-raw 'name=Ada%20Lovelace&note=a%26b%3Dc%2F%C3%A9'
