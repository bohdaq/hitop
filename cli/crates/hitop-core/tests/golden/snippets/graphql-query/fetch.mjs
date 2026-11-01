const response = await fetch("https://api.example.test/graphql", {
  method: "POST",
  headers: {
    "Authorization": "Bearer {{token}}",
    "Content-Type": "application/json",
  },
  body: "{\"query\":\"query User($id: ID!) {\\n  user(id: $id) { name }\\n}\\n\",\"variables\":{\"id\": \"7\"},\"operationName\":\"User\"}",
});

console.log(response.status);
console.log(await response.text());
