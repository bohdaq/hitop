import requests

url = "https://cdn.example.test/assets/app.js"
headers = {
    "X-Tag": "a, b",
}

response = requests.request("PURGE", url, headers=headers, auth=("svc", ""))
print(response.status_code)
print(response.text)
