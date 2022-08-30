use std::{cell::RefCell, collections::HashMap};

use bollard::{
    service::{EventActor, EventMessageTypeEnum},
    system::EventsOptions,
    Docker,
};
use tokio::signal::unix::{signal, SignalKind};
use tokio_stream::StreamExt;

use crate::container::Container;

const NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");

const ACTION_CREATE: &str = "create";
const ACTION_DESTROY: &str = "destroy";
const ACTION_RENAME: &str = "rename";

pub struct GantryCrane {
    docker: Docker,
    containers: RefCell<HashMap<String, Container>>,
}

impl GantryCrane {
    pub fn new() -> Self {
        match Docker::connect_with_local_defaults() {
            Ok(docker) => GantryCrane {
                docker,
                containers: RefCell::new(HashMap::new()),
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
            _ = self.events_loop() => log::debug!("listen_for_events() ended"),
            // _ = self.poll_loop() => log::debug!("poll_containers() ended"), // TODO: uncomment
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

    /// Listens to docker container events endlessly and creates, removes or renames containers accordingly.
    async fn events_loop(&self) {
        let mut stream = self.docker.events(Some(EventsOptions::<String> {
            until: None,
            ..Default::default()
        }));
        while let Some(result) = stream.next().await {
            match result {
                Err(e) => log::error!("Failed to read from docker: {}", e),
                Ok(event) => {
                    // We only care about container related events
                    if event.typ == Some(EventMessageTypeEnum::CONTAINER) {
                        let get_container_attr =
                            |actor: &Option<EventActor>, attr: &str| -> Option<String> {
                                actor
                                    .to_owned()?
                                    .attributes?
                                    .get(attr)
                                    .map(|name| name.to_owned())
                            };

                        if let Some(container_name) = get_container_attr(&event.actor, "name") {
                            // We only care about new, removed or renamed containers
                            match event.action.as_deref() {
                                Some(ACTION_CREATE) => {
                                    log::info!("Adding new container '{}'", container_name);
                                    self.add_container(&container_name).await;
                                }
                                Some(ACTION_DESTROY) => {
                                    if let Some(_container) =
                                        self.containers.borrow_mut().remove(&container_name)
                                    {
                                        log::info!("Removed container '{}'", container_name);
                                    } else {
                                        log::debug!(
                                            "Received destroy event for unknown container '{}'",
                                            container_name
                                        );
                                    }
                                }
                                Some(ACTION_RENAME) => {
                                    if let Some(mut old_name) =
                                        get_container_attr(&event.actor, "oldName")
                                    {
                                        // Remove leading slash
                                        old_name.remove(0);

                                        let mut containers = self.containers.borrow_mut();
                                        if let Some(mut container) = containers.remove(&old_name) {
                                            log::info!(
                                                "Renaming container '{}' to '{}'",
                                                old_name,
                                                container_name
                                            );
                                            container.name = container_name.clone();
                                            if containers
                                                .insert(container_name.clone(), container)
                                                .is_some()
                                            {
                                                log::debug!("Container '{}' was already available and has been replaced", container_name);
                                            }
                                        } else {
                                            log::debug!(
                                                "Received rename event for unknown container '{}'",
                                                old_name
                                            );
                                        }
                                    }
                                }
                                _ => continue,
                            }
                        }
                    }
                }
            }
        }
    }

    /// Regularly polls all known containers and updates them accordingly.
    async fn poll_loop(&self) {
        todo!("Regularly poll container stats");
    }

    async fn add_container(&self, container_name: &str) {
        if let Some(stats) = self.get_container_stats(container_name).await {
            if self
                .containers
                .borrow_mut()
                .insert(container_name.into(), Container::from_stats(&stats))
                .is_some()
            {
                log::debug!(
                    "Container '{}' was already available and has been replaced",
                    container_name
                );
            }
        }
    }
}
