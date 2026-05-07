//! MQTT client binary for ESP32-S3.
//!
//! Connects to a WiFi network, obtains an IP via DHCP, then establishes an
//! MQTT v5 connection, subscribes to a topic, and prints every received event
//! to the serial monitor.
//!
//! # Configuration
//!
//! Set the following environment variables before building (or replace the
//! `env!` calls below with string literals):
//!
//! ```shell
//! export WIFI_SSID="your-ssid"
//! export WIFI_PASSWORD="your-password"
//! ```
//!
//! Also update `MQTT_BROKER_IP` and `MQTT_TOPIC` to match your setup.

#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

// Provides the `#[panic_handler]` for ESP builds.
extern crate esp_backtrace;

// extern crate alloc;

// use embassy_executor::Spawner;
// use embassy_net::{IpAddress, IpEndpoint, Stack, StackResources, tcp::TcpSocket};
// use embassy_time::{Duration, Timer};
// use esp_backtrace as _;
// use esp_hal::clock::CpuClock;
// use esp_hal::rng::Rng;
// use esp_hal::timer::timg::TimerGroup;
// use esp_wifi::{
//     EspWifiController,
//     wifi::{
//         ClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent, WifiStaDevice,
//         WifiState,
//     },
// };
// use log::{error, info, warn};
// use rust_mqtt::{
//     client::client::MqttClient,
//     client::client_config::{ClientConfig, MqttVersion},
//     packet::v5::reason_codes::ReasonCode,
//     utils::rng_generator::CountingRng,
// };
// use static_cell::StaticCell;

// esp_bootloader_esp_idf::esp_app_desc!();

// // ---------------------------------------------------------------------------
// // Configuration – adjust these to match your environment
// // ---------------------------------------------------------------------------

// /// WiFi SSID, read from the WIFI_SSID environment variable at compile time.
// const SSID: &str = env!("WIFI_SSID");

// /// WiFi password, read from the WIFI_PASSWORD environment variable at compile time.
// const PASSWORD: &str = env!("WIFI_PASSWORD");

// /// MQTT broker IPv4 address (bytes).  Change to your broker's IP.
// const MQTT_BROKER_IP: [u8; 4] = [192, 168, 1, 100];

// /// MQTT broker port (1883 = plain TCP, 8883 = TLS).
// const MQTT_PORT: u16 = 1883;

// /// MQTT client identifier – must be unique per broker session.
// const MQTT_CLIENT_ID: &str = "esp32s3-client";

// /// Topic to subscribe to.  MQTT wildcards ('+', '#') are allowed.
// const MQTT_TOPIC: &str = "test/#";

// // ---------------------------------------------------------------------------
// // Static storage required by embassy-net and esp-wifi
// // ---------------------------------------------------------------------------

// /// Heap for dynamic allocations (needed by rust-mqtt's AllocBuffer and other
// /// crates that use `alloc`).
// esp_alloc::heap_allocator!(size: 72 * 1024);

// /// Socket resources for the embassy-net stack (3 concurrent sockets).
// static STACK_RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();

// /// Owns the WiFi radio controller for the lifetime of the program.
// static WIFI_INIT: StaticCell<EspWifiController<'static>> = StaticCell::new();

// // ---------------------------------------------------------------------------
// // Embassy tasks
// // ---------------------------------------------------------------------------

// /// Drives the WiFi connection state machine.
// ///
// /// Handles initial connect and automatic reconnect after a disconnect.
// #[embassy_executor::task]
// async fn wifi_task(mut controller: WifiController<'static>) {
//     loop {
//         // If already connected, block until a disconnect event arrives.
//         if matches!(esp_wifi::wifi::get_wifi_state(), WifiState::StaConnected) {
//             controller.wait_for_event(WifiEvent::StaDisconnected).await;
//             Timer::after(Duration::from_millis(5_000)).await;
//         }

//         // Start the radio if it hasn't been started yet.
//         if !matches!(controller.is_started(), Ok(true)) {
//             let client_config = Configuration::Client(ClientConfiguration {
//                 ssid: SSID.try_into().unwrap(),
//                 password: PASSWORD.try_into().unwrap(),
//                 ..Default::default()
//             });
//             controller.set_configuration(&client_config).unwrap();
//             info!("Starting WiFi radio...");
//             controller.start_async().await.unwrap();
//             info!("WiFi radio started");
//         }

//         info!("Connecting to '{}' ...", SSID);
//         match controller.connect_async().await {
//             Ok(_) => info!("WiFi connected!"),
//             Err(e) => {
//                 warn!("WiFi connect failed: {:?}", e);
//                 Timer::after(Duration::from_millis(5_000)).await;
//             }
//         }
//     }
// }

// /// Runs the embassy-net network stack (processes packets, drives DHCP, etc.).
// #[embassy_executor::task]
// async fn net_task(runner: embassy_net::Runner<'static, WifiDevice<'static, WifiStaDevice>>) {
//     runner.run().await
// }

// // ---------------------------------------------------------------------------
// // Entry point
// // ---------------------------------------------------------------------------

// #[allow(
//     clippy::large_stack_frames,
//     reason = "it's not unusual to allocate larger buffers etc. in main"
// )]
// #[esp_rtos::main]
// async fn main(spawner: Spawner) -> ! {
//     esp_println::logger::init_logger_from_env();

//     // Initialise esp-hal.
//     let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
//     let peripherals = esp_hal::init(config);

//     // TIMG0 drives the esp-rtos / embassy time-driver.
//     // TIMG1 is handed to esp-wifi for its internal scheduler.
//     let timg0 = TimerGroup::new(peripherals.TIMG0);
//     let timg1 = TimerGroup::new(peripherals.TIMG1);

//     let sw_interrupt =
//         esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
//     esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

