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
            Self::new_retained(msg.topic, msg.payload, msg.qos)
        } else {
            Self::new(msg.topic, msg.payload, msg.qos)
        }
    }
}

pub struct MqttClient {
    settings: Arc<Settings>,
    client: Arc<mqtt::AsyncClient>,
    receiver: Arc<RwLock<mqtt::AsyncReceiver<Option<mqtt::Message>>>>,
    published: Arc<RwLock<HashSet<String>>>,
    subscribed: Arc<RwLock<Vec<String>>>,
    event_tx: EventSender,
    event_rx: Arc<RwLock<EventReceiver>>,
}

impl MqttClient {
    pub fn new(event_channel: &EventChannel, settings: Arc<Settings>) -> Result<Self, mqtt::Error> {
        let scheme = if settings.mqtt.tls_encryption {
            "ssl"
        } else {
            "tcp"
        };
        let uri = format!("{}://{}:{}", scheme, settings.mqtt.host, settings.mqtt.port);
        let options = mqtt::CreateOptionsBuilder::new()
            .mqtt_version(mqtt::MQTT_VERSION_3_1_1)
            .server_uri(uri)
            .client_id(&settings.mqtt.client_id)
            .max_buffered_messages(BUFFER_SIZE_MQTT_SEND)
            .finalize();
        let mut client = mqtt::AsyncClient::new(options)?;
        let receiver = Arc::new(RwLock::new(client.get_stream(BUFFER_SIZE_MQTT_RECV)));

        Ok(Self {
            client: Arc::new(client),
            receiver,
            published: Arc::new(RwLock::new(HashSet::new())),
            subscribed: Arc::new(RwLock::new(Vec::new())),
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

        // Authentication
        if let Some(user) = self.settings.mqtt.username.as_ref() {
            options.user_name(user);
        }
        if let Some(password) = self.settings.mqtt.password.as_ref() {
            options.password(password);
        }

        // Encryption
        if self.settings.mqtt.tls_encryption {
            let mut ssl_options = mqtt::SslOptionsBuilder::new();
            if let Some(path) = self.settings.mqtt.ca_certificate.as_ref() {
                ssl_options.trust_store(path)?;
            }
            if let Some(path) = self.settings.mqtt.client_certificate.as_ref() {
                ssl_options.key_store(path)?;
            }
            if let Some(path) = self.settings.mqtt.client_key.as_ref() {
                ssl_options.private_key(path)?;
            }
            options.ssl_options(ssl_options.finalize());
        }

        // Store availabiliy topic in published topics
        let availabiliy_topic = self.get_availability_topic();
        self.published
            .write()
            .await
            .insert(availabiliy_topic.clone());

        // Set connected callback to publish availability. This will make sure the
        // availabiliy is set to online even after losing the connection temporarily
        let event_tx = self.event_tx.clone();
        self.client.set_connected_callback(move |client| {
            log::info!("Connected to MQTT server");
            _ = event_tx.send(Event::MqttConnected);

            // Publish state
            let msg = mqtt::Message::new_retained(&availabiliy_topic, STATE_ONLINE, 0);
            client.publish(msg);
        });

        // Try to connect to broker
        match self.client.connect(options.finalize()).await {
            Ok(_) => Ok(()),
            Err(e) => {
                log::error!("Failed to connect to MQTT server: {}", e);
                Err(e)
            }
        }
    }

    pub async fn disconnect(&self, clean: bool) {
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
            }
            Err(e) => {
                log::error!("Failed to disconnect from MQTT: {}", e);
            }
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
        let subscribed_topics = self.subscribed.clone();
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
                            if let Err(e) = client.subscribe(&topic, mqtt::QOS_0).await {
                                log::error!("Failed to subscribe to topic '{}': {}", topic, e);
                            } else {
                                subscribed_topics.write().await.push(topic);
                            }
                        }
                        Event::MqttConnected => {
                            let topics = subscribed_topics.read().await;
                            if topics.len() > 0 {
                                log::debug!("Resubscribing to {} topics", topics.len());
                                let qos = vec![0; topics.len()];
                                client.subscribe_many(&topics, &qos);
                            }
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

    fn get_availability_topic(&self) -> String {
        format!("{}/{}", self.settings.mqtt.base_topic, AVAILABILITY_TOPIC)
    }

    async fn publish_state(&self, state: Option<bool>) {
        // Construct message
        let topic = self.get_availability_topic();
        let payload = state.map_or("", |value| if value { STATE_ONLINE } else { STATE_OFFLINE });
        let msg = mqtt::Message::new_retained(&topic, payload, 1);

        // Publish message
        match self.client.publish(msg).await {
            Ok(()) => {
                log::debug!("Published MQTT message for topic '{}'", topic);
            }
            Err(e) => {
                log::error!("Failed to publish MQTT message: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use crate::{
        constants::AVAILABILITY_TOPIC,
        events::{Event, EventChannel},
        settings::Settings,
    };

    use super::{MqttClient, MqttMessage};
    use paho_mqtt as mqtt;

    #[test]
    fn test_mqtt_message() {
        let topic = "test/topic";
        let payload = "[{\"test\": true}]";
        let retained = true;
        let qos = 2;

        // Internal to paho
        let msg = MqttMessage::new(topic.into(), payload.into(), retained, qos);
        let paho_msg = mqtt::Message::from(msg);
        assert_eq!(paho_msg.topic(), topic);
        assert_eq!(paho_msg.payload_str(), payload);
        assert_eq!(paho_msg.retained(), retained);
        assert_eq!(paho_msg.qos(), qos);

        // Paho to internal
        let paho_msg = mqtt::Message::new(topic, payload, qos);
        let paho_msg_retained = mqtt::Message::new_retained(topic, payload, qos);
        let msg = MqttMessage::from(paho_msg);
        assert_eq!(msg.topic, topic);
        assert_eq!(msg.payload, payload);
        assert_eq!(msg.retained, false);
        assert_eq!(msg.qos, qos);
        let msg = MqttMessage::from(paho_msg_retained);
        assert_eq!(msg.topic, topic);
        assert_eq!(msg.payload, payload);
        assert_eq!(msg.retained, true);
        assert_eq!(msg.qos, qos);
    }

    #[tokio::test]
    async fn test_new() {
        let settings = temp_env::with_var("MQTT_CLIENT_ID", "test-client-id-abc123".into(), || {
            Settings::new(None).unwrap()
        });

        let event_channel = EventChannel::new();
        let mqtt = MqttClient::new(&event_channel, Arc::new(settings));
        assert!(mqtt.is_ok());
        let mqtt = mqtt.unwrap();
        assert_eq!(mqtt.client.mqtt_version(), mqtt::MQTT_VERSION_3_1_1);
        assert_eq!(mqtt.client.client_id(), "test-client-id-abc123");

        assert_eq!(mqtt.event_rx.read().await.len(), 0);
        event_channel.send(Event::MqttConnected);
        assert_eq!(mqtt.event_rx.read().await.len(), 1);

        let recv = event_channel.get_receiver();
        assert_eq!(recv.len(), 0);
        mqtt.event_tx.send(Event::MqttConnected).unwrap();
        assert_eq!(recv.len(), 1);
    }

    #[test]
    fn test_availability_topic() {
        let settings = temp_env::with_var("MQTT_BASE_TOPIC", "test/base".into(), || {
            Settings::new(None).unwrap()
        });

        let event_channel = EventChannel::new();
        let mqtt = MqttClient::new(&event_channel, Arc::new(settings)).unwrap();
        assert_eq!(
            mqtt.get_availability_topic(),
            format!("test/base/{}", AVAILABILITY_TOPIC)
        );
    }
}
