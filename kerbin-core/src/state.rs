use std::sync::{RwLock, atomic::AtomicBool};

use ascii_forge::prelude::*;

use crate::buffer::Buffers;

pub struct State {
    pub running: AtomicBool,

    pub buffers: RwLock<Buffers>,

    pub window: RwLock<Window>,
}

impl State {
    pub fn new(window: Window) -> Self {
        Self {
            running: AtomicBool::new(true),

            buffers: RwLock::new(Buffers::default()),

            window: RwLock::new(window),
        }
    }
}
