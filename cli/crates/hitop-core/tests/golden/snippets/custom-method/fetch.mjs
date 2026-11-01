const response = await fetch("https://cdn.example.test/assets/app.js", {
  method: "PURGE",
  headers: {
    "X-Tag": "b",
    "Authorization": "Basic " + btoa("svc:"),
  },
});

console.log(response.status);
console.log(await response.text());
