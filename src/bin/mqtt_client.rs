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

extern crate alloc;

use alloc::borrow::ToOwned;
use embassy_executor::Spawner;
use embassy_net::driver::Driver;
use embassy_net::{IpAddress, IpEndpoint, Stack, StackResources, tcp::TcpSocket};
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{clock::CpuClock, interrupt::software::SoftwareInterruptControl};
use esp_mqtt_project::esp;
use esp_radio::wifi::Ssid;
use log::{error, info, warn};
use rust_mqtt::{
    client::client::MqttClient,
    client::client_config::{ClientConfig, MqttVersion},
    packet::v5::reason_codes::ReasonCode,
    utils::rng_generator::CountingRng,
};
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

// ---------------------------------------------------------------------------
// Static storage required by embassy-net and esp-wifi
// ---------------------------------------------------------------------------

/// Heap for dynamic allocations (needed by rust-mqtt's AllocBuffer and other
/// crates that use `alloc`).
esp_alloc::heap_allocator!(size: 72 * 1024);

/// Socket resources for the embassy-net stack (3 concurrent sockets).
static STACK_RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();

// ---------------------------------------------------------------------------
// Configuration – adjust these to match your environment
// ---------------------------------------------------------------------------

/// WiFi SSID, read from the WIFI_SSID environment variable at compile time.
const SSID: Ssid = Ssid::from(env!("WIFI_SSID"));

/// WiFi password, read from the WIFI_PASSWORD environment variable at compile time.
const PASSWORD: String = env!("WIFI_PASSWORD").to_owned();

/// MQTT broker address.
const MQTT_BROKER: &str = env!("MQTT_BROKER");

/// MQTT broker port (1883 = plain TCP, 8883 = TLS).
const MQTT_PORT: u16 = env!("MQTT_PORT")
    .parse()
    .expect("MQTT_PORT must be a valid integer");

/// MQTT client identifier – must be unique per broker session.
const MQTT_CLIENT_ID: &str = env!("MQTT_CLIENT_ID");

/// Topic to subscribe to.  MQTT wildcards ('+', '#') are allowed.
const MQTT_TOPIC: &str = env!("MQTT_TOPIC");

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    // Initialise esp-hal.
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);

    // TIMG0 drives the esp-rtos / embassy time-driver.

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    info!("Embassy initialised");

    // Initialise the WiFi radio using esp-radio 0.18.

    let (mut wifi_controller, interfaces) =
        esp_radio::wifi::new(peripherals.WIFI, Default::default())
            .expect("Failed to initialize Wi-Fi controller");

    // Build the embassy-net stack with DHCP.
    let net_config = embassy_net::Config::dhcpv4(Default::default());
    let (stack, runner) = embassy_net::new(
        net_device,
        net_config,
        STACK_RESOURCES.init(StackResources::new()),
        /* random_seed */ 0xDEAD_BEEF_CAFE_BABEu64,
    );

    spawner
        .spawn(esp::wifi_task(wifi_controller, SSID, PASSWORD).expect("failed to spawn wifi task"));

    // Wait until the network link is up.
    info!("Waiting for WiFi link...");
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    // Wait for a DHCP-assigned address.
    info!("Waiting for IP address (DHCP)...");
    loop {
        if let Some(cfg) = stack.config_v4() {
            info!("IP address: {}", cfg.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    // Run the MQTT loop indefinitely (never returns under normal operation).
    mqtt_loop(stack).await
}
