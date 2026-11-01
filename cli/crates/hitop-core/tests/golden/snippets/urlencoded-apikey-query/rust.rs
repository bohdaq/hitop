// Cargo.toml: reqwest = { version = "0.12", features = ["blocking"] }
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .put("https://api.example.test/profile?api_key={{key}}")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body("name=Ada%20Lovelace&note=a%26b%3Dc%2F%C3%A9")
        .send()?;
    println!("{}", response.status());
    println!("{}", response.text()?);
    Ok(())
}
