use ascii_forge::prelude::*;
use rune::ContextError;
use rune::alloc::clone::TryClone;
use rune::compile::meta::Kind;
use rune::compile::{CompileVisitor, MetaError, MetaRef};
use rune::diagnostics::Diagnostic;
use rune::runtime::RuntimeContext;
use rune::sync::Arc;
use rune::{Any, Context, Diagnostics, Module, Source, Sources, Unit, Vm, runtime::VmError};
use stategine::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::{fs, path::Path};

use crate::plugin_libs::*;

use crate::*;

/// A compile visitor that collects functions with a specific attribute.
#[derive(Default)]
pub struct FunctionVisitor {
    functions: Vec<String>,
}

impl FunctionVisitor {
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

macro_rules! build_api {
    (
        $api_name:ident,
        $((
            $wrapper_name:ident,
            $type:ident,
            $type_field_name:ident
            $(
                , $func_name:ident ($($arg_name:ident: $arg_type:ty),*) -> $return_type:ty
            )*
            $(
                , :$extra_func_name:ident
            )*
        )),*
    ) => {
        $(
            #[derive(Any, TryClone)]
            struct $wrapper_name {
                inner: Rc<RefCell<Option<$type>>>,
            }

            impl $wrapper_name {
                $(
                    #[rune::function]
                    fn $func_name (&mut self, $($arg_name: $arg_type),*) -> $return_type {
                        self.inner.borrow_mut().as_mut().unwrap().$func_name($($arg_name),*)
                    }
                )*
            }
        )*

        #[derive(Any)]
        struct $api_name {
            $(
                #[rune(get)]
                $type_field_name: $wrapper_name
            ),*
        }

        impl $api_name {
            pub fn new(engine: &mut Engine) -> Self {
                $(
                    let $type_field_name = engine.take_state::<$type>();
                    let $type_field_name = $wrapper_name { inner: Rc::new(RefCell::new(Some($type_field_name))) };
                )*

                Self {
                    $($type_field_name),*
                }
            }

            pub fn finish_api(self, engine: &mut Engine) {
                $(
                    engine.state(self.$type_field_name.inner.take().unwrap());
                )*
            }

            pub fn module() -> Result<Module, ContextError> {
                let mut module = Module::new();

                module.ty::<$api_name>()?;

                $(

                    module.ty::<$wrapper_name>()?;
                    $(
                        // Register already known functions
                        module.function_meta($wrapper_name::$func_name)?;
                    )*
                    $(
                        // Register extra functions
                        module.function_meta($wrapper_name::$extra_func_name)?;
                    )*
                )*

                Ok(module)

            }

        }
    };
}

build_api! {
    Api,
    (CommandWrapper, Commands, commands, add(command: EditorCommand) -> ()),
    (ThemeWrapper, Theme, theme, register(name: String, style: EditorStyle) -> ()),
    (WindowWrapper, Window, window, :render)
}

impl WindowWrapper {
    #[rune::function]
    pub fn render(&mut self, x: u16, y: u16, text: String) {
        render!(self.inner.borrow_mut().as_mut().unwrap(), (x, y) => [ text ]);
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

        context.install(Api::module()?)?;

        let mut module = Module::new();

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
            runtime: Arc::try_new(context.runtime()?)?,
            context: Arc::try_new(context)?,
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

                let mut func_visitor = FunctionVisitor::default();

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

                let unit = Arc::try_new(unit?)?;

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
        let mut api = Api::new(engine);

        for plugin in &self.plugins {
            let mut vm = Vm::new(self.runtime.clone(), plugin.unit.clone());

            // Call the `render` function in the script, if it exists.
            if plugin.has_update {
                let res = vm.call(["update"], (&mut api,));

                if let Err(e) = res {
                    api.finish_api(engine);
                    return Err(e);
                }
            }
        }

        api.finish_api(engine);
        Ok(())
    }

    /// Runs the `load` hook for all loaded plugins.
    pub fn run_load_hooks(&self, engine: &mut Engine) -> Result<(), VmError> {
        let mut api = Api::new(engine);

        for plugin in &self.plugins {
            let mut vm = Vm::new(self.runtime.clone(), plugin.unit.clone());

            // Call the `render` function in the script, if it exists.
            if plugin.has_load {
                let res = vm.call(["load"], (&mut api,));

                if let Err(e) = res {
                    api.finish_api(engine);
                    return Err(e);
                }
            }
        }

        api.finish_api(engine);
        Ok(())
    }
}
