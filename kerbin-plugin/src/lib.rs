#![allow(improper_ctypes_definitions)]

use ::libloading::Library;
use async_ffi::FfiFuture;

pub struct Plugin {
    lib: Library,
}

impl Plugin {
    pub fn load(path: &str) -> Self {
        unsafe {
            let lib = Library::new(path).expect("Library should be loadable");

            Self { lib }
        }
    }

    /// Will return none when the function doesn't exist
    pub fn call_func<I, R>(&self, symbol: &[u8], input: I) -> Option<R> {
        unsafe {
            let func = self.lib.get::<extern "C" fn(I) -> R>(symbol).unwrap();
            Some(func(input))
        }
    }

    pub async fn call_async_func<'a, I: 'static + Send + Sync, R: 'static + Send + Sync>(
        &'a self,
        symbol: &[u8],
        input: I,
    ) -> R {
        unsafe {
            let func = self
                .lib
                .get::<extern "C" fn(I) -> FfiFuture<R>>(symbol)
                .unwrap();

            func(input).await
        }
    }
}
