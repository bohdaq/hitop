import requests

url = "https://api.example.test/profile?api_key={{key}}"
headers = {
    "Content-Type": "application/x-www-form-urlencoded",
}
data = "name=Ada%20Lovelace&note=a%26b%3Dc%2F%C3%A9".encode("utf-8")

response = requests.request("PUT", url, headers=headers, data=data)
print(response.status_code)
print(response.text)
