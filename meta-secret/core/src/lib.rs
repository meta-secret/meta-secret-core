extern crate core;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use std::ffi::OsStr;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::{fs, io};

use image::ImageError;
use rqrr::DeQRError;

use errors::RecoveryError::EmptyInput;
use errors::{RecoveryError, SharesLoaderError, SplitError};

use crate::errors::CoreError;
use crate::secret::data_block::common::SharedSecretConfig;
use crate::secret::data_block::shared_secret_data_block::SharedSecretBlock;
use crate::secret::shared_secret::{PlainText, SharedSecret, SharedSecretEncryption, UserShareDto};

pub mod crypto;
pub mod errors;
pub mod meta_tests;
pub mod node;
pub mod secret;
pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, thiserror::Error)]
pub enum RecoveryOperationError {
    #[error(transparent)]
    LoaderError(#[from] SharesLoaderError),
    #[error(transparent)]
    RecoveryFromSharesError(#[from] RecoveryError),
}

pub fn recover() -> CoreResult<PlainText> {
    let users_shares = load_users_shares()?;
    let recovered = recover_from_shares(users_shares)?;
    Ok(recovered)
}

pub fn recover_from_shares(users_shares: Vec<UserShareDto>) -> CoreResult<PlainText> {
    let mut secret_blocks: Vec<SharedSecretBlock> = vec![];

    if users_shares[0].share_blocks.is_empty() {
        let err = EmptyInput("Empty shares list. Nothing to recover".to_string());
        return Err(CoreError::from(err));
    }

    let blocks_num: usize = users_shares[0].share_blocks.len();

    for block_index in 0..blocks_num {
        let mut encrypted_data_blocks = vec![];

        for user_share in users_shares.iter() {
            let encrypted_data_block = user_share.get_encrypted_data_block(block_index)?;
            encrypted_data_blocks.push(encrypted_data_block);
        }

        let curr_block = &users_shares[0].share_blocks[block_index];
        let secret_block = SharedSecretBlock {
            config: curr_block.config,
            meta_data: curr_block.meta_data.clone(),
            shares: encrypted_data_blocks,
        };

        secret_blocks.push(secret_block);
    }

    let secret = SharedSecret { secret_blocks };

    let result = secret.recover()?;
    Ok(result)
}

fn load_users_shares() -> Result<Vec<UserShareDto>, SharesLoaderError> {
    //read json files
    let shares = fs::read_dir("secrets")?;

    let mut users_shares_dto: Vec<UserShareDto> = vec![];
    for secret_share_file in shares {
        let file_path = secret_share_file?.path();

        let maybe_ext = file_path.extension().and_then(OsStr::to_str);

        if let Some(ext) = maybe_ext {
            if ext.eq("json") {
                // Open the file in read-only mode with buffer.
                let file = File::open(file_path)?;
                let reader = BufReader::new(file);

                // Read the JSON contents of the file as an instance of `User`.
                let secret_share: UserShareDto = serde_json::from_reader(reader)?;
                users_shares_dto.push(secret_share);
            }
        }
    }

    Ok(users_shares_dto)
}

pub fn split(secret: String, config: SharedSecretConfig) -> CoreResult<()> {
    let plain_text = PlainText::from(secret);
    let shared_secret = SharedSecretEncryption::new(config, plain_text)?;

    let dir_op = fs::create_dir_all("secrets");

    if let Err(dir_err) = dir_op {
        return Err(CoreError::from(SplitError::from(dir_err)));
    }

    for share_index in 0..config.number_of_shares {
        let share: UserShareDto = shared_secret.get_share(share_index);
        let share_json = serde_json::to_string_pretty(&share)?;

        // Save the JSON structure into the output file
        let write_op = fs::write(
            format!("secrets/shared-secret-{share_index}.json"),
            share_json.clone(),
        );

        if let Err(op_err) = write_op {
            return Err(CoreError::from(SplitError::from(op_err)));
        }

        //generate qr code
        generate_qr_code(
            share_json.as_str(),
            format!("secrets/shared-secret-{share_index}.png").as_str(),
        )
    }

    Ok(())
}

pub fn generate_qr_code(data: &str, path: &str) {
    use qrcode_generator::QrCodeEcc;

    qrcode_generator::to_png_to_file(data, QrCodeEcc::High, data.len(), path).unwrap();
}

#[derive(Debug, thiserror::Error)]
pub enum QrToJsonParserError {
    #[error(
        "Secrets directory has invalid structure. \
        Please check that 'secrets' dir exists and \
        contains json or qr files with password shares"
    )]
    SecretsDirectoryError {
        #[from]
        source: io::Error,
    },
    #[error("Image parsing error")]
    ImageParsingError {
        #[from]
        source: QrCodeParserError,
    },
}

