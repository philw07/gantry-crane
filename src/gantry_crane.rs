use bollard::Docker;
use tokio::signal::unix::{signal, SignalKind};

const NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct GantryCrane {
    docker: Docker,
}

impl GantryCrane {
    pub fn new() -> Self {
        match Docker::connect_with_local_defaults() {
            Ok(docker) => {
                GantryCrane {
                    docker,
                }
            },
            Err(e) => panic!("Failed to connect to docker: {}", e),
        }
    }

    pub async fn run(&self) {
        log::info!("Starting {} {}", NAME, VERSION);

        tokio::select! {
            res = self.handle_signals() => {
                if let Err(e) = res {
                    log::error!("An error occurred trying to setup signal handlers: {}", e);
                }
            },
            _ = self.listen_for_events() => log::debug!("listen_for_events() ended"),
            _ = self.poll_containers() => log::debug!("poll_containers() ended"),
        }

        log::info!("Shutting down");
    }

    async fn handle_signals(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut sigint = signal(SignalKind::interrupt())?;
        let mut sigterm = signal(SignalKind::terminate())?;

        tokio::select! {
            _ = sigint.recv() => log::debug!("Received SIGINT signal"),
            _ = sigterm.recv() => log::debug!("Received SIGTERM signal"),
        };
        Ok(())
    }

    async fn listen_for_events(&self) {
        // Listen to docker events endlessly
        todo!("Listen for docker events");
    }

    async fn poll_containers(&self) {
        // Regularly poll the stats of all known containers
        todo!("Regularly poll container stats");
    }
}
