import requests

url = "https://api.example.test/users?page=2&q=a%20b"
headers = {
    "Accept": "application/json",
    "Authorization": "Bearer tok-123",
}

response = requests.request("GET", url, headers=headers)
print(response.status_code)
print(response.text)
