use ascii_forge::prelude::*;
use rune::diagnostics::Diagnostic;
use rune::runtime::RuntimeContext;
use rune::{Any, Context, Diagnostics, Module, Source, Sources, Unit, Vm, runtime::VmError};
use rune::{FromValue, Value};
use stategine::prelude::*;
use std::{fs, path::Path, sync::Arc};

use crate::commands::EditorCommand;

use crate::plugin_libs::*;

/// A wrapper for the API provided to plugins.
/// This struct gives scripts safe access to parts of the editor.
#[derive(Any)]
struct PluginApi {
    commands: Vec<EditorCommand>,
    draw_calls: Vec<(u16, u16, String)>,
}

impl PluginApi {
    /// Draws text to the screen buffer at a specific location.
    #[rune::function]
    fn draw_text(&mut self, x: u16, y: u16, text: &str) {
        self.draw_calls.push((x, y, text.to_string()))
    }

    /// Dispatches an editor command
    #[rune::function]
    fn command(&mut self, command: Value) {
        let command = match EditorCommand::from_value(command) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Failed to add command due to: {e}");
                return;
            }
        };

        self.commands.push(command)
    }
}

/// Represents a single loaded and compiled plugin.
pub struct Plugin {
    unit: Arc<Unit>,
}

/// Manages all plugins, including loading and execution.
pub struct PluginManager {
    plugins: Vec<Plugin>,
    context: Arc<Context>,
    runtime: Arc<RuntimeContext>,
}

impl PluginManager {
    pub fn context() -> Result<Context, anyhow::Error> {
        let mut context = rune::Context::with_default_modules()?;
        context.install(rune_modules::fs::module(true)?)?;
        context.install(rune_modules::http::module(true)?)?;
        context.install(rune_modules::json::module(true)?)?;
        context.install(rune_modules::rand::module(true)?)?;
        context.install(rune_modules::time::module(true)?)?;
        context.install(rune_modules::toml::module(true)?)?;
        context.install(rune_modules::base64::module(true)?)?;
        context.install(rune_modules::signal::module(true)?)?;
        context.install(rune_modules::process::module(true)?)?;

        context.install(chrono::module()?)?;

        let mut module = Module::new();

        // Install our custom API
        module.ty::<PluginApi>()?;
        module.function_meta(PluginApi::draw_text)?;
        module.function_meta(PluginApi::command)?;

        context.install(module)?;

        Ok(context)
    }

    /// Creates a new PluginManager and defines the script API.
    pub fn new() -> Result<Self, anyhow::Error> {
        let context = Self::context()?;

        Ok(Self {
            plugins: Vec::new(),
            runtime: Arc::new(context.runtime()?),
            context: Arc::new(context),
        })
    }

    /// Loads all `.rune` scripts from the `plugins/` directory.
    pub fn load_plugins(&mut self) -> Result<(), anyhow::Error> {
        let dir = Path::new("plugins");
        if !dir.exists() {
            fs::create_dir(dir)?;
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("rune") {
                let mut sources = Sources::new();
                sources.insert(Source::from_path(&path)?)?;
                let mut diagnostics = Diagnostics::new();

                let unit = rune::prepare(&mut sources)
                    .with_context(&self.context)
                    .with_diagnostics(&mut diagnostics)
                    .build();

                if !diagnostics.is_empty() {
                    for diagnostic in diagnostics.into_diagnostics() {
                        match diagnostic {
                            Diagnostic::Fatal(f) => tracing::error!("{f}"),
                            Diagnostic::Warning(w) => tracing::warn!("{w}"),
                            Diagnostic::RuntimeWarning(rw) => tracing::warn!("{rw}"),
                            d => tracing::warn!("{d:#?}"),
                        }
                    }
                }

                let unit = Arc::new(unit?);
                self.plugins.push(Plugin { unit });
            }
        }
        Ok(())
    }

    /// Runs the `render` hook for all loaded plugins.
    pub fn run_render_hooks(
        &self,
        window: &mut Window,
        commands: &mut Commands,
    ) -> Result<(), VmError> {
        for plugin in &self.plugins {
            let mut vm = Vm::new(self.runtime.clone(), plugin.unit.clone());

            let mut api = PluginApi {
                commands: vec![],
                draw_calls: vec![],
            };

            // Call the `render` function in the script, if it exists.
            let result = vm.call(["render"], (&mut api,));

            // We ignore "missing function" errors to make the hook optional.
            if let Err(e) = result {
                return Err(e);
            }

            for (x, y, text) in api.draw_calls {
                render!(window, (x, y) => [ text ]);
            }

            for command in api.commands.into_iter() {
                commands.add(command);
            }
        }
        Ok(())
    }

    /// Runs the `load` hook for all loaded plugins.
    pub fn run_load_hooks(
        &self,
        window: &mut Window,
        commands: &mut Commands,
    ) -> Result<(), VmError> {
        for plugin in &self.plugins {
            let mut vm = Vm::new(self.runtime.clone(), plugin.unit.clone());

            let mut api = PluginApi {
                commands: vec![],
                draw_calls: vec![],
            };

            // Call the `render` function in the script, if it exists.
            let result = vm.call(["load"], (&mut api,));

            // We ignore "missing function" errors to make the hook optional.
            if let Err(e) = result {
                return Err(e);
            }

            for (x, y, text) in api.draw_calls {
                render!(window, (x, y) => [ text ]);
            }

            for command in api.commands.into_iter() {
                commands.add(command);
            }
        }
        Ok(())
    }
}

/// A stategine system to run plugin render hooks each frame.
pub fn run_plugin_render_hooks(
    manager: Res<PluginManager>,
    mut window: ResMut<Window>,
    mut commands: ResMut<Commands>,
) {
    if let Err(e) = manager.run_render_hooks(&mut window, &mut commands) {
        tracing::error!("Rune VM Error: {}", e);
    }
}

/// A stategine system to run plugin render hooks each frame.
pub fn run_plugin_load_hooks(
    manager: Res<PluginManager>,
    mut window: ResMut<Window>,
    mut commands: ResMut<Commands>,
) {
    if let Err(e) = manager.run_load_hooks(&mut window, &mut commands) {
        tracing::error!("Rune VM Error: {}", e);
    }
}
