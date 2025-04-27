use anyhow::{bail, Result};
use meta_db_redb::ReDbRepo;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;
use std::path::Path;
use std::sync::Arc;

/// Container for database-related components
pub struct DbContext {
    pub repo: Arc<ReDbRepo>,
    pub p_obj: Arc<PersistentObject<ReDbRepo>>,
    pub p_creds: PersistentCredentials<ReDbRepo>,
}

pub struct BaseCommand {
    pub db_name: String,
}

impl BaseCommand {
    pub fn new(db_name: String) -> Self {
        Self { db_name }
    }
    
    /// Opens an existing database and returns a context with repo, persistent object and credentials
    pub async fn open_existing_db(&self) -> Result<DbContext> {
        let db_path = Path::new(self.db_name.as_str());
        
        if !db_path.exists() {
            bail!("Database not found. Please run 'meta-secret init-device' command first.");
        }
        
        let repo = Arc::new(ReDbRepo::open(db_path)?);
        let p_obj = Arc::new(PersistentObject::new(repo.clone()));
        let p_creds = PersistentCredentials {
            p_obj: p_obj.clone(),
        };
        
        Ok(DbContext {
            repo,
            p_obj,
            p_creds,
        })
    }
    
    /// Opens an existing database or creates a new one if it doesn't exist
    pub async fn open_or_create_db(&self) -> Result<DbContext> {
        let db_path = Path::new(self.db_name.as_str());
        
        // Check if database exists and either open or create it
        let repo = if db_path.exists() {
            Arc::new(ReDbRepo::open(db_path)?)
        } else {
            Arc::new(ReDbRepo::new(db_path)?)
        };
        
        let p_obj = Arc::new(PersistentObject::new(repo.clone()));
        let p_creds = PersistentCredentials {
            p_obj: p_obj.clone(),
        };
        
        Ok(DbContext {
            repo,
            p_obj,
            p_creds,
        })
    }
    
    /// Common error message for credentials that already exist
    pub fn already_exists_error(entity: &str) -> String {
        let err_msg = "credentials already exist. Cannot initialize again.";
        let info_msg = "Use the 'info' command to view existing credentials.";
        format!("{} {} {}", entity, err_msg, info_msg)
    }
} 