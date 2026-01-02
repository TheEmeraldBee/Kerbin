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

/// This runs after Update, but before render, so that chunks can be stateful
pub struct ChunkRegister;
impl Hook for ChunkRegister {
    fn info(&self) -> HookInfo {
        HookInfo::new("chunk_register")
    }
}

/// This runs right before Render
pub struct PreRender;
impl Hook for PreRender {
    fn info(&self) -> HookInfo {
        HookInfo::new("pre_render")
    }
}

/// This runs right before building RenderLines
pub struct PreLines;
impl Hook for PreLines {
    fn info(&self) -> HookInfo {
        HookInfo::new("pre_lines")
    }
}

/// This runs at the end of each frame
pub struct Render;
impl Hook for Render {
    fn info(&self) -> HookInfo {
        HookInfo::new("render")
    }
}

/// This runs after update each frame and should be used to register chunks by layouts
pub struct RenderChunks;
impl Hook for RenderChunks {
    fn info(&self) -> HookInfo {
        HookInfo::new("render_chunks")
    }
}

/// Runs before updating the buffer's lines and after creating chunks
pub struct UpdateFiletype(pub HookInfo);

impl UpdateFiletype {
    pub fn new(info: impl AsRef<str>) -> Self {
        let info = HookInfo::new(info.as_ref());

        Self(info)
    }
}

impl Hook for UpdateFiletype {
    fn info(&self) -> HookInfo {
        let mut path = self.0.path.clone();
        path.insert(0, HookPathComponent::Path("update_filetype".to_string()));
        HookInfo {
            path,
            rank: self.0.rank,
        }
    }
}

/// Runs right after updating the filetype
pub struct CreateRenderLines;
impl Hook for CreateRenderLines {
    fn info(&self) -> HookInfo {
        HookInfo::new("create_render_lines")
    }
}

/// Runs immediately after update
pub struct PostUpdate;
impl Hook for PostUpdate {
    fn info(&self) -> HookInfo {
        HookInfo::new("post_update")
    }
}

/// This state runs after all updates and should be used to clear states
pub struct UpdateCleanup;
impl Hook for UpdateCleanup {
    fn info(&self) -> HookInfo {
        HookInfo::new("update_cleanup")
    }
}
