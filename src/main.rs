mod constants;
mod container;
mod gantry_crane;
mod mqtt;
mod settings;
mod util;

use anyhow::Result;
use gantry_crane::GantryCrane;

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();

    let app = GantryCrane::new()?;
    app.run().await
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
