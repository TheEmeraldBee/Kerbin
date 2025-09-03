use kerbin_state_machine::{Hook, HookInfo};

/// This runs after each plugin's init function is run
pub struct PostInit;
impl Hook for PostInit {
    fn info(&self) -> HookInfo {
        HookInfo::new("post_init")
    }
}

/// This runs at the beginning of each frame
pub struct Update;
impl Hook for Update {
    fn info(&self) -> HookInfo {
        HookInfo::new("update")
    }
}

/// This runs at the end of each frame
pub struct Render;
impl Hook for Render {
    fn info(&self) -> HookInfo {
        HookInfo::new("render")
    }
}
