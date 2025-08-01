use ascii_forge::prelude::*;
use ref_cast::RefCast;
use rune::compile::meta::Kind;
use rune::compile::{CompileVisitor, MetaError, MetaRef};
use rune::diagnostics::Diagnostic;
use rune::runtime::RuntimeContext;
use rune::{Any, Context, Diagnostics, Module, Source, Sources, Unit, Vm, runtime::VmError};
use stategine::prelude::*;
use std::{fs, path::Path, sync::Arc};

use crate::plugin_libs::*;

use crate::*;

/// A compile visitor that collects functions with a specific attribute.
pub struct FunctionVisitor {
    functions: Vec<String>,
}

impl FunctionVisitor {
    pub fn new() -> Self {
        Self {
            functions: Vec::default(),
        }
    }

    /// Convert visitor into test functions.
    pub fn into_functions(self) -> Vec<String> {
        self.functions
    }
}

impl CompileVisitor for FunctionVisitor {
    fn register_meta(&mut self, meta: MetaRef<'_>) -> Result<(), MetaError> {
        match &meta.kind {
            Kind::Function {
                is_test, is_bench, ..
            } if !(*is_test || *is_bench) => {}
            _ => return Ok(()),
        };

        self.functions.push(
            meta.item
                .base_name()
                .expect("Function should have name")
                .to_string(),
        );
        Ok(())
    }
}

macro_rules! wrap_type {
    ($new_name:ident, $type:ty, $($name:ident ($($arg_name:ident: $arg_ty:ty),*) -> $ret_type:ty)*) => {
        #[derive(Any, RefCast)]
        #[repr(transparent)]
        struct $new_name {
            inner: $type,
        }

        impl $new_name {
            pub fn new(inner: &mut $type) -> &mut Self {
                Self::ref_cast_mut(inner)
            }

            $(
            #[rune::function]
            pub fn $name (&mut self, $($arg_name: $arg_ty),*) -> $ret_type {
                self.inner.$name($($arg_name),*)
            }

            )*
        }
    };
}

wrap_type! {
    WrappedTheme,
    Theme,
    register(name: String, style: EditorStyle) -> ()
    get(name: &str) -> Option<EditorStyle>
}

#[derive(Any, RefCast)]
#[repr(transparent)]
struct PluginApi {
    engine: Engine,
}

impl PluginApi {
    pub fn new(engine: &mut Engine) -> &mut Self {
        Self::ref_cast_mut(engine)
    }

    #[rune::function]
    pub fn command(&mut self, command: EditorCommand) {
        self.engine.get_state_mut::<Commands>().add(command);
    }

    #[rune::function]
    pub fn render(&mut self, x: u16, y: u16, text: String) {
        let mut window = self.engine.get_state_mut::<Window>();
        render!(window, (x, y) => [ text ]);
    }
}

/// Represents a single loaded and compiled plugin.
pub struct Plugin {
    unit: Arc<Unit>,
    has_load: bool,
    has_update: bool,
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

        module.ty::<PluginApi>()?;
        module.function_meta(PluginApi::render)?;
        module.function_meta(PluginApi::command)?;

        module.ty::<EditorCommand>()?;

        module.ty::<EditorStyle>()?;

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

                let mut func_visitor = FunctionVisitor::new();

                let unit = rune::prepare(&mut sources)
                    .with_context(&self.context)
                    .with_diagnostics(&mut diagnostics)
                    .with_visitor(&mut func_visitor)?
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

                let mut has_update = false;
                let mut has_load = false;

                for function in func_visitor.into_functions() {
                    if has_load && has_update {
                        break;
                    }

                    if function.as_str() == "load" {
                        has_load = true;
                    } else if function.as_str() == "update" {
                        has_update = true;
                    }
                }

                self.plugins.push(Plugin {
                    unit,
                    has_load,
                    has_update,
                });
            }
        }
        Ok(())
    }

    /// Runs the `update` hook for all loaded plugins.
    pub fn run_update_hooks(&self, engine: &mut Engine) -> Result<(), VmError> {
        for plugin in &self.plugins {
            let mut vm = Vm::new(self.runtime.clone(), plugin.unit.clone());

            let api = PluginApi::new(engine);

            // Call the `render` function in the script, if it exists.
            if plugin.has_update {
                vm.call(["update"], (api,))?;
            }
        }
        Ok(())
    }

    /// Runs the `load` hook for all loaded plugins.
    pub fn run_load_hooks(&self, engine: &mut Engine) -> Result<(), VmError> {
        for plugin in &self.plugins {
            let mut vm = Vm::new(self.runtime.clone(), plugin.unit.clone());

            let api = PluginApi::new(engine);

            // Call the `render` function in the script, if it exists.
            if plugin.has_load {
                vm.call(["load"], (api,))?;
            }
        }
        Ok(())
    }
}
