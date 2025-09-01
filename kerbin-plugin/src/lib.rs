#![allow(improper_ctypes_definitions)]

use ::libloading::Library;

pub struct Plugin {
    pub lib: Library,
}

impl Plugin {
    pub fn load(path: &str) -> Self {
        unsafe {
            let lib = Library::new(path).expect("Library should be loadable");

            Self { lib }
        }
    }

    pub fn call_func<I, R>(&self, symbol: &[u8], input: I) -> R {
        unsafe {
            let func = self.lib.get::<extern "C" fn(I) -> R>(symbol).unwrap();
            func(input)
        }
    }
}
