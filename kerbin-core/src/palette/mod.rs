#[derive(Default)]
pub struct CommandPaletteState {
    pub input: String,
    pub suggestions: Vec<String>,
}
