import requests

url = "https://api.example.test/graphql"
headers = {
    "Authorization": "Bearer {{token}}",
    "Content-Type": "application/json",
}
data = "{\"query\":\"query User($id: ID!) {\\n  user(id: $id) { name }\\n}\\n\",\"variables\":{\"id\": \"7\"},\"operationName\":\"User\"}".encode("utf-8")

response = requests.request("POST", url, headers=headers, data=data)
print(response.status_code)
print(response.text)
