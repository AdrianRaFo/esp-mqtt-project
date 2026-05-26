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

extern crate alloc;

use alloc::borrow::ToOwned;
use embassy_executor::Spawner;
use embassy_net::StackResources;

use esp_backtrace as _;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{clock::CpuClock, interrupt::software::SoftwareInterruptControl};
use esp_mqtt_project::mqtt_task;
use esp_mqtt_project::{MqttConfig, esp};
use esp_radio::wifi::AuthenticationMethod;
use esp_radio::wifi::ControllerConfig;
use esp_radio::wifi::sta::StationConfig;
use log::info;

use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

// ---------------------------------------------------------------------------
// Static storage required by embassy-net and esp-wifi
// ---------------------------------------------------------------------------

/// Socket resources for the embassy-net stack (3 concurrent sockets).
static STACK_RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();

// ---------------------------------------------------------------------------
// Configuration – adjust these to match your environment
// ---------------------------------------------------------------------------

/// WiFi SSID, read from the WIFI_SSID environment variable at compile time.
const SSID: &str = env!("WIFI_SSID");

/// WiFi password, read from the WIFI_PASSWORD environment variable at compile time.
const PASSWORD: &str = env!("WIFI_PASSWORD");

/// MQTT broker address.
const MQTT_BROKER_IP: &str = env!("MQTT_BROKER_IP");

/// MQTT broker port (1883 = plain TCP, 8883 = TLS).
const MQTT_PORT: &str = env!("MQTT_PORT");

/// Topic to subscribe to.  MQTT wildcards ('+', '#') are allowed.
const MQTT_TOPIC: &str = env!("MQTT_TOPIC");

/// MQTT client identifier – must be unique per broker session.
const MQTT_CLIENT_ID: &str = env!("MQTT_CLIENT_ID");

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) {
    // Initialise esp-hal.
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // TIMG0 drives the esp-rtos / embassy time-driver.

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    info!("Embassy initialised");

    // Initialise the WiFi radio using esp-radio 0.18.

    let wifi_config = esp_radio::wifi::Config::Station(
        StationConfig::default()
            .with_ssid(SSID)
            .with_auth_method(AuthenticationMethod::Wpa3Personal)
            .with_password(PASSWORD.to_owned()),
    );

    let (wifi_controller, interfaces) = esp_radio::wifi::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(wifi_config),
    )
    .expect("Failed to initialize Wi-Fi controller");

    let net_config = embassy_net::Config::dhcpv4(Default::default());

    spawner.spawn(esp::wifi_task(wifi_controller).expect("failed to spawn wifi task"));

    // Build the embassy-net stack with DHCP.
    let (stack, runner) = embassy_net::new(
        interfaces.station,
        net_config,
        STACK_RESOURCES.init(StackResources::new()),
        0xDEAD_BEEF_CAFE_BABEu64,
    );

    spawner.spawn(esp::net_task(runner).expect("failed to spawn mqtt task"));

    stack.wait_config_up().await;

    let mqtt_config = MqttConfig {
        broker: MQTT_BROKER_IP
            .parse()
            .expect("MQTT_BROKER must be a valid IP address"),
        port: MQTT_PORT
            .parse()
            .expect("MQTT_PORT must be a valid integer"),
        topic: MQTT_TOPIC,
        client_id: MQTT_CLIENT_ID,
    };

    // Run the MQTT loop indefinitely (never returns under normal operation).
    spawner.spawn(mqtt_task(stack, mqtt_config).expect("failed to spawn mqtt task"))
}
