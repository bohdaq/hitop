import requests

url = "https://cdn.example.test/assets/app.js"
headers = {
    "X-Tag": "b",
}

response = requests.request("PURGE", url, headers=headers, auth=("svc", ""))
print(response.status_code)
print(response.text)
