import requests

url = "https://api.example.test/tenants/{{tenant}}/note"
headers = {
    "X-Trace": "t-1",
    "Authorization": "Bearer {{token}}",
}
data = "plain text\twith a tab".encode("utf-8")

response = requests.request("PATCH", url, headers=headers, data=data)
print(response.status_code)
print(response.text)
