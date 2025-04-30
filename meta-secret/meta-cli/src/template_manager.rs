use anyhow::Result;
use tera::{Tera, Context};
use std::sync::OnceLock;

/// Template manager for the CLI
/// 
/// This manager handles loading and rendering templates.
/// It uses a lazy-loaded singleton pattern to avoid loading templates multiple times.
pub struct TemplateManager {
    tera: Tera
}

static TEMPLATE_MANAGER: OnceLock<TemplateManager> = OnceLock::new();

impl TemplateManager {
    /// Get the template manager instance (creates it if it doesn't exist)
    pub fn instance() -> &'static TemplateManager {
        TEMPLATE_MANAGER.get_or_init(|| {
            TemplateManager::new().expect("Failed to initialize template manager")
        })
    }
    
    /// Create a new template manager by loading all templates
    fn new() -> Result<Self> {
        let mut tera = Tera::default();
        
        // Add all templates
        tera.add_raw_template("info", include_str!("templates/info.tera"))?;
        
        // Add more templates as needed
        // tera.add_raw_template("another_template", include_str!("templates/another.tera"))?;
        
        Ok(Self { tera })
    }
    
    /// Render a template with the given context
    pub fn render(&self, template_name: &str, context: &Context) -> Result<String> {
        Ok(self.tera.render(template_name, context)?)
    }
} 