const response = await fetch("https://api.example.test/profile?api_key={{key}}", {
  method: "PUT",
  headers: {
    "Content-Type": "application/x-www-form-urlencoded",
  },
  body: "name=Ada%20Lovelace&note=a%26b%3Dc%2F%C3%A9",
});

console.log(response.status);
console.log(await response.text());
