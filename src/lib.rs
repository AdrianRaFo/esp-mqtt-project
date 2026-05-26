#![no_std]

pub mod esp;

use embassy_net::tcp::TcpSocket;
use embassy_net::{IpAddress, Stack};
use embassy_time::{Duration, Timer};
use log::{error, info, warn};
use minimq::{Buffers, ConfigBuilder, ConnectEvent, Session, TopicFilter};

#[embassy_executor::task]
pub async fn hello_task() -> ! {
    info!("Embassy initialized!");

    loop {
        info!("Hello world!");
        Timer::after(Duration::from_secs(1)).await;
    }
}

pub struct MqttConfig {
    pub broker: IpAddress,
    pub port: u16,
    pub topic: &'static str,
    pub client_id: &'static str,
}

/// Connects to the MQTT broker, subscribes to [`MQTT_TOPIC`], and prints
/// every received event.  Reconnects automatically on any error.
#[embassy_executor::task]
pub async fn mqtt_task(stack: Stack<'static>, mqtt_config: MqttConfig) -> ! {
    // ----------------------------
    // static buffers (required)
    // ----------------------------
    let mut tcp_rx: [u8; 4096] = [0; 4096];
    let mut tcp_tx: [u8; 4096] = [0; 4096];

    loop {
        let mut mqtt_rx: [u8; 4096] = [0; 4096];
        let mut mqtt_tx: [u8; 4096] = [0; 4096];

        let buffers = Buffers::new(&mut mqtt_rx, &mut mqtt_tx);

        let config = ConfigBuilder::new(buffers)
            .client_id(mqtt_config.client_id)
            .unwrap();
        let mut session = Session::new(config);

        // ----------------------------
        // TCP connection
        // ----------------------------

        let mut socket = TcpSocket::new(stack, &mut tcp_rx, &mut tcp_tx);

        if let Err(e) = socket.connect((mqtt_config.broker, mqtt_config.port)).await {
            warn!("TCP connect failed: {:?}", e);
            Timer::after(Duration::from_secs(2)).await;
            continue;
        }

        info!("TCP connected");

        // ----------------------------
        // MQTT CONNECT
        // ----------------------------
        let conn = match session.connect(socket).await {
            Ok(c) => c,
            Err(e) => {
                error!("MQTT connect failed: {:?}", e);
                Timer::after(Duration::from_secs(2)).await;
                continue;
            }
        };

        match conn {
            ConnectEvent::Connected => {
                info!("fresh session");
            }
            ConnectEvent::Reconnected => {
                info!("session resumed");
            }
        }

        // ----------------------------
        // SUBSCRIBE
        // ----------------------------
        if let Err(e) = session
            .subscribe(&[TopicFilter::new("sensors/temp")], &[])
            .await
        {
            error!("subscribe failed: {:?}", e);
            continue;
        }

        info!("subscribed");

        // ----------------------------
        // CONSUMER LOOP
        // ----------------------------
        loop {
            match session.recv().await {
                Ok(msg) => {
                    info!("topic={} payload={:?}", msg.topic(), msg.payload());
                }

                Err(minimq::Error::Disconnected) => {
                    warn!("mqtt disconnected");
                    break;
                }

                Err(e) => {
                    error!("mqtt error: {:?}", e);
                    break;
                }
            }

            // keep session alive (important in minimq)
            let _ = session.poll().await;
        }

        Timer::after(Duration::from_secs(1)).await;
    }
}
