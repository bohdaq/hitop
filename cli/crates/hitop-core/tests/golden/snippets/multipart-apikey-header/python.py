import requests

url = "https://api.example.test/upload"
headers = {
    "X-API-Key": "k-123",
}
files = [
    ("title", (None, "@not-a-file")),
    ("avatar", ("photo.png", open("photo.png", "rb"))),
]

response = requests.request("POST", url, headers=headers, files=files)
print(response.status_code)
print(response.text)
