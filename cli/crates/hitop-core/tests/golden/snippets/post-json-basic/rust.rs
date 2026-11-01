// Cargo.toml: reqwest = { version = "0.12", features = ["blocking"] }
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .post("https://api.example.test/items")
        .header("Content-Type", "application/json")
        .basic_auth("ada", Some("p'w\"d"))
        .body(r#"{
  "name": "it's \"quoted\"",
  "path": "C:\\tmp",
  "shell": "$HOME `id`",
  "city": "Kraków"
}"#)
        .send()?;
    println!("{}", response.status());
    println!("{}", response.text()?);
    Ok(())
}
