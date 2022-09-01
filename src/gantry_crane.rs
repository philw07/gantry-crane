use std::{cell::RefCell, collections::HashMap, time::Duration};

use bollard::{
    container::{ListContainersOptions, Stats, StatsOptions},
    service::{EventActor, EventMessageTypeEnum},
    system::EventsOptions,
    Docker,
};
use futures::{stream::FuturesUnordered, StreamExt};
use tokio::{
    signal::unix::{signal, SignalKind},
    time,
};

use crate::{container::Container, settings::Settings};

const NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");

const ACTION_CREATE: &str = "create";
const ACTION_DESTROY: &str = "destroy";
const ACTION_RENAME: &str = "rename";

pub struct GantryCrane {
    docker: Docker,
    settings: Settings,
    containers: RefCell<HashMap<String, Container>>,
}

impl GantryCrane {
    pub fn new() -> Self {
        match Docker::connect_with_local_defaults() {
            Ok(docker) => GantryCrane {
                docker,
                settings: Settings::new(),
                containers: RefCell::new(HashMap::new()),
            },
            Err(e) => panic!("Failed to connect to docker: {}", e),
        }
    }

    pub async fn run(&self) {
        log::info!("Starting {} {}", NAME, VERSION);

        // Get available containers
        self.get_available_containers().await;

        // Run tasks endlessly
        tokio::select! {
            res = self.handle_signals() => {
                if let Err(e) = res {
                    log::error!("An error occurred trying to setup signal handlers: {}", e);
                }
            },
            _ = self.events_loop() => log::debug!("listen_for_events() ended"),
            _ = self.poll_loop() => log::debug!("poll_containers() ended"),
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

    /// Adds all containers from docker
    async fn get_available_containers(&self) {
        let options = ListContainersOptions::<String> {
            all: true,
            limit: None,
            size: false,
            ..Default::default()
        };
        match self.docker.list_containers(Some(options)).await {
            Ok(containers) => {
                let all_stats = containers
                    .into_iter()
                    .flat_map(|c| c.names?.first().cloned())
                    .collect::<Vec<_>>()
                    .iter()
                    .map(|name| self.get_container_stats(name))
                    .collect::<FuturesUnordered<_>>()
                    .collect::<Vec<_>>()
                    .await;
                for stats in all_stats.into_iter().flatten() {
                    self.add_container(stats).await;
                }
            }
            Err(e) => panic!("Failed to get docker containers: {}", e),
        }
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

                        if let Some(mut container_name) = get_container_attr(&event.actor, "name") {
                            // Events don't use the qualified container name, add the leading slash
                            container_name.insert(0, '/');

                            // We only care about new, removed or renamed containers
                            match event.action.as_deref() {
                                Some(ACTION_CREATE) => {
                                    if let Some(stats) =
                                        self.get_container_stats(&container_name).await
                                    {
                                        self.add_container(stats).await;
                                    }
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
                                    if let Some(old_name) =
                                        get_container_attr(&event.actor, "oldName")
                                    {
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
        loop {
            log::debug!("Polling stats for all containers");
            let names: Vec<String> = self.containers.borrow().keys().cloned().collect();

            // Collect stats for all containers
            let all_stats = names
                .iter()
                .map(|name| self.get_container_stats(name))
                .collect::<FuturesUnordered<_>>()
                .collect::<Vec<_>>()
                .await;

            // Update all containers with the collected stats
            for stats in all_stats.into_iter().flatten() {
                // Update container if available
                if let Some(container) = self.containers.borrow_mut().get_mut(&stats.name) {
                    container.update_from_stats(&stats);
                } else {
                    log::error!("Got stats for '{}', but no container!", &stats.name);
                }
            }

            // Wait for the next iteration
            time::sleep(Duration::from_secs(self.settings.poll_interval as u64)).await;
        }
    }

    async fn add_container(&self, stats: Stats) {
        log::info!("Adding new container '{}'", stats.name);
        if self
            .containers
            .borrow_mut()
            .insert(stats.name.clone(), Container::from_stats(&stats))
            .is_some()
        {
            log::debug!(
                "Container '{}' was already available and has been replaced",
                stats.name
            );
        }
    }

    async fn get_container_stats(&self, container_name: &str) -> Option<Stats> {
        // Remove leading slash
        let container_name = container_name.strip_prefix('/').unwrap_or(container_name);

        // Try to get stats
        let options = StatsOptions {
            stream: false,
            one_shot: true,
        };
        let mut stream = self.docker.stats(container_name, Some(options));
        match stream.next().await {
            Some(Ok(stats)) => Some(stats),
            Some(Err(e)) => {
                log::error!(
                    "Error trying to retrieve stats for container '{}': {}",
                    container_name,
                    e
                );
                None
            }
            None => {
                log::warn!(
                    "Got no result from stream in get_container_stats() for '{}'",
                    container_name
                );
                None
            }
        }
    }
}
