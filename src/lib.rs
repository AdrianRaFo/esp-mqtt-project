#![no_std]

use embassy_time::{Duration, Timer};

#[embassy_executor::task]
pub async fn run() -> ! {
    log::info!("Embassy initialized!");

    loop {
        log::info!("Hello world!");
        Timer::after(Duration::from_secs(1)).await;
    }
}
