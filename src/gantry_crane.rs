use std::{borrow::Borrow, collections::HashMap, sync::Arc, time::Duration};

use anyhow::Result;
use bollard::{
    container::{ListContainersOptions, Stats, StatsOptions},
    service::{ContainerInspectResponse, EventActor, EventMessageTypeEnum},
    system::EventsOptions,
    Docker,
};
use futures::{
    future::select_all,
    stream::{FuturesOrdered, FuturesUnordered},
    StreamExt,
};
use tokio::{
    signal::unix::{signal, SignalKind},
    sync::{
        mpsc::{Receiver, Sender},
        RwLock,
    },
    time,
};

use crate::{
    args::GantryCraneArgs,
    constants::{
        APP_NAME, APP_VERSION, BUFFER_SIZE_POLL_CHANNEL, DOCKER_EVENT_ACTION_CREATE,
        DOCKER_EVENT_ACTION_DESTROY, DOCKER_EVENT_ACTION_PAUSE, DOCKER_EVENT_ACTION_RENAME,
        DOCKER_EVENT_ACTION_RESTART, DOCKER_EVENT_ACTION_START, DOCKER_EVENT_ACTION_STOP,
        DOCKER_EVENT_ACTION_UNPAUSE, DOCKER_LABEL_FILTER, SET_STATE_TOPIC,
    },
    container::Container,
    events::{Event, EventChannel},
    homeassistant::HomeAssistantIntegration,
    mqtt::{MqttClient, MqttMessage},
    settings::Settings,
};

pub struct GantryCrane {
    docker: Docker,
    settings: Arc<Settings>,
    mqtt: MqttClient,
    event_channel: EventChannel,
    containers: RwLock<HashMap<String, Container>>,
}

impl GantryCrane {
    pub fn new(args: &GantryCraneArgs) -> Result<Self> {
        match Docker::connect_with_local_defaults() {
            Ok(docker) => {
                let event_channel = EventChannel::new();
                let settings = Arc::new(Settings::new(args.config.as_deref())?);
                let mqtt = MqttClient::new(&event_channel, settings.clone())?;
                Ok(GantryCrane {
                    docker,
                    settings,
                    mqtt,
                    event_channel,
                    containers: RwLock::new(HashMap::new()),
                })
            }
            Err(e) => panic!("Failed to connect to docker: {}", e),
        }
    }

    pub async fn run_clean(&self) -> Result<()> {
        log::info!(
            "{} {} - Cleaning up retained MQTT messages, this will take a few seconds",
            APP_NAME,
            APP_VERSION
        );

        // Connect to MQTT
        self.mqtt.connect().await?;

        let mut tasks = Vec::new();

        // Setup signal handler
        tasks.push(tokio::spawn(async {
            if let Err(e) = Self::handle_signals().await {
                log::error!("An error occurred trying to setup signal handlers: {}", e);
            }
        }));

        // Run MQTT client event loop
        let mqtt_fut = self.mqtt.event_loop();

        // Setup home assistant integration
        if self.settings.homeassistant.active {
            let mut ha = HomeAssistantIntegration::new(self.settings.clone(), &self.event_channel);
            tasks.push(tokio::spawn(async move { ha.run().await }));
        }

        tokio::select! {
            _ = select_all(tasks) => (),
            _ = mqtt_fut => log::debug!("MQTT event loop ended"),
            _ = self.handle_mqtt_messages() => log::debug!("handle_mqtt_messages() ended"),

            // Run for a few seconds
            _ = tokio::time::sleep(Duration::from_secs(5)) => (),
        }

        // Disconnect from MQTT
        self.mqtt.disconnect(true).await;

        Ok(())
    }

