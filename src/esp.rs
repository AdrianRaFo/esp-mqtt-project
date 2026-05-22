extern crate alloc;

use alloc::string::String;
use embassy_time::{Duration, Timer};
use esp_radio::wifi::{AuthenticationMethod, Ssid, WifiController, sta::StationConfig};
use log::{error, info, warn};

#[embassy_executor::task]
pub async fn wifi_task(mut controller: WifiController<'static>, ssid: Ssid, password: String) -> ! {
    const MAX_BACKOFF: Duration = Duration::from_secs(15);
    let mut backoff: Duration = Duration::from_secs(5);

    let config = esp_radio::wifi::Config::Station(
        StationConfig::default()
            .with_ssid(ssid)
            .with_auth_method(AuthenticationMethod::Wpa3Personal)
            .with_password(password),
    );

    controller
        .set_config(&config)
        .expect("Failed to set WiFi config");

    loop {
        // Transition to Connecting state if we were in Disconnected or stuck here
        if !controller.is_connected() {
            info!("Attempting to connect to WiFi...");

            match controller.connect_async().await {
                Ok(_) => info!("WiFi connected."),
                Err(e) => {
                    error!("Failed to connect WiFi: {:?}", e);
                    Timer::after(backoff).await; // Wait before retrying starting/connecting
                    backoff = (backoff * 2).min(MAX_BACKOFF); // Exponential backoff for start failures
                    continue;
                }
            }
        }

        // Wait for the link to go down (disconnection event).
        if controller.wait_for_disconnect_async().await.is_ok() {
            warn!("Disconnected from WiFi");
            backoff = Duration::from_secs(5); // Reset backoff on disconnect
        }
    }
}
