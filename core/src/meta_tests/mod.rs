use tracing::Level;

pub mod spec;
pub mod fixture_util;

pub fn setup_tracing() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .without_time()
        .compact()
        .pretty()
        .init();
    Ok(())
}
