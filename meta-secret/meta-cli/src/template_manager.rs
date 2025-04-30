use crate::cli_format::CliOutputFormat;
use anyhow::Result;
use std::sync::OnceLock;
use tera::{Context, Tera};

/// Template manager for the CLI
///
/// This manager handles loading and rendering templates.
/// It uses a lazy-loaded singleton pattern to avoid loading templates multiple times.
pub struct TemplateManager {
    tera: Tera,
}

static TEMPLATE_MANAGER: OnceLock<TemplateManager> = OnceLock::new();

impl TemplateManager {
    /// Get the template manager instance (creates it if it doesn't exist)
    pub fn instance() -> &'static TemplateManager {
        TEMPLATE_MANAGER
            .get_or_init(|| TemplateManager::new().expect("Failed to initialize template manager"))
    }

    /// Create a new template manager by loading all templates
    fn new() -> Result<Self> {
        let mut tera = Tera::default();

        // Add JSON templates
        tera.add_raw_template("info.json", include_str!("templates/info.json.tera"))?;
        tera.add_raw_template(
            "recovery_claims.json",
            include_str!("templates/recovery_claims.json.tera"),
        )?;
        tera.add_raw_template("secrets.json", include_str!("templates/secrets.json.tera"))?;
        tera.add_raw_template(
            "vault_events.json",
            include_str!("templates/vault_events.json.tera"),
        )?;
        tera.add_raw_template("error.json", include_str!("templates/error.json.tera"))?;

        // Add YAML templates
        tera.add_raw_template("info.yaml", include_str!("templates/info.yaml.tera"))?;
        tera.add_raw_template(
            "recovery_claims.yaml",
            include_str!("templates/recovery_claims.yaml.tera"),
        )?;
        tera.add_raw_template("secrets.yaml", include_str!("templates/secrets.yaml.tera"))?;
        tera.add_raw_template(
            "vault_events.yaml",
            include_str!("templates/vault_events.yaml.tera"),
        )?;
        tera.add_raw_template("error.yaml", include_str!("templates/error.yaml.tera"))?;

        Ok(Self { tera })
    }

    /// Get the full template name based on the base name and output format
    pub fn get_template_name(&self, base_name: &str, format: CliOutputFormat) -> String {
        match format {
            CliOutputFormat::Json => format!("{}.json", base_name),
            CliOutputFormat::Yaml => format!("{}.yaml", base_name),
        }
    }

    /// Render a template with the given context and output format
    pub fn render(
        &self,
        base_template_name: &str,
        context: &Context,
        format: CliOutputFormat,
    ) -> Result<String> {
        let template_name = self.get_template_name(base_template_name, format);
        Ok(self.tera.render(&template_name, context)?)
    }
}
