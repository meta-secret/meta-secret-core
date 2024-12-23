use tracing::Level;

pub mod fixture_util;
pub mod spec;

pub fn setup_tracing() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .without_time()
        .compact()
        .pretty()
        .init();
    Ok(())
}
