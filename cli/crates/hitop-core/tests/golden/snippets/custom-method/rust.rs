// Cargo.toml: reqwest = { version = "0.12", features = ["blocking"] }
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .request(reqwest::Method::from_bytes("PURGE".as_bytes())?, "https://cdn.example.test/assets/app.js")
        .header("X-Tag", "b")
        .basic_auth("svc", Some(""))
        .send()?;
    println!("{}", response.status());
    println!("{}", response.text()?);
    Ok(())
}
