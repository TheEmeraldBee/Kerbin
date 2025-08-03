use std::collections::HashMap;

use ascii_forge::window::{Attribute, Attributes, Color, ContentStyle, Stylize};
use rune::{Any, ContextError, Module, alloc::clone::TryClone};

/// The style that can be put on content.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Any, TryClone)]
#[rune(constructor)]
pub struct EditorStyle {
    /// The foreground color.
    #[rune(get, set, copy)]
    pub fg: Option<(u8, u8, u8)>,
    /// The background color.
    #[rune(get, set, copy)]
    pub bg: Option<(u8, u8, u8)>,
    /// If the text should be underlined.
    #[rune(get, set, copy)]
    pub underline: Option<(u8, u8, u8)>,
    /// If the text should be bold.
    #[rune(get, set, copy)]
    pub bold: bool,
    /// If the text should be italic.
    #[rune(get, set, copy)]
    pub italic: bool,
}

impl EditorStyle {
    pub fn module() -> Result<Module, ContextError> {
        let mut module = Module::new();

        module.ty::<Self>()?;
        module.function_meta(Self::cloned)?;
        module.function_meta(Self::new__meta)?;
        module.function_meta(Self::fg__meta)?;
        module.function_meta(Self::bg__meta)?;
        module.function_meta(Self::underline__meta)?;
        module.function_meta(Self::bold__meta)?;
        module.function_meta(Self::italic__meta)?;

        Ok(module)
    }

    #[rune::function]
    pub fn cloned(&self) -> Self {
        self.clone()
    }

    #[rune::function(keep, path = Self::new)]
    pub fn new() -> Self {
        Self::default()
    }

    #[rune::function(keep)]
    pub fn fg(mut self, color: (u8, u8, u8)) -> Self {
        self.fg = Some(color);
        self
    }

    #[rune::function(keep)]
    pub fn bg(mut self, color: (u8, u8, u8)) -> Self {
        self.bg = Some(color);
        self
    }

    #[rune::function(keep)]
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    #[rune::function(keep)]
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    #[rune::function(keep)]
    pub fn underline(mut self, color: (u8, u8, u8)) -> Self {
        self.underline = Some(color);
        self
    }

    pub fn to_content_style(&self) -> ContentStyle {
        let mut style = ContentStyle {
            foreground_color: self.fg.map(|(r, g, b)| Color::Rgb { r, g, b }),
            background_color: self.bg.map(|(r, g, b)| Color::Rgb { r, g, b }),
            underline_color: None,
            attributes: Attributes::none(),
        };

        if let Some(underline) = &self.underline {
            style.underlined();
            style = style.underline(Color::Rgb {
                r: underline.0,
                g: underline.1,
                b: underline.2,
            });
        }

        if self.italic {
            style = style.italic();
        }

        if self.bold {
            style = style.bold();
        }

        style
    }
}

#[derive(Any, Debug, Clone, Default, PartialEq)]
pub struct Theme {
    pub theme_map: HashMap<String, EditorStyle>,
}

impl Theme {
    /// Registers a theme with a given key to a style
    /// Used for theming backgrounds and stuff
    #[rune::function(keep)]
    pub fn register(&mut self, name: String, style: EditorStyle) {
        self.theme_map.insert(name, style);
    }

    /// Returns an optional theme value if one is set.
    /// Used for theming text to make it pretty in the user's editor
    #[rune::function(keep)]
    pub fn get(&self, name: &str) -> Option<EditorStyle> {
        self.theme_map.get(name).cloned()
    }
}
