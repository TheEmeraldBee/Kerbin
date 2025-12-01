use crate::*;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Debug)]
pub struct DebounceEvent {
    pub events: Vec<String>,
    pub min_ms: u64,
    #[serde(default)]
    pub modes: Vec<char>,
    #[serde(default)]
    pub ignore_modes: Vec<char>,
    #[serde(default)]
    pub ignore_with_template: Vec<String>,
}

/// Stores debounce info, is None if not in mode where config requests it's tracked
/// Can be used by plugins to run commands or similar
#[derive(State, Default)]
pub struct Debounce {
    flag: bool,
    /// The time and mode of the debounce
    state: Option<(Instant, char)>,
    triggered_events: Vec<usize>,
}

impl Debounce {
    pub fn debounce_time(&self) -> Option<Duration> {
        self.state.map(|(x, _)| Instant::now().duration_since(x))
    }

    pub fn set_flag(&mut self, flag: bool) {
        self.flag = flag;
    }

    pub fn flag(&self) -> bool {
        self.flag
    }

    pub fn reset(&mut self, mode: char) {
        self.state = Some((Instant::now(), mode));
        self.triggered_events.clear();
    }

    pub fn clear(&mut self) {
        self.state = None;
        self.triggered_events.clear();
    }
}

pub async fn update_debounce(
    buffers: ResMut<Buffers>,
    plugin_config: Res<PluginConfig>,
    modes: Res<ModeStack>,
    command_registry: Res<CommandRegistry>,
    prefix_registry: Res<CommandPrefixRegistry>,
    command_sender: Res<CommandSender>,
) {
    get!(
        mut buffers,
        plugin_config,
        modes,
        command_registry,
        prefix_registry,
        command_sender
    );

    let mut buf = buffers.cur_buffer_mut().await;
    let mut debounce = buf.get_or_insert_state_mut(|| Debounce::default()).await;
    let current_mode = modes.get_mode();

    // Handle state transitions
    match (debounce.state, !buf.byte_changes.is_empty()) {
        (_, true) => {
            // Changes occurred - reset and wait for idle
            debounce.clear();
            debounce.set_flag(true);
            return;
        }
        (Some((_, mode)), false) if mode != current_mode => {
            // Mode changed - require new changes
            debounce.clear();
            debounce.set_flag(false);
            return;
        }
        (None, false) if debounce.flag() => {
            // Idle after changes - start timer
            debounce.reset(current_mode);
            return;
        }
        (None, _) => return, // No active debounce
        _ => {}              // Continue to check events
    }

    // Get config and check events
    let Some(Ok(events)) = plugin_config.get::<Vec<DebounceEvent>>("debounce_event") else {
        return;
    };

    let elapsed = Instant::now()
        .duration_since(debounce.state.unwrap().0)
        .as_millis();
    let engine = resolver_engine().await;

    for (i, event) in events.iter().enumerate() {
        // Skip if already triggered, not enough time passed, or mode/template conditions not met
        if debounce.triggered_events.contains(&i)
            || elapsed < event.min_ms as u128
            || event
                .ignore_with_template
                .iter()
                .any(|t| engine.has_template(t))
            || (!event.modes.is_empty() && !event.modes.iter().any(|m| modes.mode_on_stack(*m)))
            || event.ignore_modes.iter().any(|m| modes.mode_on_stack(*m))
        {
            continue;
        }

        // Mark as triggered and execute commands
        debounce.triggered_events.push(i);

        let resolver = engine.as_resolver();
        for cmd_str in &event.events {
            if let Some(cmd) = command_registry.parse_command(
                word_split(cmd_str),
                true,
                false,
                Some(&resolver),
                true,
                &prefix_registry,
                &modes,
            ) {
                command_sender.send(cmd).unwrap();
            }
        }
    }
}
