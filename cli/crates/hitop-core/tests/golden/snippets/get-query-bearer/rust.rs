// Cargo.toml: reqwest = { version = "0.12", features = ["blocking"] }
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .get("https://api.example.test/users?page=2&q=a%20b")
        .header("Accept", "application/json")
        .header("Authorization", "Bearer tok-123")
        .send()?;
    println!("{}", response.status());
    println!("{}", response.text()?);
    Ok(())
}
