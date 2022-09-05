use std::{borrow::BorrowMut, collections::HashMap, rc::Rc, time::Duration};

use bollard::{
    container::{ListContainersOptions, Stats, StatsOptions},
    service::{ContainerInspectResponse, EventActor, EventMessageTypeEnum},
    system::EventsOptions,
    Docker,
};
use futures::{
    stream::{FuturesOrdered, FuturesUnordered},
    StreamExt,
};
use tokio::{
    signal::unix::{signal, SignalKind},
    sync::RwLock,
    time,
};

use crate::{
    constants::{
        APP_NAME, APP_VERSION, DOCKER_EVENT_ACTION_CREATE, DOCKER_EVENT_ACTION_DESTROY,
        DOCKER_EVENT_ACTION_RENAME,
    },
    container::Container,
    mqtt::MqttClient,
    settings::Settings,
};

pub struct GantryCrane {
    docker: Docker,
    settings: Settings,
    mqtt: Rc<RwLock<MqttClient>>,
    containers: RwLock<HashMap<String, Container>>,
}

impl GantryCrane {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        match Docker::connect_with_local_defaults() {
            Ok(docker) => {
                let settings = Settings::new();
                let mqtt = MqttClient::new(&settings)?;
                Ok(GantryCrane {
                    docker,
                    settings,
                    mqtt: Rc::new(RwLock::new(mqtt)),
                    containers: RwLock::new(HashMap::new()),
                })
            }
            Err(e) => panic!("Failed to connect to docker: {}", e),
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut result = Ok(());
        log::info!("Starting {} {}", APP_NAME, APP_VERSION);

        // Connect to MQTT
        self.mqtt.read().await.connect().await?;

        // Get container list from MQTT
        let container_list_response = self
            .mqtt
            .write()
            .await
            .subscribe_once("bridge/containers", Duration::from_secs(2))
            .await;

        // Get available containers
        match self.get_available_containers().await {
            Ok(()) => {
                // Remove stale topics from MQTT
                if let Some(container_list) = container_list_response {
                    self.remove_stale_topics(container_list).await;
                }

                // Run tasks endlessly
                tokio::select! {
                    res = self.handle_signals() => {
                        if let Err(e) = res {
                            log::error!("An error occurred trying to setup signal handlers: {}", e);
                            result = Err(e)
                        }
                    },
                    _ = self.events_loop() => log::debug!("listen_for_events() ended"),
                    _ = self.poll_loop() => log::debug!("poll_containers() ended"),
                }

                // Publish container list before shutting down
                self.publish_container_list().await;
            }
            Err(e) => result = Err(e),
        }

        log::info!("Shutting down");