//     info!("Embassy initialised");

//     // Initialise the WiFi radio.
//     let rng = Rng::new(peripherals.RNG);
//     let wifi_init =
//         WIFI_INIT.init(esp_wifi::init(timg1.timer0, rng, peripherals.RADIO_CLK).unwrap());

//     let (wifi_interface, controller) =
//         esp_wifi::wifi::new_with_mode(wifi_init, peripherals.WIFI, WifiStaDevice).unwrap();

//     // Build the embassy-net stack with DHCP.
//     let net_config = embassy_net::Config::dhcpv4(Default::default());
//     let (stack, runner) = embassy_net::new(
//         wifi_interface,
//         net_config,
//         STACK_RESOURCES.init(StackResources::new()),
//         /* random_seed */ 0xDEAD_BEEF_CAFE_BABEu64,
//     );

//     spawner.spawn(wifi_task(controller)).ok();
//     spawner.spawn(net_task(runner)).ok();

//     // Wait until the network link is up.
//     info!("Waiting for WiFi link...");
//     loop {
//         if stack.is_link_up() {
//             break;
//         }
//         Timer::after(Duration::from_millis(500)).await;
//     }

//     // Wait for a DHCP-assigned address.
//     info!("Waiting for IP address (DHCP)...");
//     loop {
//         if let Some(cfg) = stack.config_v4() {
//             info!("IP address: {}", cfg.address);
//             break;
//         }
//         Timer::after(Duration::from_millis(500)).await;
//     }

//     // Run the MQTT loop indefinitely (never returns under normal operation).
//     mqtt_loop(stack).await
// }

// // ---------------------------------------------------------------------------
// // MQTT client loop
// // ---------------------------------------------------------------------------

// /// Connects to the MQTT broker, subscribes to [`MQTT_TOPIC`], and prints
// /// every received event.  Reconnects automatically on any error.
// async fn mqtt_loop(stack: Stack<'_>) -> ! {
//     // TCP socket I/O buffers – sized to fit the largest expected MQTT packet.
//     let mut rx_buffer = [0u8; 4_096];
//     let mut tx_buffer = [0u8; 4_096];

//     // Scratch buffers used by the MqttClient internally.
//     let mut mqtt_recv_buf = [0u8; 512];
//     let mut mqtt_send_buf = [0u8; 512];

//     let broker_endpoint = IpEndpoint::new(
//         IpAddress::v4(
//             MQTT_BROKER_IP[0],
//             MQTT_BROKER_IP[1],
//             MQTT_BROKER_IP[2],
//             MQTT_BROKER_IP[3],
//         ),
//         MQTT_PORT,
//     );

//     loop {
//         // ----------------------------------------------------------------
//         // 1. Open a TCP connection to the broker.
//         // ----------------------------------------------------------------
//         let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
//         socket.set_timeout(Some(Duration::from_secs(30)));

//         info!("Connecting TCP to {:?}:{}...", MQTT_BROKER_IP, MQTT_PORT);
//         if let Err(e) = socket.connect(broker_endpoint).await {
//             error!("TCP connect failed: {:?}", e);
//             Timer::after(Duration::from_secs(5)).await;
//             continue; // socket is dropped here, buffers become available again
//         }
//         info!("TCP connection established");

//         // ----------------------------------------------------------------
//         // 2. Build the MQTT client configuration.
//         // ----------------------------------------------------------------
//         let mut config: ClientConfig<'_, 5, CountingRng> =
//             ClientConfig::new(MqttVersion::MQTTv5, CountingRng(20_000));
//         config.add_client_id(MQTT_CLIENT_ID);
//         config.add_max_subscribe_qos(rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS0);
//         config.max_packet_size = 512;

//         // ----------------------------------------------------------------
//         // 3. Create the MQTT client and perform the CONNECT handshake.
//         // ----------------------------------------------------------------
//         let mut client = MqttClient::<_, 5, _>::new(
//             socket,
//             &mut mqtt_send_buf,
//             512,
//             &mut mqtt_recv_buf,
//             512,
//             config,
//         );

//         match client.connect_to_broker().await {
//             Ok(_) => info!("MQTT CONNECT accepted by broker"),
//             Err(ReasonCode::NetworkError) => {
//                 error!("MQTT broker unreachable (network error)");
//                 Timer::after(Duration::from_secs(5)).await;
//                 continue;
//             }
//             Err(code) => {
//                 error!("MQTT CONNECT rejected, reason code: {:?}", code);
//                 Timer::after(Duration::from_secs(5)).await;
//                 continue;
//             }
//         }

//         // ----------------------------------------------------------------
//         // 4. Subscribe to the configured topic.
//         // ----------------------------------------------------------------
//         match client.subscribe_to_topic(MQTT_TOPIC).await {
//             Ok(_) => info!("Subscribed to '{}'", MQTT_TOPIC),
//             Err(e) => {
//                 error!("SUBSCRIBE failed: {:?}", e);
//                 continue;
//             }
//         }

//         // ----------------------------------------------------------------
//         // 5. Event loop – receive and print incoming messages.
//         // ----------------------------------------------------------------
//         info!("Listening for MQTT messages...");
//         loop {
//             match client.receive_message().await {
//                 Ok((topic, payload)) => {
//                     let text = core::str::from_utf8(payload).unwrap_or("<binary payload>");
//                     info!("[{}] {}", topic, text);
//                 }
//                 Err(ReasonCode::NetworkError) => {
//                     warn!("MQTT connection lost, reconnecting...");
//                     break;
//                 }
//                 Err(code) => {
//                     error!("MQTT receive error, reason code: {:?}", code);
//                     break;
//                 }
//             }
//         }

//         // Brief pause before the next connection attempt.
//         Timer::after(Duration::from_secs(5)).await;
//     }
// }
