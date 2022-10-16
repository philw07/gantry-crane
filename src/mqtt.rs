use std::{collections::HashSet, fmt::Debug, sync::Arc, time::Duration};

use futures::StreamExt;
use paho_mqtt as mqtt;
use tokio::sync::RwLock;

use crate::{
    constants::{
        AVAILABILITY_TOPIC, BUFFER_SIZE_MQTT_RECV, BUFFER_SIZE_MQTT_SEND, STATE_OFFLINE,
        STATE_ONLINE,
    },
    events::{Event, EventChannel, EventReceiver, EventSender},
    settings::Settings,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: String,
    pub retained: bool,
    pub qos: i32,
}

impl MqttMessage {
    pub fn new(topic: String, payload: String, retained: bool, qos: i32) -> Self {
        Self {
            topic,
            payload,
            retained,
            qos,
        }
    }
}

impl From<mqtt::Message> for MqttMessage {
    fn from(msg: mqtt::Message) -> Self {
        Self {
            topic: msg.topic().into(),
            payload: msg.payload_str().into(),
            retained: msg.retained(),
            qos: msg.qos(),
        }
    }
}

impl From<MqttMessage> for mqtt::Message {
    fn from(msg: MqttMessage) -> Self {
        if msg.retained {
            mqtt::Message::new_retained(msg.topic, msg.payload, msg.qos)
        } else {
            mqtt::Message::new(msg.topic, msg.payload, msg.qos)
        }
    }
}

pub struct MqttClient {
    settings: Arc<Settings>,
    client: Arc<mqtt::AsyncClient>,
    username: Option<String>,
    password: Option<String>,
    receiver: Arc<RwLock<mqtt::AsyncReceiver<Option<mqtt::Message>>>>,
    published: Arc<RwLock<HashSet<String>>>,
    event_tx: EventSender,
    event_rx: Arc<RwLock<EventReceiver>>,
}

impl MqttClient {
    pub fn new(event_channel: &EventChannel, settings: Arc<Settings>) -> Result<Self, mqtt::Error> {
        let uri = format!("tcp://{}:{}", settings.mqtt.host, settings.mqtt.port);
        let options = mqtt::CreateOptionsBuilder::new()
            .mqtt_version(mqtt::MQTT_VERSION_3_1_1)
            .server_uri(uri)
            .client_id(&settings.mqtt.client_id)
            .max_buffered_messages(BUFFER_SIZE_MQTT_SEND)
            .finalize();
        let mut client = mqtt::AsyncClient::new(options)?;
        let receiver = Arc::new(RwLock::new(client.get_stream(BUFFER_SIZE_MQTT_RECV)));

        Ok(MqttClient {
            client: Arc::new(client),
            username: settings.mqtt.username.clone(),
            password: settings.mqtt.password.clone(),
            receiver,
            published: Arc::new(RwLock::new(HashSet::new())),
            event_tx: event_channel.get_sender(),
            event_rx: Arc::new(RwLock::new(event_channel.get_receiver())),
            settings,
        })
    }

    pub async fn connect(&self) -> Result<(), mqtt::Error> {
        let will = mqtt::Message::new_retained(
            format!("{}/{}", self.settings.mqtt.base_topic, AVAILABILITY_TOPIC),
            STATE_OFFLINE,
            mqtt::QOS_1,
        );
        let mut options = mqtt::ConnectOptionsBuilder::new();
        options
            .clean_session(false)
            .will_message(will)
            .automatic_reconnect(Duration::from_secs(2), Duration::from_secs(30));
        if let (Some(user), Some(pass)) = (self.username.as_ref(), self.password.as_ref()) {
            options.user_name(user).password(pass);
        }
        let options = options.finalize();

        // Try to connect to broker
        match self.client.connect(options).await {
            Ok(_) => {
                log::info!("Connected to MQTT server");
                self.publish_state(Some(true)).await;
                Ok(())
            }
            Err(e) => {
                log::error!("Failed to connect to MQTT server: {}", e);
                Err(e)
            }
        }
    }