        // Disconnect from MQTT
        self.mqtt.read().await.disconnect().await;
        result
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
    async fn get_available_containers(&self) -> Result<(), Box<dyn std::error::Error>> {
        let options = ListContainersOptions::<String> {
            all: true,
            limit: None,
            size: false,
            ..Default::default()
        };
        match self.docker.list_containers(Some(options)).await {
            Ok(containers) => {
                // Get all image names
                let images = containers
                    .iter()
                    .map(|c| c.image.clone())
                    .collect::<Vec<_>>();

                // Get all stats and inspects
                let all_info = containers
                    .into_iter()
                    .flat_map(|c| c.names?.first().cloned())
                    .collect::<Vec<_>>()
                    .iter()
                    .map(|name| self.get_container_stats_and_inspect(name))
                    .collect::<FuturesOrdered<_>>()
                    .collect::<Vec<_>>()
                    .await;

                // Add containers
                for (res, image) in all_info.into_iter().zip(images) {
                    if let (Some(stats), Some(inspect)) = res {
                        self.create_container(stats, inspect, image).await;
                    }
                }
                Ok(())
            }
            Err(e) => {
                log::error!("Failed to get docker containers: {}", e);
                Err(e.into())
            }
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
                                Some(DOCKER_EVENT_ACTION_CREATE) => {
                                    if let (Some(stats), Some(inspect)) =
                                        self.get_container_stats_and_inspect(&container_name).await
                                    {
                                        let image = get_container_attr(&event.actor, "image");
                                        self.create_container(stats, inspect, image).await;
                                    }
                                }
                                Some(DOCKER_EVENT_ACTION_DESTROY) => {
                                    if let Some(_container) =
                                        self.remove_container(&container_name).await
                                    {
                                        log::info!("Removed container '{}'", container_name);
                                    } else {
                                        log::debug!(
                                            "Received destroy event for unknown container '{}'",
                                            container_name
                                        );
                                    }
                                }
                                Some(DOCKER_EVENT_ACTION_RENAME) => {
                                    if let Some(old_name) =
                                        get_container_attr(&event.actor, "oldName")
                                    {
                                        self.rename_container(&old_name, container_name).await;
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
            let names: Vec<String> = self.containers.read().await.keys().cloned().collect();

            // Collect info for all containers
            let all_info = names
                .iter()
                .map(|name| self.get_container_stats_and_inspect(name))
                .collect::<FuturesUnordered<_>>()
                .collect::<Vec<_>>()
                .await;

            // Update all containers with the collected stats
            for res in all_info.into_iter() {
                if let (Some(stats), Some(inspect)) = res {
                    // Update container if available
                    let mut containers = self.containers.write().await;
                    if let Some(container) = containers.get_mut(&stats.name) {
                        container.update(stats, inspect);
                        container.publish().await;
                    } else {
                        log::error!("Got stats for '{}', but no container!", &stats.name);
                    }
                }
            }

            // Wait for the next iteration
            time::sleep(Duration::from_secs(self.settings.poll_interval as u64)).await;
        }
    }

    async fn create_container(
        &self,
        stats: Stats,
        inspect: ContainerInspectResponse,
        image: Option<String>,
    ) {
        let mut container = Container::new(self.mqtt.clone(), stats, inspect, image);
        container.publish().await;
        self.add_container(container).await;
    }

    async fn add_container(&self, container: Container) {
        log::info!("Adding new container '{}'", container.get_name());
        {
            let mut containers = self.containers.write().await;
            if let Some(old_container) = containers.insert(container.get_name().into(), container) {
                log::debug!(
                    "Container '{}' was already available and has been replaced",
                    old_container.get_name()
                );
            }
        }

        // Update container list in MQTT
        self.publish_container_list().await;
    }

    async fn remove_container(&self, name: &str) -> Option<Container> {
        log::info!("Removing container '{}'", name);
        let mut ret;
        {
            let mut containers = self.containers.write().await;
            ret = containers.remove(name);
        }

        // Unpublish container
        if let Some(container) = ret.borrow_mut() {
            container.unpublish().await;
        }

        // Update container list in MQTT
        self.publish_container_list().await;

        ret
    }

    async fn rename_container(&self, old_name: &str, new_name: String) {
        if let Some(mut container) = self.remove_container(old_name).await {
            log::info!("Renaming container '{}' to '{}'", old_name, new_name);
            container.rename(new_name);
            self.add_container(container).await;
        } else {
            log::debug!("Received rename event for unknown container '{}'", old_name);
        }
    }

    async fn get_container_stats_and_inspect(
        &self,
        container_name: &str,
    ) -> (Option<Stats>, Option<ContainerInspectResponse>) {
        // Remove leading slash from name
        let container_name = container_name.strip_prefix('/').unwrap_or(container_name);

        // Try to get stats and inspect
        let options = StatsOptions {
            stream: false,
            one_shot: true,
        };
        let mut stats_stream = self.docker.stats(container_name, Some(options));
        let inspect_fut = self.docker.inspect_container(container_name, None);

        // Wait for futures
        let (stats_res, inspect_res) = tokio::join!(stats_stream.next(), inspect_fut);

        let stats = match stats_res {
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
                    "Got no stats result from stream in get_container_stats() for '{}'",
                    container_name
                );
                None
            }
        };

        let inspect = match inspect_res {
            Ok(val) => Some(val),
            Err(e) => {
                log::error!(
                    "Error trying to retrieve inspect for container '{}': {}",
                    container_name,
                    e
                );
                None
            }
        };

        (stats, inspect)
    }

    async fn publish_container_list(&self) {
        let containers: Vec<_> = self.containers.read().await.keys().cloned().collect();
        if let Ok(json) = serde_json::to_string(&containers) {
            let _ = self
                .mqtt
                .read()
                .await
                .publish("bridge/containers", &json, true, Some(1))
                .await;
        }
    }

    async fn remove_stale_topics(&self, container_list: String) {
        // Remove stale topics
        match serde_json::from_str::<Vec<String>>(&container_list) {
            Ok(mqtt_containers) => {
                let docker_containers: Vec<_> =
                    self.containers.read().await.keys().cloned().collect();
                let stale = mqtt_containers
                    .into_iter()
                    .filter(|name| !docker_containers.contains(name));
                for name in stale {
                    if let Err(e) = self
                        .mqtt
                        .read()
                        .await
                        .publish(&name[1..], "", true, Some(1))
                        .await
                    {
                        log::error!(
                            "Failed to remove stale topic for container '{}': {}",
                            name,
                            e
                        );
                    } else {
                        log::debug!("Removed stale topic for container '{}'", name);
                    }
                }
            }
            Err(e) => log::error!("Failed to deserialize container list from MQTT: {}", e),
        }
    }
}
