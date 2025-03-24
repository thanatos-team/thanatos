use std::{
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use crate::system::System;

pub struct Clock {
    last_frame: Instant,
    delta: Duration,
}

static CLOCK: LazyLock<Mutex<Clock>> = LazyLock::new(|| {
    Mutex::new(Clock {
        last_frame: Instant::now(),
        delta: Duration::ZERO,
    })
});

impl Clock {
    fn get<T, F: FnOnce(&Self) -> T>(f: F) -> T {
        f(&CLOCK.lock().unwrap())
    }

    fn update<F: FnOnce(&mut Self)>(f: F) {
        f(&mut CLOCK.lock().unwrap())
    }

    pub fn delta() -> Duration {
        Self::get(|clock| clock.delta)
    }
}

impl System for Clock {
    fn on_frame_end() {
        Self::update(|clock| {
            let now = Instant::now();
            clock.delta = now - clock.last_frame;
            clock.last_frame = now;
        });
    }
}
