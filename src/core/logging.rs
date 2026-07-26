use std::io;
use tracing_subscriber::{Registry, fmt, layer::SubscriberExt};

pub fn setup() -> Result<(), io::Error> {
    let level = if std::env::var("APP_ENV").unwrap_or(String::from("development")) != "production" {
        tracing_subscriber::filter::LevelFilter::TRACE
    } else {
        tracing_subscriber::filter::LevelFilter::INFO
    };

    tracing::subscriber::set_global_default(
        Registry::default().with(fmt::layer().json()).with(level),
    )
    .map_err(|err| {
        io::Error::new(
            io::ErrorKind::Other,
            format!(
                "Failure while registering the customized logger: {}",
                err
            ),
        )
    })?;
    std::panic::set_hook(Box::new(tracing_panic::panic_hook));
    Ok(())
}
