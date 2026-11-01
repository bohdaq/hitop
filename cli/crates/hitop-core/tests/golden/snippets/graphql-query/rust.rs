// Cargo.toml: reqwest = { version = "0.12", features = ["blocking"] }
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .post("https://api.example.test/graphql")
        .header("Authorization", "Bearer {{token}}")
        .header("Content-Type", "application/json")
        .body(r#"{"query":"query User($id: ID!) {\n  user(id: $id) { name }\n}\n","variables":{"id": "7"},"operationName":"User"}"#)
        .send()?;
    println!("{}", response.status());
    println!("{}", response.text()?);
    Ok(())
}
