mod args;
mod constants;
mod container;
mod events;
mod gantry_crane;
mod homeassistant;
mod mqtt;
mod settings;
mod util;

use anyhow::Result;
use args::GantryCraneArgs;
use constants::{APP_NAME, APP_VERSION};
use gantry_crane::GantryCrane;

#[tokio::main]
async fn main() -> Result<()> {
    let args = GantryCraneArgs::from_env();

    // Print version and exit if requested
    if args.version {
        println!("{} {}", APP_NAME, APP_VERSION);
        std::process::exit(0);
    }

    init_logging();

    // Run app
    let app = GantryCrane::new()?;
    if args.clean {
        app.run_clean().await
    } else {
        app.run().await
    }
}

fn init_logging() {
    let default_level = if cfg!(debug_assertions) {
        "gantry_crane=debug"
    } else {
        "gantry_crane=info"
    };
    let env = env_logger::Env::default().filter_or("LOG_LEVEL", default_level);
    env_logger::init_from_env(env);
}
