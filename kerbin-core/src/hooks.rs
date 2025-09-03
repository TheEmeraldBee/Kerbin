use kerbin_state_machine::{Hook, HookInfo, HookPathComponent};

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

/// This runs when rendering the filetype
pub struct RenderFiletype(pub HookInfo);

impl RenderFiletype {
    pub fn new(info: impl AsRef<str>) -> Self {
        let info = HookInfo::new(info.as_ref());

        Self(info)
    }
}

impl Hook for RenderFiletype {
    fn info(&self) -> HookInfo {
        let mut path = self.0.path.clone();
        path.insert(0, HookPathComponent::Path("render_filetype".to_string()));
        HookInfo {
            path,
            rank: self.0.rank,
        }
    }
}
