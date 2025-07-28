use ascii_forge::prelude::*;
use crokey::{Combiner, KeyCombination};

pub trait KeyCheckExt {
    fn combination(&self, combiner: &mut Combiner, combination: KeyCombination) -> bool;
}

impl KeyCheckExt for Window {
    fn combination(&self, combiner: &mut Combiner, combination: KeyCombination) -> bool {
        for event in self.events() {
            let Event::Key(k) = event else { continue };
            if let Some(c) = combiner.transform(*k) {
                if c == combination {
                    return true;
                }
            }
        }
        false
    }
}
