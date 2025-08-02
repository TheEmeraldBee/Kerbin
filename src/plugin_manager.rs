use ascii_forge::prelude::*;
use rune::alloc::clone::TryClone;
use rune::compile::meta::Kind;
use rune::compile::{CompileVisitor, FileSourceLoader, MetaError, MetaRef};
use rune::runtime::{Function, RuntimeContext};
use rune::sync::Arc;
use rune::{Any, Context, Diagnostics, Module, Source, Sources, Vm, runtime::VmError};
use rune::{ContextError, Options, Value};
use stategine::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::plugin_libs::*;

use crate::*;

/// A compile visitor that collects functions with a specific attribute.
#[derive(Default)]
pub struct FunctionVisitor {
    functions: Vec<String>,
}

#[derive(Any, Default)]
pub struct PluginConfig {
    values: HashMap<String, Value>,
}

impl PluginConfig {
    pub fn register(&mut self, name: String, value: Value) {
        self.values.insert(name, value);
    }

    pub fn get(&mut self, name: &str) -> Option<Value> {
        self.values.get(name).cloned()
    }
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

        $(,)?
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
    (WindowWrapper, Window, window, :render, :width, :height),
    (CommandWrapper, Commands, commands, add(command: EditorCommand) -> (), :add_all),
    (InputWrapper, InputConfig, input, register_input(modes: Vec<char>, sequence: Vec<String>, func: Function, desc: String) -> ()),
    (ThemeWrapper, Theme, theme, register(name: String, style: EditorStyle) -> (), get(name: &str) -> Option<EditorStyle>),
    (GrammarWrapper, GrammarManager, grammar, register_extension(ext: String, lang: String) -> ()),
    (ModeWrapper, Mode, mode, :get),
    (PluginConfigWrapper, PluginConfig, config, register(name: String, value: Value) -> (), get(name: &str) -> Option<Value>),
}

impl ModeWrapper {
    #[rune::function]
    fn get(&self) -> char {
        self.inner.borrow().as_ref().unwrap().0
    }
}

impl CommandWrapper {
    #[rune::function]
    pub fn add_all(&mut self, commands: Vec<EditorCommand>) {
        for command in commands {
            self.inner.borrow_mut().as_mut().unwrap().add(command);
        }
    }
}

impl WindowWrapper {
    #[rune::function]
    pub fn width(&self) -> u16 {
        self.inner.borrow().as_ref().unwrap().size().x
    }

    #[rune::function]
    pub fn height(&self) -> u16 {
        self.inner.borrow().as_ref().unwrap().size().y
    }

    #[rune::function]
    pub fn render(&mut self, x: u16, y: u16, text: String, style: Option<EditorStyle>) {
        render!(self.inner.borrow_mut().as_mut().unwrap(), (x, y) => [ StyledContent::new(style.unwrap_or_default().to_content_style(), text) ]);
    }
}

/// Represents a single loaded and compiled plugin.
pub struct Config {
    vm: Vm,
    has_load: bool,
    has_update: bool,
}

/// Manages all plugins, including loading and execution.
pub struct ConfigManager {
    config: Option<Config>,
    context: Arc<Context>,
    runtime: Arc<RuntimeContext>,
}

impl ConfigManager {
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

        let mut api_module = Api::module()?;

        api_module.ty::<EditorCommand>()?;
        api_module.ty::<EditorStyle>()?;

        context.install(api_module)?;

        Ok(context)
    }

    /// Creates a new PluginManager and defines the script API.
    pub fn new() -> Result<Self, anyhow::Error> {
        let context = Self::context()?;

        Ok(Self {
            config: None,
            runtime: Arc::try_new(context.runtime()?)?,
            context: Arc::try_new(context)?,
        })
    }

    /// Loads the main `config.rn` file from the config directory
    pub fn load_config(&mut self) -> Result<(), anyhow::Error> {
        let file_path = "config/config.rn";
        if std::fs::exists(file_path)? {
            let mut sources = Sources::new();
            sources.insert(Source::from_path(file_path)?)?;
            let mut diagnostics = Diagnostics::new();

            let mut func_visitor = FunctionVisitor::default();

            let mut source_loader = FileSourceLoader::new();

            let options = Options::default();

            let unit = rune::prepare(&mut sources)
                .with_options(&options)
                .with_context(&self.context)
                .with_diagnostics(&mut diagnostics)
                .with_visitor(&mut func_visitor)?
                .with_source_loader(&mut source_loader)
                .build();

            if !diagnostics.is_empty() {
                let mut out = rune::termcolor::Buffer::no_color();

                diagnostics.emit(&mut out, &sources)?;

                tracing::error!("\n{}", String::from_utf8(out.into_inner())?);
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

            let vm = Vm::new(self.runtime.clone(), unit);

            self.config = Some(Config {
                vm,
                has_load,
                has_update,
            });
        }
        Ok(())
    }

    /// Runs the `update` hook for the config.
    pub fn run_update_hook(&mut self, engine: &mut Engine) -> Result<(), VmError> {
        if let Some(conf) = &mut self.config {
            let mut api = Api::new(engine);

            // Call the `update` function in the script, if it exists.
            if conf.has_update {
                let res = conf.vm.call(["update"], (&mut api,));

                if let Err(e) = res {
                    api.finish_api(engine);
                    return Err(e);
                }
            }
            api.finish_api(engine);
        }
        Ok(())
    }

    /// Runs the `update` hook for the config.
    pub fn run_load_hook(&mut self, engine: &mut Engine) -> Result<(), VmError> {
        if let Some(conf) = &mut self.config {
            let mut api = Api::new(engine);

            // Call the `load` function in the script, if it exists.
            if conf.has_load {
                let res = conf.vm.call(["load"], (&mut api,));

                if let Err(e) = res {
                    api.finish_api(engine);
                    return Err(e);
                }
            }
            api.finish_api(engine);
        }
        Ok(())
    }

    pub fn run_function(
        engine: &mut Engine,
        function: Rc<Function>,
        extra_arg: Value,
    ) -> Result<Value, anyhow::Error> {
        let mut api = Api::new(engine);

        let res = function.call((&mut api, extra_arg));

        api.finish_api(engine);

        Ok(res?)
    }
}
