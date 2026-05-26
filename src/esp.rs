use embassy_time::{Duration, Timer};
use esp_radio::wifi::{ WifiController,Interface};
use log::{error, info, warn};

#[embassy_executor::task]
pub async fn wifi_task(mut controller: WifiController<'static>) -> ! {
    const MAX_BACKOFF: Duration = Duration::from_secs(15);
    let mut backoff: Duration = Duration::from_secs(5);

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

#[embassy_executor::task]
pub async fn net_task(mut runner: embassy_net::Runner<'static, Interface<'static>>) -> ! {
    runner.run().await;
}