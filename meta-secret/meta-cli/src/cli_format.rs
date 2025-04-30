use std::str::FromStr;

/// Output format for CLI command results
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliOutputFormat {
    /// JSON output
    Json,
    /// YAML output
    Yaml,
}

impl Default for CliOutputFormat {
    fn default() -> Self {
        CliOutputFormat::Yaml
    }
}

impl FromStr for CliOutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(CliOutputFormat::Json),
            "yaml" => Ok(CliOutputFormat::Yaml),
            _ => Err(format!("Unknown format: {}", s)),
        }
    }
} 