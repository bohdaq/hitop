// Cargo.toml: reqwest = { version = "0.12", features = ["blocking", "multipart"] }
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let form = reqwest::blocking::multipart::Form::new()
        .text("title", "@not-a-file")
        .file("avatar", "photo.png")?;
    let client = reqwest::blocking::Client::new();
    let response = client
        .post("https://api.example.test/upload")
        .header("X-API-Key", "k-123")
        .multipart(form)
        .send()?;
    println!("{}", response.status());
    println!("{}", response.text()?);
    Ok(())
}
