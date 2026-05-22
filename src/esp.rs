#![no_std]
#![cfg(feature = "esp")]

use embassy_net_driver::Driver;
use esp_radio::{
    Radio,
    wifi::{Configuration, Controller as WifiController},
};

/// Drives the WiFi connection state machine.
///
/// Handles initial connect and automatic reconnect after a disconnect.
#[embassy_executor::task]
pub async fn wifi_task(mut controller: WifiController<'static>) {
    loop {
        // If already connected, block until disconnected.
        if controller.is_connected() {
            info!("Connected to '{}'", SSID);
            controller.wait_for_disconnect().await;
            warn!("Disconnected from WiFi");
            Timer::after(Duration::from_millis(5_000)).await;
        }

        // Start the radio if it hasn't been started yet.
        let connected = match controller.is_connected() {
            true => true,
            false => {
                info!("Starting WiFi radio...");
                loop {
                    let result = controller.start_async().await;
                    if matches!(result, Ok(_)) {
                        info!("WiFi radio started");
                        break;
                    }
                    warn!("WiFi start failed: {:?}, retrying...", result);
                    Timer::after(Duration::from_millis(5_000)).await;
                }

                // Configure client credentials.
                let config = Configuration::Station {
                    ssid: SSID.try_into().unwrap(),
                    password: PASSWORD.try_into().unwrap(),
                    ..Default::default()
                };
                controller.set_configuration(&config).ok();

                info!("Connecting to '{}' ...", SSID);
                match controller.connect_async().await {
                    Ok(_) => true,
                    Err(e) => {
                        warn!("WiFi connect failed: {:?}", e);
                        Timer::after(Duration::from_millis(5_000)).await;
                        false
                    }
                }
            }
        };

        // If we are connected, wait for disconnect and repeat.
        if connected {
            controller.wait_for_disconnect().await;
            warn!("Disconnected from WiFi");
            Timer::after(Duration::from_millis(5_000)).await;
        }
    }
}

/// Runs the embassy-net network stack (processes packets, drives DHCP, etc.).
#[embassy_executor::task]
pub async fn net_task(runner: embassy_net::Runner<'static, WifiDevice<'static, WifiStaDevice>>) {
    runner.run().await
}
