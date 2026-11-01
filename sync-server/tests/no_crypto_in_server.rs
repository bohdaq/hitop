//! The server binary links no decryption code: the client crypto crate and the AEAD/HPKE crates
//! are dev-dependencies only (tests play the clients). If this fails, something made the server
//! able to decrypt; that changes the threat model.

#[test]
fn server_build_has_no_client_crypto() {
    let out = std::process::Command::new(env!("CARGO"))
        .args(["tree", "--offline", "-e", "normal,build", "--prefix", "none", "--all-features", "-p", "hitop-sync-server"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo tree");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let tree = String::from_utf8_lossy(&out.stdout);
    assert!(tree.contains("rust-web-server"), "{tree}");
    for name in ["hitop-sync-crypto ", "hpke ", "chacha20poly1305 ", "x25519-dalek "] {
        assert!(!tree.lines().any(|l| l.starts_with(name)), "the server depends on {name}");
    }
}
