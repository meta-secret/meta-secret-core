use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

fn unique_run_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System clock is before UNIX epoch")
        .as_nanos();
    format!("{}-{}", std::process::id(), timestamp)
}

#[test]
fn test_one_device_lifecycle() {
    let run_id = unique_run_id();
    let vault_name = format!("cli_{}@test.com", run_id);
    let secret_name = format!("Secret{}", run_id);
    let secret_value = run_id.clone();

    println!(
        "\n📝 CLI test run: vault='{}', secret='{}', value='{}'\n",
        vault_name, secret_name, secret_value
    );

    // Create an isolated temp directory for this test run
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_path = temp_dir.path();

    let bin_path = env!("CARGO_BIN_EXE_meta-cli");
    println!("✅ Using CLI binary: {}", bin_path);

    // Step 1: init device
    println!("⏳ Step 1: init device");
    let output = Command::new(bin_path)
        .arg("init")
        .arg("device")
        .arg("--device-name")
        .arg("one-device-test")
        .current_dir(&temp_path)
        .output()
        .expect("Failed to run init device");

    if !output.status.success() {
        eprintln!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
        panic!("init device failed");
    }
    println!("✅ Device initialized");

    // Step 2: init user with vault name
    println!("⏳ Step 2: init user --vault-name '{}'", vault_name);
    let output = Command::new(bin_path)
        .arg("init")
        .arg("user")
        .arg("--vault-name")
        .arg(&vault_name)
        .current_dir(&temp_path)
        .output()
        .expect("Failed to run init user");

    if !output.status.success() {
        eprintln!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
        panic!("init user failed");
    }
    println!("✅ User initialized with vault name '{}'", vault_name);

    // Step 3: auth sign-up
    println!("⏳ Step 3: auth sign-up");
    let output = Command::new(bin_path)
        .arg("auth")
        .arg("sign-up")
        .current_dir(&temp_path)
        .output()
        .expect("Failed to run auth sign-up");

    if !output.status.success() {
        eprintln!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
        panic!("auth sign-up failed");
    }
    println!("✅ Signed up (vault created on server)");

    // Step 4: secret split (create secret)
    println!(
        "⏳ Step 4: secret split --pass-name '{}' --stdin",
        secret_name
    );
    let mut child = Command::new(bin_path)
        .arg("secret")
        .arg("split")
        .arg("--pass-name")
        .arg(&secret_name)
        .arg("--stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(&temp_path)
        .spawn()
        .expect("Failed to spawn secret split");

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(secret_value.as_bytes());
    }

    let output = child
        .wait_with_output()
        .expect("Failed to wait for secret split");
    if !output.status.success() {
        eprintln!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
        panic!("secret split failed");
    }
    println!("✅ Secret created: '{}' = '{}'", secret_name, secret_value);

    // Step 5: info secrets (verify secret exists)
    println!("⏳ Step 5: info secrets");
    let output = Command::new(bin_path)
        .arg("info")
        .arg("secrets")
        .current_dir(&temp_path)
        .output()
        .expect("Failed to run info secrets");

    let secrets_output = String::from_utf8_lossy(&output.stdout);
    assert!(
        secrets_output.contains(&secret_name),
        "Secret '{}' not found in secrets list",
        secret_name
    );
    println!("✅ Secret '{}' found in vault", secret_name);

    // Step 6: secret recovery-request
    println!("⏳ Step 6: secret recovery-request");
    let output = Command::new(bin_path)
        .arg("secret")
        .arg("recovery-request")
        .arg("--pass-name")
        .arg(&secret_name)
        .current_dir(&temp_path)
        .output()
        .expect("Failed to run recovery-request");

    if !output.status.success() {
        eprintln!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
        panic!("recovery-request failed");
    }
    println!("✅ Recovery request sent");

    // Step 7: secret accept-all-recovery-requests (self-approve)
    println!("⏳ Step 7: secret accept-all-recovery-requests");
    let output = Command::new(bin_path)
        .arg("secret")
        .arg("accept-all-recovery-requests")
        .current_dir(&temp_path)
        .output()
        .expect("Failed to run accept-all-recovery-requests");

    if !output.status.success() {
        eprintln!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
        panic!("accept-all-recovery-requests failed");
    }
    println!("✅ Recovery request accepted (self-approved)");

    // Step 8: info recovery-claims (JSON format to get claim ID)
    println!("⏳ Step 8: --output-format json info recovery-claims");
    let output = Command::new(bin_path)
        .arg("--output-format")
        .arg("json")
        .arg("info")
        .arg("recovery-claims")
        .current_dir(&temp_path)
        .output()
        .expect("Failed to run recovery-claims");

    if !output.status.success() {
        eprintln!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
        panic!("recovery-claims failed");
    }

    let claims_json = String::from_utf8_lossy(&output.stdout);
    let claims: serde_json::Value =
        serde_json::from_str(&claims_json).expect("Failed to parse recovery-claims JSON");

    let claims_array = claims["claims"].as_array().expect("No claims array found");
    assert!(!claims_array.is_empty(), "No recovery claims found");

    // Find the claim for our secret
    let claim_id = claims_array
        .iter()
        .find(|claim| {
            claim["password"]
                .as_str()
                .map(|p| p == secret_name)
                .unwrap_or(false)
        })
        .and_then(|claim| claim["id"].as_str())
        .expect(&format!("Claim for secret '{}' not found", secret_name));

    println!("✅ Found claim ID: {}", claim_id);

    // Step 9: secret show --claim-id (JSON format to get recovered value)
    println!(
        "⏳ Step 9: --output-format json secret show --claim-id '{}'",
        claim_id
    );
    let output = Command::new(bin_path)
        .arg("--output-format")
        .arg("json")
        .arg("secret")
        .arg("show")
        .arg("--claim-id")
        .arg(claim_id)
        .current_dir(&temp_path)
        .output()
        .expect("Failed to run secret show");

    if !output.status.success() {
        eprintln!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
        panic!("secret show failed");
    }

    let show_json = String::from_utf8_lossy(&output.stdout);
    let show_result: serde_json::Value =
        serde_json::from_str(&show_json).expect("Failed to parse secret show JSON");

    let recovered_value = show_result["secret"]
        .as_str()
        .expect(&format!("No 'secret' field in response: {}", show_json));

    assert_eq!(
        recovered_value, secret_value,
        "Recovered value does not match original"
    );
    println!(
        "✅ Secret recovered correctly: '{}' = '{}'",
        secret_name, recovered_value
    );

    println!("\n🎉 Full lifecycle test passed\n");
}
