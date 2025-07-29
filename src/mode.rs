use derive_more::*;

#[derive(Deref, DerefMut)]
/// This state holds the current mode of the editor.
/// Built in modes include 'c' (The Command Pallette), 'n' (The Default Mode of the Editor), and 'i' (Insert mode for inserting text).
pub struct Mode(pub char);

impl Default for Mode {
    fn default() -> Self {
        Self('n')
    }
}