    pub async fn disconnect(&self, clean: bool) -> bool {
        // Remove or publish availability topic
        // Since the intent is to disconnect, we'll timeout after a short moment
        let state = if clean { None } else { Some(false) };
        tokio::select! {
            _ = self.publish_state(state) => (),
            _ = tokio::time::sleep(Duration::from_secs(2)) => {
                log::debug!("Failed to publish state while disconnecting");
                // Don't disconnect, so the last will message kicks in
                return;
            },
        }

        match self.client.disconnect(None).await {
            Ok(_) => {
                log::info!("Successfully disconnected from MQTT");
                true
            }
            Err(e) => {
                log::error!("Failed to disconnect from MQTT: {}", e);
                false
            }
        }
    }

    pub async fn subscribe(&self, topic: &str) {
        if let Err(e) = self.client.subscribe(topic, mqtt::QOS_0).await {
            log::error!("Failed to subscribe to topic '{}': {}", topic, e);
        }
    }

    pub async fn event_loop(&self) {
        // Task to receive incoming message
        let published_topics = self.published.clone();
        let event_tx = self.event_tx.clone();
        let receiver = self.receiver.clone();
        let task_recv = tokio::spawn(async move {
            let mut receiver = receiver.write().await;
            loop {
                match receiver.next().await {
                    Some(msg_opt) => {
                        if let Some(msg) = msg_opt {
                            if !published_topics.read().await.contains(msg.topic()) {
                                if let Err(e) =
                                    event_tx.send(Event::MqttMessageReceived(msg.into()))
                                {
                                    log::error!("Failed to send MQTT message event: {}", e);
                                }
                            }
                        } else {
                            // None means disconnected from MQTT
                            log::debug!("Disconnected from MQTT in event loop");
                        }
                    }
                    None => {
                        log::debug!("Received None from MQTT client receiver");
                        break;
                    }
                }
            }
        });

        // Task to handle internal events
        let published_topics = self.published.clone();
        let event_rx = self.event_rx.clone();
        let client = self.client.clone();
        let task_send = tokio::spawn(async move {
            let mut event_rx = event_rx.write().await;
            loop {
                match event_rx.recv().await {
                    Ok(event) => match event {
                        Event::PublishMqttMessage(msg) => {
                            // Store topic we are going to publish
                            published_topics.write().await.insert(msg.topic.clone());

                            // Publish message in a separate task to avoid waiting
                            let topic = msg.topic.clone();
                            let fut_client = client.clone();
                            tokio::spawn(async move {
                                match fut_client.publish(msg.into()).await {
                                    Ok(()) => {
                                        log::debug!("Published MQTT message for topic '{}'", topic);
                                    }
                                    Err(e) => {
                                        log::error!("Failed to publish MQTT message: {}", e);
                                    }
                                }
                            });
                        }
                        Event::SubscribeMqttTopic(topic) => {
                            log::debug!("Subscribing to MQTT topic '{}'", topic);
                            client.subscribe(topic, 0);
                        }
                        _ => {}
                    },
                    Err(e) => {
                        log::error!("Error while reading events: {}", e);
                        break;
                    }
                }
            }
        });

        // Wait for any task to end
        tokio::select! {
            _ = task_recv => (),
            _ = task_send => (),
        }
    }

    async fn publish_state(&self, state: Option<bool>) {
        // Construct message
        let topic = format!("{}/{}", self.settings.mqtt.base_topic, AVAILABILITY_TOPIC);
        let payload = if let Some(value) = state {
            if value {
                STATE_ONLINE
            } else {
                STATE_OFFLINE
            }
        } else {
            ""
        }
        .into();
        let msg = MqttMessage::new(topic.clone(), payload, true, 1);

        // Store topic we are going to publish
        self.published.write().await.insert(topic.clone());

        // Publish message
        match self.client.publish(msg.into()).await {
            Ok(()) => {
                log::debug!("Published MQTT message for topic '{}'", topic);
            }
            Err(e) => {
                log::error!("Failed to publish MQTT message: {}", e);
            }
        }
    }
}
