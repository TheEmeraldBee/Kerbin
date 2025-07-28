use derive_more::*;

#[derive(Deref, DerefMut)]
pub struct Mode(pub char);

impl Default for Mode {
    fn default() -> Self {
        // Default to the normal mode (n)
        Self('n')
    }
}
