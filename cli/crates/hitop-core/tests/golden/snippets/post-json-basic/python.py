import requests

url = "https://api.example.test/items"
headers = {
    "Content-Type": "application/json",
}
data = "{\n  \"name\": \"it's \\\"quoted\\\"\",\n  \"path\": \"C:\\\\tmp\",\n  \"shell\": \"$HOME `id`\",\n  \"city\": \"Kraków\"\n}".encode("utf-8")

response = requests.request("POST", url, headers=headers, data=data, auth=("ada", "p'w\"d"))
print(response.status_code)
print(response.text)
