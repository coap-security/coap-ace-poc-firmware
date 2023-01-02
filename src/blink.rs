//! Module managing the LED functions of the demo board

use core::cell::Cell;

/// The collection of device LEDs, along with all it needs to run animations and return to an idle
/// state again.
///
/// All its operations work on mutable state, allowing LEDs to be accessed from different
/// components in a system. The actual workhorse implementations are all on [LedPins], on which
/// they require exclusive access; [core::cell::Cell]s in this struct are used to coordinate
/// access.
pub struct Leds {
    /// GPIO pins representing the LEDs. As these are temporarily handed off to an embassy task
    /// during identification, they are optional here. (The value being None indicates that the
    /// task is running.).
    pins: Cell<Option<LedPins>>,
    /// State that was last set. Mostly merely set in order to be readable again, but this is also
    /// where the identify task looks up which state to return the LEDs to.
    idle_state: Cell<u8>,
    /// Means to start a task that runs an animation
    spawner: embassy_executor::Spawner,
}

// Embassy tasks don't take too kindly to being generic, so we're using concrete types here.
//
// Repeating the pinout could be avoided by using &mut dyn OutputPin, clever TAIT, or just
// degrading the pins.
pub struct LedPins {
    pub l1: embassy_nrf::gpio::Output<'static, embassy_nrf::peripherals::P0_17>,
    pub l2: embassy_nrf::gpio::Output<'static, embassy_nrf::peripherals::P0_18>,
    pub l3: embassy_nrf::gpio::Output<'static, embassy_nrf::peripherals::P0_19>,
    pub l4: embassy_nrf::gpio::Output<'static, embassy_nrf::peripherals::P0_20>,
}

impl Leds {
    pub fn new(spawner: embassy_executor::Spawner, pins: LedPins) -> Self {
        Self {
            spawner,
            pins: Cell::new(Some(pins)),
            idle_state: Cell::new(0),
        }
    }

    /// Set the number of LEDs to be active when idle.
    pub fn set_idle(&self, level: u8) {
        self.idle_state.set(level);

        if let Some(mut pins) = self.pins.take() {
            pins.set_level(level);
            self.pins.set(Some(pins))
        }
    }

    /// Return the number of LEDs active when idle.
    pub fn idle(&self) -> u8 {
        self.idle_state.get()
    }

    /// Run some animation useful for visually identifying a device.
    ///
    /// If the animation is already running, this is a no-op.
    pub fn run_identify(&'static self) {
        // Discarding result: Either there's a slot free, or we're already identifying.
        //
        // Not trying to take the pins out first: If we did, we'd have to return them ourselves
        // (and a failed spawn doesn't return the token's parts).
        let _ = defmt::dbg!(self.spawner.spawn(identify(self)));
    }
}

impl LedPins {
    fn set_level(&mut self, level: u8) {
        use nrf52832_hal::prelude::OutputPin;
        // `<` rather than `>=`: Pins are active-low.
        self.l1.set_state((level < 1).into()).unwrap();
        self.l2.set_state((level < 2).into()).unwrap();
        self.l3.set_state((level < 3).into()).unwrap();
        self.l4.set_state((level < 4).into()).unwrap();
    }

    async fn identify(&mut self) {
        use embassy_time::Duration;
        use embassy_time::Timer;

        let pause = Duration::from_millis(50);

        self.l1.set_high();
        self.l2.set_high();
        self.l3.set_high();
        self.l4.set_high();

        // They're numbered line-wise, but we go circular
        let a = &mut self.l1;
        let b = &mut self.l2;
        let c = &mut self.l4;
        let d = &mut self.l3;

        for _ in 0..4 {
            a.set_low();
            Timer::after(pause).await;
            d.set_high();
            Timer::after(pause).await;
            b.set_low();
            Timer::after(pause).await;
            a.set_high();
            Timer::after(pause).await;
            c.set_low();
            Timer::after(pause).await;
            b.set_high();
            Timer::after(pause).await;
            d.set_low();
            Timer::after(pause).await;
            c.set_high();
            Timer::after(pause).await;
        }
    }
}

/// Task for configuring and blinking the board LEDs
#[embassy_executor::task]
async fn identify(leds: &'static Leds) {
    if let Some(mut pins) = leds.pins.take() {
        pins.identify().await;

        // Pins are not Sync, so we're in a single-threaded setup, which means that we can just set
        // the level without racing against what happens inside a concurrent set_idle as long as we
        // don't have an await point between get(), set_level() and returning the pins
        pins.set_level(leds.idle_state.get());
        leds.pins.set(Some(pins))
    }
}
