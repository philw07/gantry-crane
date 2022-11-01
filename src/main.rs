mod args;
mod constants;
mod container;
mod events;
mod gantry_crane;
mod homeassistant;
mod mqtt;
mod settings;
mod signal_handler;
mod util;

use anyhow::Result;
use args::GantryCraneArgs;
use constants::{APP_NAME, APP_VERSION};
use gantry_crane::GantryCrane;

#[cfg(debug_assertions)]
const LOG_LEVEL_DEFAULT: &str = "gantry_crane=debug";
#[cfg(not(debug_assertions))]
const LOG_LEVEL_DEFAULT: &str = "gantry_crane=info";

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
    let app = GantryCrane::new(&args)?;
    if args.clean {
        app.run_clean().await
    } else {
        app.run().await
    }
}

fn init_logging() {
    let env = env_logger::Env::default().filter_or("LOG_LEVEL", LOG_LEVEL_DEFAULT);
    env_logger::init_from_env(env);
}
