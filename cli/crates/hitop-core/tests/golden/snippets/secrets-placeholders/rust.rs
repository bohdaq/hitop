// Cargo.toml: reqwest = { version = "0.12", features = ["blocking"] }
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .patch("https://api.example.test/tenants/{{tenant}}/note")
        .header("X-Trace", "t-1")
        .header("Authorization", "Bearer {{token}}")
        .header("Content-Type", "text/plain")
        .body("plain text\twith a tab")
        .send()?;
    println!("{}", response.status());
    println!("{}", response.text()?);
    Ok(())
}
