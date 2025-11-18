use ascii_forge::prelude::*;
use kerbin_input::{KeyTree, ParseError, Resolver, StepResult, UnresolvedKeyBind};
use std::{collections::HashMap, error::Error, fmt::Display, time::Duration};

use serde::*;

const CONFIG: &str = include_str!("keybindings.toml");

#[derive(Deserialize, Serialize)]
pub struct Config {
    #[serde(rename = "group")]
    groups: Vec<Group>,
    #[serde(rename = "keybind")]
    keybindings: Vec<Keybinding>,
}

#[derive(Deserialize, Serialize)]
pub struct Group {
    sequence: Vec<UnresolvedKeyBind>,

    #[serde(flatten)]
    metadata: Metadata,
}

#[derive(Deserialize, Serialize)]
pub struct Keybinding {
    sequence: Vec<UnresolvedKeyBind>,
    action: String,

    #[serde(flatten)]
    metadata: Metadata,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Metadata {
    desc: String,
}

impl Display for Metadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.desc)
    }
}

pub fn keybindings_to_tree(
    binds: Vec<Keybinding>,
    resolver: &Resolver,
) -> Result<KeyTree<String, Metadata>, ParseError> {
    let mut tree = KeyTree::default();

    for item in binds {
        tree.register(resolver, item.sequence, item.action, Some(item.metadata))?;
    }

    Ok(tree)
}

fn execute_command(_cmd: &str, _args: &[String]) -> Result<Vec<String>, ParseError> {
    Ok(vec!["x".to_string(), "y".to_string()])
}

fn main() -> Result<(), Box<dyn Error>> {
    let config: Config = toml::from_str(CONFIG).unwrap();

    let templates = HashMap::from([(
        "digits".to_string(),
        vec![
            "0".to_string(),
            "1".to_string(),
            "2".to_string(),
            "3".to_string(),
            "4".to_string(),
            "5".to_string(),
            "6".to_string(),
            "7".to_string(),
            "8".to_string(),
            "9".to_string(),
        ],
    )]);

    let resolver = Resolver::new(&templates, &execute_command);
    let mut tree = keybindings_to_tree(config.keybindings, &resolver).unwrap();

    let mut sub_status = String::new();
    let mut status = String::new();

    let mut key_string = String::new();

    let mut window = Window::init()?;
    handle_panics();

    loop {
        window.update(Duration::from_millis(500))?;
        window.keyboard().ok();

        for event in window.events() {
            let Event::Key(event) = event else { continue };

            key_string = format!("{event:?}");

            match tree.step(&resolver, event.code, event.modifiers)? {
                StepResult::Success(seq, a) => {
                    sub_status = "Finished".to_string();
                    if &a == "quit" {
                        return Ok(());
                    }

                    key_string = seq
                        .iter()
                        .map(|x| x.to_string())
                        .collect::<Vec<_>>()
                        .join(" -> ");

                    status = a;
                }
                StepResult::Step => {
                    sub_status = "Step".to_string();
                }
                StepResult::Reset => {
                    sub_status = "Reset".to_string();
                }
            };
        }

        render!(window, (0, 0) => [status.as_str().green(), " ".repeat(10), sub_status.as_str().green(), " ".repeat(10), key_string.as_str().blue()]);

        for (i, (key, meta)) in tree.collect_layer_metadata().unwrap().iter().enumerate() {
            render!(window, (0, i as u16 + 5) => [ key.to_string().cyan(), " : ", meta.as_ref().map(|x| x.to_string()).unwrap_or_default().green() ]);
        }
    }
}
