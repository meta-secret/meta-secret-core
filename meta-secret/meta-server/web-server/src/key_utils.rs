use anyhow::Result;
use meta_secret_core::crypto::key_pair::{KeyPair, TransportDsaKeyPair};
use meta_secret_core::crypto::keys::TransportSk;
use std::fs;
use std::path::Path;
use tracing::info;
use serde_json;

pub fn load_or_create_master_key(key_file_path: &str) -> Result<TransportSk> {
    if Path::new(key_file_path).exists() {
        // Read the key from file
        info!("Reading master key from file: {}", key_file_path);
        let file = fs::File::open(key_file_path)?;
        
        // Deserialize the key using serde_json
        let master_key: TransportSk = serde_json::from_reader(file)?;
        
        Ok(master_key)
    } else {
        // Generate a new key
        info!("Generating new master key and saving to: {}", key_file_path);
        let key_pair = TransportDsaKeyPair::generate();
        let master_key = key_pair.sk();
        
        // Serialize and save the key using serde_json
        let file = fs::File::create(key_file_path)?;
        serde_json::to_writer_pretty(file, &master_key)?;
        
        Ok(master_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[tokio::test]
    async fn test_create_new_master_key() -> Result<()> {
        // Use a test-specific file path
        let test_file_path = "test_master_key.json";
        
        // Ensure file doesn't exist before test
        if Path::new(test_file_path).exists() {
            fs::remove_file(test_file_path)?;
        }
        
        // Call function to create a new key
        let master_key1 = load_or_create_master_key(test_file_path)?;
        
        // Verify the file was created
        assert!(Path::new(test_file_path).exists());
        
        // Call function again to load the existing key
        let master_key2 = load_or_create_master_key(test_file_path)?;
        
        // Verify the loaded key matches the original key
        assert_eq!(
            serde_json::to_string(&master_key1)?,
            serde_json::to_string(&master_key2)?
        );
        
        // Clean up
        fs::remove_file(test_file_path)?;
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_load_existing_master_key() -> Result<()> {
        // Use a test-specific file path
        let test_file_path = "existing_key_test.json";
        
        // Generate a key pair for testing
        let key_pair = TransportDsaKeyPair::generate();
        let original_key = key_pair.sk();
        
        // Manually create the key file
        let file = fs::File::create(test_file_path)?;
        serde_json::to_writer_pretty(file, &original_key)?;
        
        // Load the key using our function
        let loaded_key = load_or_create_master_key(test_file_path)?;
        
        // Verify the loaded key matches the original
        assert_eq!(
            serde_json::to_string(&original_key)?,
            serde_json::to_string(&loaded_key)?
        );
        
        // Clean up
        fs::remove_file(test_file_path)?;
        
        Ok(())
    }
} 