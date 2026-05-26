#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering};
use core::task::Waker;
use cortex_m::peripheral::syst::SystClkSource;
use cortex_m_rt::exception;
use embassy_time_driver::Driver;
use esp_mqtt_project::hello_task;

// ---------------------------------------------------------------------------
// SysTick-based Embassy time driver
// ---------------------------------------------------------------------------
//
// Fires at 1 kHz (tick-hz-1_000). LM3S6965EVB core @ 12 MHz → reload = 11_999.
//
// Cortex-M3 has no native AtomicU64, so the 64-bit counter is split into two
// AtomicU32 values.  The SysTick ISR and schedule_wake both run inside a
// critical section (CPSID/CPSIE) so plain static mut is safe for the waker.

static TICKS_LO: AtomicU32 = AtomicU32::new(0);
static TICKS_HI: AtomicU32 = AtomicU32::new(0);

// Single pending waker – embassy-time only needs one per executor.
static mut WAKER_SLOT: Option<(u64, Waker)> = None;

fn ticks_now() -> u64 {
    // Read hi/lo/hi to detect a carry between the two halves.
    loop {
        let hi = TICKS_HI.load(Ordering::Acquire);
        let lo = TICKS_LO.load(Ordering::Acquire);
        if TICKS_HI.load(Ordering::Acquire) == hi {
            return ((hi as u64) << 32) | lo as u64;
        }
    }
}

struct SysTickDriver;
unsafe impl Sync for SysTickDriver {}

embassy_time_driver::time_driver_impl!(static DRIVER: SysTickDriver = SysTickDriver);

impl Driver for SysTickDriver {
    fn now(&self) -> u64 {
        ticks_now()
    }

    fn schedule_wake(&self, at: u64, waker: &Waker) {
        if at <= self.now() {
            waker.wake_by_ref();
            return;
        }
        critical_section::with(|_| unsafe {
            *core::ptr::addr_of_mut!(WAKER_SLOT) = Some((at, waker.clone()));
        });
    }
}

#[exception]
fn SysTick() {
    let old_lo = TICKS_LO.fetch_add(1, Ordering::Release);
    if old_lo == u32::MAX {
        TICKS_HI.fetch_add(1, Ordering::Release);
    }
    let now = ticks_now();
    critical_section::with(|_| unsafe {
        let slot = core::ptr::addr_of_mut!(WAKER_SLOT);
        if let Some((at, _)) = &*slot {
            if now >= *at {
                if let Some((_, waker)) = (*slot).take() {
                    waker.wake();
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

// Registers the panic handler for QEMU builds (must be at the crate root).
extern crate panic_semihosting;

// ---------------------------------------------------------------------------
// Semihosting logger
// ---------------------------------------------------------------------------

struct SemihostingLogger;

impl log::Log for SemihostingLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        cortex_m_semihosting::hprintln!("[{}] {}", record.level(), record.args());
    }
    fn flush(&self) {}
}

static LOGGER: SemihostingLogger = SemihostingLogger;

#[embassy_executor::main]
async fn main(spawner: embassy_executor::Spawner) -> ! {
    log::set_logger(&LOGGER).ok();
    log::set_max_level(log::LevelFilter::Info);

    // Configure SysTick at 1 kHz (12 MHz / 12_000).
    let mut core = cortex_m::Peripherals::take().unwrap();
    core.SYST.set_clock_source(SystClkSource::Core);
    core.SYST.set_reload(11_999);
    core.SYST.clear_current();
    core.SYST.enable_counter();
    core.SYST.enable_interrupt();

    spawner.spawn(hello_task().expect("failed to spawn run task"));

    core::future::pending::<()>().await;
    unreachable!()
}
