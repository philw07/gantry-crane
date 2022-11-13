use std::{
    borrow::Borrow,
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use bollard::{
    container::{
        Config, CreateContainerOptions, ListContainersOptions, RenameContainerOptions, Stats,
        StatsOptions, WaitContainerOptions,
    },
    image::CreateImageOptions,
    network::ConnectNetworkOptions,
    service::{
        ContainerInspectResponse, EventActor, EventMessageTypeEnum, ImageInspect,
        MountPointTypeEnum,
    },
    system::EventsOptions,
    Docker,
};
use futures::{
    future::select_all,
    stream::{FuturesOrdered, FuturesUnordered},
    StreamExt,
};
use tokio::{sync::RwLock, time};

use crate::{
    args::GantryCraneArgs,
    constants::{
        APP_NAME, APP_VERSION, CONTAINER_REQUEST_PAUSE, CONTAINER_REQUEST_PULL_RECREATE,
        CONTAINER_REQUEST_RECREATE, CONTAINER_REQUEST_RESTART, CONTAINER_REQUEST_START,
        CONTAINER_REQUEST_STOP, CONTAINER_REQUEST_UNPAUSE, DOCKER_EVENT_ACTION_CREATE,
        DOCKER_EVENT_ACTION_DESTROY, DOCKER_EVENT_ACTION_PAUSE, DOCKER_EVENT_ACTION_RENAME,
        DOCKER_EVENT_ACTION_RESTART, DOCKER_EVENT_ACTION_START, DOCKER_EVENT_ACTION_STOP,
        DOCKER_EVENT_ACTION_UNPAUSE, DOCKER_LABEL_FILTER, SET_STATE_TOPIC,
    },
    container::Container,
    events::{Event, EventChannel},
    homeassistant::HomeAssistantIntegration,
    mqtt::{MqttClient, MqttMessage},
    settings::Settings,
    signal_handler::handle_signals,
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
        let docker = Docker::connect_with_local_defaults()?;
        let event_channel = EventChannel::new();
        let settings = Arc::new(Settings::new(args.config.as_deref())?);
        let mqtt = MqttClient::new(&event_channel, settings.clone())?;
        Ok(Self {
            docker,
            settings,
            mqtt,
            event_channel,
            containers: RwLock::new(HashMap::new()),
        })
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
            if let Err(e) = handle_signals().await {
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

        let unpublish_topics = async move {
            self.event_channel.send(Event::SubscribeMqttTopic(format!(
                "{}/#",
                self.settings.mqtt.base_topic
            )));

            let mut event_rx = self.event_channel.get_receiver();
            let topic_prefix = format!("{}/", self.settings.mqtt.base_topic);
            let mut last_message = Instant::now();
            loop {
                tokio::select! {
                    event = event_rx.recv() => {
                        if let Ok(Event::MqttMessageReceived(msg)) = event {
                            last_message = Instant::now();
                            if msg.retained && msg.topic.starts_with(&topic_prefix)
                            {
                                let msg = MqttMessage::new(msg.topic, "".into(), true, 1);
                                self.event_channel.send(Event::PublishMqttMessage(msg));
                            }
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(1)) => (),
                }

                // If no message has been received for a while, we consider the job done
                if last_message.elapsed() > Duration::from_secs(2) {
                    break;
                }
            }
        };

        tokio::select! {
            _ = select_all(tasks) => (),
            _ = mqtt_fut => log::debug!("MQTT event loop ended"),
            _ = unpublish_topics => log::info!("All topics have been unpublished"),
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
                    if let Err(e) = handle_signals().await {
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
                tokio::select! {
                    _ = select_all(tasks) => (),
                    _ = mqtt_fut => log::debug!("MQTT event loop ended"),
                    _ = self.handle_mqtt_messages() => log::debug!("handle_mqtt_messages() ended"),
                    _ = self.events_loop() => log::debug!("listen_for_events() ended"),
                    _ = self.poll_loop() => log::debug!("poll_containers() ended"),
                }
            }
            Err(e) => result = Err(e),
        }

        log::info!("Shutting down");

        // Disconnect from MQTT
        self.mqtt.disconnect(false).await;
        result
    }

    async fn handle_mqtt_messages(&self) {
        let mut receiver = self.event_channel.get_receiver();
        self.event_channel.send(Event::SubscribeMqttTopic(format!(
            "{}/#",
            self.settings.mqtt.base_topic
        )));

        loop {
            match receiver.recv().await {
                Ok(Event::MqttMessageReceived(msg)) => {
                    let subtopics: Vec<_> = msg.topic.split('/').collect();

                    // Ignore messages with another base topic or if no subtopics are given
                    if subtopics[0] != self.settings.mqtt.base_topic || subtopics.len() < 2 {
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
                    else if !msg.retained
                        && subtopics.len() == 3
                        && subtopics[2] == SET_STATE_TOPIC
                    {
                        // Ignore if container doesn't exist
                        if !self.containers.read().await.contains_key(&container_name) {
                            log::debug!(
                                "Received container request for unknown container '{}'",
                                container_name
                            );
                            continue;
                        }

                        match msg.payload.as_str() {
                            CONTAINER_REQUEST_START => {
                                log::info!(
                                    "Trying to {} container '{}'",
                                    msg.payload,
                                    container_name
                                );
                                if let Err(e) = self
                                    .docker
                                    .start_container::<String>(&container_name[1..], None)
                                    .await
                                {
                                    log::error!("Failed to {} container: {}", msg.payload, e);
                                };
                            }
                            CONTAINER_REQUEST_STOP => {
                                log::info!(
                                    "Trying to {} container '{}'",
                                    msg.payload,
                                    container_name
                                );
                                if let Err(e) =
                                    self.docker.stop_container(&container_name[1..], None).await
                                {
                                    log::error!("Failed to {} container: {}", msg.payload, e);
                                };
                            }
                            CONTAINER_REQUEST_RESTART => {
                                log::info!(
                                    "Trying to {} container '{}'",
                                    msg.payload,
                                    container_name
                                );
                                if let Err(e) = self
                                    .docker
                                    .restart_container(&container_name[1..], None)
                                    .await
                                {
                                    log::error!("Failed to {} container: {}", msg.payload, e);
                                };
                            }
                            CONTAINER_REQUEST_PAUSE => {
                                log::info!(
                                    "Trying to {} container '{}'",
                                    msg.payload,
                                    container_name
                                );
                                if let Err(e) =
                                    self.docker.pause_container(&container_name[1..]).await
                                {
                                    log::error!("Failed to {} container: {}", msg.payload, e);
                                };
                            }
                            CONTAINER_REQUEST_UNPAUSE => {
                                log::info!(
                                    "Trying to {} container '{}'",
                                    msg.payload,
                                    container_name
                                );
                                if let Err(e) =
                                    self.docker.unpause_container(&container_name[1..]).await
                                {
                                    log::error!("Failed to {} container: {}", msg.payload, e);
                                };
                            }
                            CONTAINER_REQUEST_RECREATE | CONTAINER_REQUEST_PULL_RECREATE => {
                                let pull = msg.payload.as_str() == CONTAINER_REQUEST_PULL_RECREATE;
                                log::info!("Trying to recreate container '{}'", container_name);
                                match self
                                    .docker
                                    .inspect_container(&container_name[1..], None)
                                    .await
                                {
                                    Ok(inspect) => {
                                        if let Some(image) = inspect.image.as_deref() {
                                            match self.docker.inspect_image(image).await {
                                                Ok(image_inspect) => {
                                                    self.recrate_container(
                                                        inspect,
                                                        image_inspect,
                                                        pull,
                                                    )
                                                    .await
                                                }
                                                Err(e) => log::error!(
                                                    "Failed to inspect image '{}': {}",
                                                    image,
                                                    e
                                                ),
                                            }
                                        }
                                    }
                                    Err(e) => log::error!(
                                        "Failed to inspect container '{}': {}",
                                        container_name,
                                        e
                                    ),
                                }
                            }
                            _ => log::warn!(
                                "Received unknown payload in set state message: {}",
                                msg.payload
                            ),
                        }
                    }
                }
                Err(e) => {
                    log::error!("Error while handling MQTT messages: {}", e);
                    break;
                }
                // Ignore other events
                _ => (),
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
                    .filter_map(|c| c.names?.first().cloned())
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
    async fn events_loop(&self) {
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
                                actor.clone()?.attributes?.get(attr).map(ToOwned::to_owned)
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
                                Some(
                                    DOCKER_EVENT_ACTION_START
                                    | DOCKER_EVENT_ACTION_STOP
                                    | DOCKER_EVENT_ACTION_RESTART
                                    | DOCKER_EVENT_ACTION_PAUSE
                                    | DOCKER_EVENT_ACTION_UNPAUSE,
                                ) => {
                                    // Force polling
                                    self.event_channel.send(Event::ForcePoll);
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
        let mut rx = self.event_channel.get_receiver();
        let mut polling_active = true;
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
            for res in all_info {
                if let (Some(stats), Some(inspect)) = res {
                    // Update container if available
                    let mut containers = self.containers.write().await;
                    if let Some(container) = containers.get_mut(&stats.name) {
                        container.update(&stats, &inspect);
                        container.publish().await;
                    } else {
                        log::warn!("Got stats for '{}', but no container!", &stats.name);
                    }
                }
            }

            // Wait for the next iteration or forced poll
            let next_poll =
                Instant::now() + Duration::from_secs(self.settings.poll_interval as u64);
            loop {
                // Sleep at least of 100ms
                let mut sleep_time = next_poll - Instant::now();
                if sleep_time.as_nanos() == 0 {
                    sleep_time = Duration::from_millis(100);
                }

                tokio::select! {
                    _ = time::sleep(sleep_time) => {
                        if polling_active {
                            break;
                        }
                    },
                    ev = rx.recv() => {
                        match ev {
                            Ok(Event::ForcePoll) => {
                                if polling_active {
                                    break;
                                }
                            },
                            Ok(Event::SuspendPolling) => {
                                polling_active = false;
                                log::debug!("Suspending polling");
                            },
                            Ok(Event::ResumePolling) => {
                                polling_active = true;
                                log::debug!("Resuming polling");

                                // Poll immediately after resuming
                                break;
                            },
                            Err(e) => log::error!("Error reading from channel: {}", e),
                            _ => (),
                        }
                    },
                }
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
            &stats,
            &inspect,
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
        if let Some(container) = container_opt.as_ref() {
            container.unpublish().await;
            self.event_channel
                .send(Event::ContainerRemoved(container.into()));
            log::info!("Removed container '{}'", name);
        }

        container_opt
    }

    async fn rename_container(&self, old_name: &str, new_name: String) {
        log::info!("Renaming container '{}' to '{}'", old_name, new_name);
        if let Some(mut container) = self.remove_container(old_name).await {
            container.rename(new_name);
            self.add_container(container).await;
        } else {
            log::warn!("Received rename event for unknown container '{}'", old_name);
        }
    }

    async fn recrate_container(
        &self,
        container_inspect: ContainerInspectResponse,
        image_inspect: ImageInspect,
        pull: bool,
    ) {
        let is_host_networking = Container::is_host_networking(&container_inspect);
        if let (Some(container_name), Some(container_config), Some(image_config)) = (
            container_inspect.name.as_deref(),
            container_inspect.config,
            image_inspect.config,
        ) {
            // Pull new image
            let mut image_tag = None;
            if pull {
                if let Some(mut image) = container_config.image.clone() {
                    log::info!("Pulling image '{}'", image);

                    // Add the latest tag if none is given.
                    // Otherwise all tags for the image will be pulled.
                    if !image.contains(':') {
                        image.push_str(":latest");
                    }
                    let options = CreateImageOptions {
                        from_image: image.as_str(),
                        ..Default::default()
                    };
                    let mut stream = self.docker.create_image(Some(options), None, None);
                    while let Some(res) = stream.next().await {
                        if let Err(e) = res {
                            log::error!("Error pulling image '{}': {}", image, e);
                            return;
                        }
                    }
                    drop(stream);
                    image_tag = Some(image);
                } else {
                    log::warn!(
                        "Skipping pull due to unknown image for container '{}'",
                        container_name
                    );
                }
            }

            // Create config for new container.
            // Only keep config which is different from the image config.
            let mut config = Config::from(container_config);
            macro_rules! remove_equal {
                (Option, $($x:ident),+) => {
                    $(
                        if config.$x == image_config.$x {
                            config.$x = None;
                        }
                    )*
                };
                (Vec, $($x:ident),+) => {
                    $(
                        if let (Some($x), Some(image)) = (config.$x.as_mut(), image_config.$x.as_ref()) {
                            $x.retain(|el| !image.contains(el));
                        }
                    )*
                };
                (HashMap, $($x:ident),+) => {
                    $(
                        if let (Some($x), Some(image)) =
                            (config.$x.as_mut(), image_config.$x.as_ref())
                        {
                            $x.retain(|k, v| image.get(k) != Some(v));
                        }
                    )*
                };
            }
            remove_equal!(Option, cmd, entrypoint, user, working_dir);
            remove_equal!(Vec, env);
            remove_equal!(HashMap, labels, volumes, exposed_ports);

            config.host_config = container_inspect.host_config;

            // Take over anonymous volumes
            if let Some(mounts) = container_inspect.mounts {
                let binds = config.host_config.as_ref().and_then(|hc| hc.binds.as_ref());

                // Find anonymous volumes
                let anon_binds = mounts
                    .iter()
                    .filter(|mp| mp.typ == Some(MountPointTypeEnum::VOLUME))
                    .filter_map(|mp| {
                        Some(format!(
                            "{}:{}",
                            mp.name.as_deref()?,
                            mp.destination.as_deref()?
                        ))
                    })
                    .filter(|new_bind| {
                        binds
                            .and_then(|cur_binds| {
                                cur_binds.iter().find(|cur_bind| cur_bind == &new_bind)
                            })
                            .is_none()
                    })
                    .collect::<Vec<_>>();

                // Add binds for anonymous volumes
                if !anon_binds.is_empty() {
                    if let Some(host_config) = config.host_config.as_mut() {
                        if host_config.binds.is_none() {
                            host_config.binds = Some(Vec::new());
                        }
                        if let Some(binds) = host_config.binds.as_mut() {
                            for bind in anon_binds {
                                binds.push(bind);
                            }
                        }
                    }
                }
            }

            // Only keep hostname in case it's not the default (first 12 chars of container ID)
            if let Some(id) = container_inspect.id.as_deref() {
                if let Some(hostname) = config.hostname.as_deref() {
                    if &id[..12] == hostname {
                        config.hostname = None;
                    }
                }
            }

            let name = container_name[1..].to_string();
            let name_temp = format!("{}_old", name);
            let is_autoremove = if let Some(host_config) = config.host_config.as_ref() {
                host_config.auto_remove.unwrap_or(false)
            } else {
                false
            };

            // Suspend polling while recreating container
            self.event_channel.send(Event::SuspendPolling);

            // Rename old container
            async fn rename(docker: &Docker, old: &str, new: &str) -> bool {
                if let Err(e) = docker
                    .rename_container(old, RenameContainerOptions { name: new })
                    .await
                {
                    log::error!("Failed to rename container '{}': {}", old, e);
                    return false;
                }
                true
            }
            if !rename(&self.docker, &name, &name_temp).await {
                self.event_channel.send(Event::ResumePolling);
                return;
            }

            // Stop old container
            log::debug!("Stopping old container '{}'", name_temp);
            if let Err(e) = self.docker.stop_container(&name_temp, None).await {
                log::error!("Failed to stop container '{}': {}", name_temp, e);
                self.event_channel.send(Event::ResumePolling);
                return;
            }
            _ = self
                .docker
                .wait_container::<&str>(&name_temp, None)
                .collect::<Vec<_>>()
                .await;

            // Create new container
            log::debug!("Creating new container '{}'", name);
            if let Err(e) = self
                .docker
                .create_container(Some(CreateContainerOptions { name: &name }), config)
                .await
            {
                log::error!("Failed to create container '{}': {}", container_name, e);
                // Rename old container back
                rename(&self.docker, &name_temp, &name).await;
                if let Err(e) = self.docker.start_container::<String>(&name, None).await {
                    log::error!(
                        "Failed to start old container after renaming it back: {}",
                        e
                    );
                }
                self.event_channel.send(Event::ResumePolling);
                return;
            }

            // Attach networks
            if !is_host_networking {
                if let Some(network_settings) = container_inspect.network_settings {
                    if let Some(networks) = network_settings.networks {
                        for (network, config) in networks {
                            if let Err(e) = self
                                .docker
                                .connect_network(
                                    &network,
                                    ConnectNetworkOptions {
                                        container: &name,
                                        endpoint_config: config,
                                    },
                                )
                                .await
                            {
                                log::error!(
                                    "Failed to connect container '{}' to network '{}': {}",
                                    name,
                                    network,
                                    e
                                );
                            }
                        }
                    }
                }
            }

            // Start new container
            log::debug!("Starting new container '{}'", name);
            if let Err(e) = self.docker.start_container::<String>(&name, None).await {
                log::error!("Failed to start container '{}': {}", name, e);
                // Remove new container and rename old container back
                if let Err(e) = self.docker.remove_container(&name, None).await {
                    log::error!("Failed to remove newly created container '{}': {}", name, e);
                } else {
                    rename(&self.docker, &name_temp, &name).await;
                    if let Err(e) = self.docker.start_container::<String>(&name, None).await {
                        log::error!(
                            "Failed to start old container after renaming it back: {}",
                            e
                        );
                    }
                }
                self.event_channel.send(Event::ResumePolling);
                return;
            }

            // Remove old container
            if !is_autoremove {
                if let Err(e) = self.docker.remove_container(&name_temp, None).await {
                    log::error!("Failed to remove container '{}': {}", name_temp, e);
                    self.event_channel.send(Event::ResumePolling);
                    return;
                }
            }
            _ = self
                .docker
                .wait_container(
                    &name_temp,
                    Some(WaitContainerOptions {
                        condition: "removed",
                    }),
                )
                .collect::<Vec<_>>()
                .await;

            // Delete old image, but only if a new image was pulled
            if let (Some(tag), Some(image_id)) = (image_tag, image_inspect.id) {
                if let Ok(new_image_inspect) = self.docker.inspect_image(&tag).await {
                    if new_image_inspect.id.as_deref().unwrap_or(&image_id) != image_id {
                        if !image_id.starts_with("sha256:") {
                            log::debug!("Unknown value, couldn't delete old image '{}'", image_id);
                        } else {
                            let id = &image_id[7..];
                            log::info!("Deleting old image '{}'", id);
                            if let Err(e) = self.docker.remove_image(id, None, None).await {
                                log::error!("Failed to delete old image '{}': {}", id, e);
                            }
                        }
                    }
                }
            }

            // Resume polling
            self.event_channel.send(Event::ResumePolling);
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
            one_shot: false,
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
            if let Some(config) = inspect.config.as_ref() {
                if let Some(labels) = &config.labels {
                    if let Some(value) = labels.get(DOCKER_LABEL_FILTER) {
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
