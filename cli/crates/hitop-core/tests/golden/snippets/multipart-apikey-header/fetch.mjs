import { openAsBlob } from "node:fs";

const form = new FormData();
form.append("title", "@not-a-file");
form.append("avatar", await openAsBlob("photo.png"), "photo.png");

const response = await fetch("https://api.example.test/upload", {
  method: "POST",
  headers: {
    "X-API-Key": "k-123",
  },
  body: form,
});

console.log(response.status);
console.log(await response.text());
