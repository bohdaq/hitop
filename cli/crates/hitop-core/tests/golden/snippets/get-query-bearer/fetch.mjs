const response = await fetch("https://api.example.test/users?page=2&q=a%20b", {
  method: "GET",
  headers: {
    "Accept": "application/json",
    "Authorization": "Bearer tok-123",
  },
});

console.log(response.status);
console.log(await response.text());
