#![no_std]

pub mod esp;

use embassy_net::{IpAddress, IpEndpoint, Stack};
use embassy_time::{Duration, Timer};
use log::{error, info, warn};

#[embassy_executor::task]
pub async fn hello_run() -> ! {
    log::info!("Embassy initialized!");

    loop {
        log::info!("Hello world!");
        Timer::after(Duration::from_secs(1)).await;
    }
}

pub struct MqttConfig {
    broker_ip: [u8; 4],
    port: u16,
    topic: &'static str,
    client_id: &'static str,
}

/// Connects to the MQTT broker, subscribes to [`MQTT_TOPIC`], and prints
/// every received event.  Reconnects automatically on any error.
#[embassy_executor::task]
pub async fn mqtt_run(stack: Stack<'static>, config: MqttConfig) -> ! {
    // TCP socket I/O buffers – sized to fit the largest expected MQTT packet.
    let mut rx_buffer = [0u8; 4_096];
    let mut tx_buffer = [0u8; 4_096];

    // Scratch buffers used by the MqttClient internally.
    let mut mqtt_recv_buf = [0u8; 512];
    let mut mqtt_send_buf = [0u8; 512];

    let broker_endpoint = IpEndpoint::new(
        IpAddress::v4(
            config.broker_ip[0],
            config.broker_ip[1],
            config.broker_ip[2],
            config.broker_ip[3],
        ),
        config.port,
    );

    loop {
        // ----------------------------------------------------------------
        // 1. Open a TCP connection to the broker.
        // ----------------------------------------------------------------
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(30)));

        info!(
            "Connecting TCP to {:?}:{}...",
            config.broker_ip, config.port
        );
        if let Err(e) = socket.connect(broker_endpoint).await {
            error!("TCP connect failed: {:?}", e);
            Timer::after(Duration::from_secs(5)).await;
            continue; // socket is dropped here, buffers become available again
        }
        info!("TCP connection established");

        // ----------------------------------------------------------------
        // 2. Build the MQTT client configuration.
        // ----------------------------------------------------------------
        let mut config: ClientConfig<'_, 5, CountingRng> =
            ClientConfig::new(MqttVersion::MQTTv5, CountingRng(20_000));
        config.add_client_id(client_id);
        config.add_max_subscribe_qos(rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS0);
        config.max_packet_size = 512;

        // ----------------------------------------------------------------
        // 3. Create the MQTT client and perform the CONNECT handshake.
        // ----------------------------------------------------------------
        let mut client = MqttClient::<_, 5, _>::new(
            socket,
            &mut mqtt_send_buf,
            512,
            &mut mqtt_recv_buf,
            512,
            config,
        );

        match client.connect_to_broker().await {
            Ok(_) => info!("MQTT CONNECT accepted by broker"),
            Err(ReasonCode::NetworkError) => {
                error!("MQTT broker unreachable (network error)");
                Timer::after(Duration::from_secs(5)).await;
                continue;
            }
            Err(code) => {
                error!("MQTT CONNECT rejected, reason code: {:?}", code);
                Timer::after(Duration::from_secs(5)).await;
                continue;
            }
        }

        // ----------------------------------------------------------------
        // 4. Subscribe to the configured topic.
        // ----------------------------------------------------------------
        match client.subscribe_to_topic(config.topic).await {
            Ok(_) => info!("Subscribed to '{}'", config.topic),
            Err(e) => {
                error!("SUBSCRIBE failed: {:?}", e);
                continue;
            }
        }

        // ----------------------------------------------------------------
        // 5. Event loop – receive and print incoming messages.
        // ----------------------------------------------------------------
        info!("Listening for MQTT messages...");
        loop {
            match client.receive_message().await {
                Ok((topic, payload)) => {
                    let text = core::str::from_utf8(payload).unwrap_or("<binary payload>");
                    info!("[{}] {}", topic, text);
                }
                Err(ReasonCode::NetworkError) => {
                    warn!("MQTT connection lost, reconnecting...");
                    break;
                }
                Err(code) => {
                    error!("MQTT receive error, reason code: {:?}", code);
                    break;
                }
            }
        }

        // Brief pause before the next connection attempt.
        Timer::after(Duration::from_secs(5)).await;
    }
}
