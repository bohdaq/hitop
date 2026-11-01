const response = await fetch("https://api.example.test/tenants/{{tenant}}/note", {
  method: "PATCH",
  headers: {
    "X-Trace": "t-1",
    "Authorization": "Bearer {{token}}",
  },
  body: "plain text\twith a tab",
});

console.log(response.status);
console.log(await response.text());