    pub async fn run(&self) -> Result<()> {
        let mut result = Ok(());
        log::info!("Starting {} {}", APP_NAME, APP_VERSION);

        // Connect to MQTT
        self.mqtt.connect().await?;

        // Get available containers
        match self.get_available_containers().await {
            Ok(()) => {
                let mut tasks = Vec::new();

                // Setup signal handler
                tasks.push(tokio::spawn(async {
                    if let Err(e) = Self::handle_signals().await {
                        log::error!("An error occurred trying to setup signal handlers: {}", e);
                    }
                }));

                // Run MQTT client event loop
                let mqtt_fut = self.mqtt.event_loop();

                // Setup home assistant integration
                if self.settings.homeassistant.active {
                    let mut ha =
                        HomeAssistantIntegration::new(self.settings.clone(), &self.event_channel);
                    tasks.push(tokio::spawn(async move { ha.run().await }));
                }

                // Trannsmit initial containers
                let initial_containers: Vec<_> = {
                    let containers = self.containers.read().await;
                    containers.iter().map(|v| v.1.clone()).collect()
                };
                let tx = self.event_channel.get_sender();
                for container in initial_containers {
                    if let Err(e) = tx.send(Event::ContainerCreated(container.into())) {
                        log::error!("Failed to send event: {}", e);
                    }
                }

                // Run tasks until the first one finishes
                let (tx, rx) = tokio::sync::mpsc::channel::<()>(BUFFER_SIZE_POLL_CHANNEL);
                tokio::select! {
                    _ = select_all(tasks) => (),
                    _ = mqtt_fut => log::debug!("MQTT event loop ended"),
                    _ = self.handle_mqtt_messages() => log::debug!("handle_mqtt_messages() ended"),
                    _ = self.events_loop(tx) => log::debug!("listen_for_events() ended"),
                    _ = self.poll_loop(rx) => log::debug!("poll_containers() ended"),
                }
            }
            Err(e) => result = Err(e),
        }

        log::info!("Shutting down");

        // Disconnect from MQTT
        self.mqtt.disconnect(false).await;
        result
    }

    async fn handle_signals() -> Result<()> {
        let mut sigint = signal(SignalKind::interrupt())?;
        let mut sigterm = signal(SignalKind::terminate())?;

        tokio::select! {
            _ = sigint.recv() => log::debug!("Received SIGINT signal"),
            _ = sigterm.recv() => log::debug!("Received SIGTERM signal"),
        };
        Ok(())
    }

    async fn handle_mqtt_messages(&self) {
        let mut receiver = self.event_channel.get_receiver();
        self.mqtt
            .subscribe(&format!("{}/#", self.settings.mqtt.base_topic))
            .await;

        while let Ok(event) = receiver.recv().await {
            if let Event::MqttMessageReceived(msg) = event {
                let subtopics: Vec<_> = msg.topic.split('/').collect();

                // Ignore messages with another base topic or if no subtopics are given
                if !subtopics[0].starts_with(&self.settings.mqtt.base_topic) || subtopics.len() < 2
                {
                    continue;
                }

                log::debug!("Received MQTT message for topic '{}'", msg.topic);
                let container_name = format!("/{}", subtopics[1]);

                // Remove potential stale containers
                if msg.retained && subtopics.len() == 2 {
                    // Ignore message if container exists
                    if self.containers.read().await.contains_key(&container_name) {
                        continue;
                    }

                    log::debug!("Unpublishing stale container '{}'", container_name);
                    let topic =
                        format!("{}/{}", self.settings.mqtt.base_topic, &container_name[1..]);
                    let msg = MqttMessage::new(topic, "".into(), true, 1);
                    self.event_channel.send(Event::PublishMqttMessage(msg));
                }
                // Handle container requests
                else if !msg.retained && subtopics.len() == 3 && subtopics[2] == SET_STATE_TOPIC {
                    // Ignore if container doesn't exist
                    if !self.containers.read().await.contains_key(&container_name) {
                        continue;
                    }

                    match msg.payload.as_str() {
                        DOCKER_EVENT_ACTION_START => {
                            log::info!("Trying to {} container '{}'", msg.payload, container_name);
                            if let Err(e) = self
                                .docker
                                .start_container::<String>(&container_name[1..], None)
                                .await
                            {
                                log::error!("Failed to {} container: {}", msg.payload, e);
                            };
                        }
                        DOCKER_EVENT_ACTION_STOP => {
                            log::info!("Trying to {} container '{}'", msg.payload, container_name);
                            if let Err(e) =
                                self.docker.stop_container(&container_name[1..], None).await
                            {
                                log::error!("Failed to {} container: {}", msg.payload, e);
                            };
                        }
                        DOCKER_EVENT_ACTION_RESTART => {
                            log::info!("Trying to {} container '{}'", msg.payload, container_name);
                            if let Err(e) = self
                                .docker
                                .restart_container(&container_name[1..], None)
                                .await
                            {
                                log::error!("Failed to {} container: {}", msg.payload, e);
                            };
                        }
                        DOCKER_EVENT_ACTION_PAUSE => {
                            log::info!("Trying to {} container '{}'", msg.payload, container_name);
                            if let Err(e) = self.docker.pause_container(&container_name[1..]).await
                            {
                                log::error!("Failed to {} container: {}", msg.payload, e);
                            };
                        }
                        DOCKER_EVENT_ACTION_UNPAUSE => {
                            log::info!("Trying to {} container '{}'", msg.payload, container_name);
                            if let Err(e) =
                                self.docker.unpause_container(&container_name[1..]).await
                            {
                                log::error!("Failed to {} container: {}", msg.payload, e);
                            };
                        }
                        _ => log::warn!(
                            "Received unknown payload in set state message: {}",
                            msg.payload
                        ),
                    }
                }
            }
        }
    }

