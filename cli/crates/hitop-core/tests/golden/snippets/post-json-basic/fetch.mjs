const response = await fetch("https://api.example.test/items", {
  method: "POST",
  headers: {
    "Content-Type": "application/json",
    "Authorization": "Basic " + btoa("ada:p'w\"d"),
  },
  body: "{\n  \"name\": \"it's \\\"quoted\\\"\",\n  \"path\": \"C:\\\\tmp\",\n  \"shell\": \"$HOME `id`\",\n  \"city\": \"Kraków\"\n}",
});

console.log(response.status);
console.log(await response.text());
