use kerbin_state_machine::Hook;

/// This runs after each plugin's init function is run
pub struct PostInit;
impl Hook for PostInit {
    fn parts(&self) -> Vec<String> {
        vec!["post_init".to_string()]
    }
}

/// This runs at the beginning of each frame
pub struct Update;
impl Hook for Update {
    fn parts(&self) -> Vec<String> {
        vec!["update".to_string()]
    }
}

/// This runs at the end of each frame
pub struct Render;
impl Hook for Render {
    fn parts(&self) -> Vec<String> {
        vec!["render".to_string()]
    }
}