    /// Adds all containers from docker
    async fn get_available_containers(&self) -> Result<()> {
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
                        if self.is_container_enabled(&inspect) {
                            self.create_container(stats, inspect, image).await;
                        }
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
    async fn events_loop(&self, tx: Sender<()>) {
        // Listen to events
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

                            // Handle events we care about
                            match event.action.as_deref() {
                                Some(DOCKER_EVENT_ACTION_CREATE) => {
                                    if let (Some(stats), Some(inspect)) =
                                        self.get_container_stats_and_inspect(&container_name).await
                                    {
                                        let image = get_container_attr(&event.actor, "image");
                                        if self.is_container_enabled(&inspect) {
                                            self.create_container(stats, inspect, image).await;
                                        }
                                    }
                                }
                                Some(DOCKER_EVENT_ACTION_DESTROY) => {
                                    if self.remove_container(&container_name).await.is_none() {
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
                                Some(DOCKER_EVENT_ACTION_START)
                                | Some(DOCKER_EVENT_ACTION_STOP)
                                | Some(DOCKER_EVENT_ACTION_RESTART)
                                | Some(DOCKER_EVENT_ACTION_PAUSE)
                                | Some(DOCKER_EVENT_ACTION_UNPAUSE) => {
                                    // Inform poll loop
                                    if let Err(e) = tx.send(()).await {
                                        log::error!(
                                            "Failed to send message through channel: {}",
                                            e
                                        );
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
    async fn poll_loop(&self, mut rx: Receiver<()>) {
        let mut channel_open = true;
        loop {
            log::info!("Polling all containers");
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
                        log::warn!("Got stats for '{}', but no container!", &stats.name);
                    }
                }
            }

            // Wait for the next iteration or message
            let sleep_fut = time::sleep(Duration::from_secs(self.settings.poll_interval as u64));
            if channel_open {
                tokio::select! {
                    _ = sleep_fut => (),
                    msg = rx.recv() => {
                        if msg.is_none() {
                            channel_open = false;
                        }
                    },
                }
            } else {
                sleep_fut.await;
            }
        }
    }

    async fn create_container(
        &self,
        stats: Stats,
        inspect: ContainerInspectResponse,
        image: Option<String>,
    ) {
        let container = Container::new(
            self.settings.clone(),
            &self.event_channel,
            stats,
            inspect,
            image,
        );
        container.publish().await;
        self.add_container(container).await;
    }

    async fn add_container(&self, container: Container) {
        log::info!("Adding new container '{}'", container.get_name());
        {
            let event_container = container.borrow().into();
            let mut containers = self.containers.write().await;
            if let Some(old_container) = containers.insert(container.get_name().into(), container) {
                log::debug!(
                    "Container '{}' was already available and has been replaced",
                    old_container.get_name()
                );
            }
            self.event_channel
                .send(Event::ContainerCreated(event_container));
        }
    }

    async fn remove_container(&self, name: &str) -> Option<Container> {
        log::debug!("Removing container '{}'", name);
        let container_opt = { self.containers.write().await.remove(name) };
        if let Some(container) = container_opt.borrow() {
            container.unpublish().await;
            self.event_channel
                .send(Event::ContainerRemoved(container.into()));
            log::info!("Removed container '{}'", name);
        }

        container_opt
    }

    async fn rename_container(&self, old_name: &str, new_name: String) {
        if let Some(mut container) = self.remove_container(old_name).await {
            log::info!("Renaming container '{}' to '{}'", old_name, new_name);
            container.rename(new_name);
            self.add_container(container).await;
        } else {
            log::warn!("Received rename event for unknown container '{}'", old_name);
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
                log::warn!("Got no stats result from stream for '{}'", container_name);
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

    fn is_container_enabled(&self, inspect: &ContainerInspectResponse) -> bool {
        if self.settings.filter_by_label {
            if let Some(config) = &inspect.config {
                if let Some(labels) = &config.labels {
                    if let Some(value) = labels.get(DOCKER_LABEL_FILTER) {
                        println!("/// {}", value);
                        return value == "true";
                    }
                }
            }

            false
        } else {
            true
        }
    }
}