pub fn convert_qr_images_to_json_files() -> Result<Vec<String>, QrToJsonParserError> {
    let shares = fs::read_dir("secrets")?;

    let mut shares_json: Vec<String> = vec![];

    let mut share_index = 0;
    for secret_share_file in shares {
        let file_path = secret_share_file?.path();

        let extension = file_path.extension().and_then(OsStr::to_str).unwrap();

        if !extension.eq("png") {
            continue;
        }

        let json_str = read_qr_code(file_path.as_path())?;
        fs::write(
            format!("secrets/shared-secret-{share_index}.json"),
            json_str.clone(),
        )?;

        shares_json.push(json_str.clone());

        share_index += 1;
    }

    Ok(shares_json)
}

#[derive(Debug, thiserror::Error)]
pub enum QrCodeParserError {
    #[error("Qr code parsing error")]
    ImageParsingError {
        #[from]
        source: ImageError,
    },
    #[error("Error decoding image")]
    ImageDecodingError {
        #[from]
        source: DeQRError,
    },
}

pub fn read_qr_code(path: &Path) -> Result<String, QrCodeParserError> {
    let img = image::open(path)?.to_luma8();
    // Prepare for detection
    let mut img = rqrr::PreparedImage::prepare(img);
    // Search for grids, without decoding
    let grids = img.detect_grids();
    assert_eq!(grids.len(), 1);
    // Decode the grid
    let (_, content) = grids[0].decode()?;
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_split_creates_json_and_qr_files() {
        // Create a temporary directory for the test
        let temp_dir = tempdir().expect("Failed to create temp dir");

        // Save the original directory
        let original_dir = env::current_dir().expect("Failed to get current directory");

        // Change to the temporary directory for the test
        env::set_current_dir(temp_dir.path()).expect("Failed to change directory");

        // Create the secrets directory in the current working directory (temp_dir)
        fs::create_dir("secrets").expect("Failed to create secrets directory");

        // Define a test secret and configuration
        let secret = "test secret".to_string();
        let config = SharedSecretConfig {
            number_of_shares: 3,
            threshold: 2,
        };

        // Call the split function (this will create files in the current directory)
        let result = split(secret, config);

        // Assert the operation was successful
        assert!(result.is_ok(), "Split operation failed: {:?}", result.err());

        // Verify files were created
        let mut json_count = 0;
        let mut png_count = 0;

        // Read the secrets directory and count files by type
        if let Ok(entries) = fs::read_dir("secrets") {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext == "json" {
                        json_count += 1;

                        // Verify JSON file can be parsed
                        if let Ok(content) = fs::read_to_string(&path) {
                            let parse_result: Result<UserShareDto, _> =
                                serde_json::from_str(&content);
                            assert!(parse_result.is_ok(), "Failed to parse JSON file");
                        }
                    } else if ext == "png" {
                        png_count += 1;
                    }
                }
            }
        }

        // Verify the correct number of files were created
        assert_eq!(json_count, 3, "Expected 3 JSON files");
        assert_eq!(png_count, 3, "Expected 3 PNG files");

        // Restore the original directory
        env::set_current_dir(original_dir).expect("Failed to restore original directory");
    }

    #[test]
    fn test_generate_qr_code_and_read_qr_code() {
        // Create a temporary directory using tempfile
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let temp_path = temp_dir.path();

        // Create a test file path
        let test_file_path = temp_path.join("test_qr.png");
        let test_file_path_str = test_file_path.to_str().unwrap();

        // Test data
        let test_data = "Test QR Code Data. Test QR Code Data. Test QR Code Data.\
            Test QR Code Data. Test QR Code Data. Test QR Code Data.Test QR Code Data. \
            Test QR Code Data. Test QR Code Data.Test QR Code Data.";

        // Generate QR code
        generate_qr_code(test_data, test_file_path_str);

        // Verify the file exists
        assert!(test_file_path.exists(), "QR code file was not created");

        // Read QR code
        let read_result = read_qr_code(&test_file_path);
        assert!(read_result.is_ok(), "Failed to read QR code");

        // Verify the data matches
        let read_data = read_result.unwrap();
        assert_eq!(
            read_data, test_data,
            "QR code data doesn't match original data"
        );
    }

    #[test]
    fn test_recover_from_shares() {
        // Create a test UserShareDto with empty share_blocks
        let share1 = UserShareDto {
            share_id: 0,
            share_blocks: vec![],
        };

        // For this test, we'll focus on the error case when shares list is empty
        let empty_shares = vec![share1];
        let result = recover_from_shares(empty_shares);

        // Verify it returns the expected error
        assert!(result.is_err());
        match result {
            Err(CoreError::RecoveryError { source }) => match source {
                RecoveryError::EmptyInput(_) => {}
                _ => panic!("Expected EmptyInput error variant, got: {:?}", source),
            },
            _ => panic!("Expected RecoveryError error, got: {:?}", result),
        }
    }
}
