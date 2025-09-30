# Kerbin

Kerbin is a powerful, extensible editor inspired by
[Helix](https://helix-editor.com/) and [Neovim](https://neovim.io/).
Built for ease of use, while keeping powerusers
in good hands with plugins and configurations
that could rival those of neovim.

---

# ✨ Showcase ✨

*There's nothing here yet!* 
Check back in a bit for full demonstrations
when the editor is more fleshed out (around mid October).

---

# 💡 Concepts 💡

## Core Concepts
- [Rust](https://www.rust-lang.org/) based plugin system
    - Install plugins using `cargo`
    - Write plugins using pure rust
    - Configuration can be used with rust when toml doesn't work (complicated systems)
- [Toml](https://toml.io/en/) based configuration
    - Programming languages aren't the best way of writing
    - Toml is an incredible configuration language for most use cases
        - The remaining can be part of a plugin's init function
- Flexible Bindings
    - Kerbin's goal is to be usable by anyone, Vim, Kakoune, Emacs, Visual, etc.
    Allowing anyone to use the plugin ecosystem, no matter they're keybindings!
    - Allows for anyone to use kerbin, without worrying about necissarily relearning how
    they write code
- [TreeSitter](https://tree-sitter.github.io/tree-sitter/) and [Lsp](https://microsoft.github.io/language-server-protocol/) Drives Modern Editing
    - Kerbin is fully on board with tree-sitter and lsps being the future.
    - Many people will need tree-sitter and lsps to work with code (me included), but some won't.
    For This reason, lsp and tree-sitter are plugins maintained within the core editor, but aren't used
    unless defined within the rust plugins. Though the default configuration uses these plugins, people
    can remove them without losing too much functionality.

## Unique Concepts
- Stack based modal editing
    - Although I haven't seen this within any other editor, I find it very intuitive. It allows for any version of modal editing by defining a
    mode stack. This mode stack allows for many modes to be active at once, for example, `NORMAL -> CURSOR -> INSERT` within the default bindings,
    defines that we are in a base mode of NORMAL, then we are in Multicursor mode, which prefixes commands with an `aa` command (apply next command to all cursors),
    that finally, within the insert command writes the text to all of the cursors at the same time
    - This allows for drastically simpler bindings for many shared binding types, as well as allowing users to create even more powerful editing systems quickly without
    sacrificing time to write out the same bindings over and over again.
- Kitty Terminal Support
    - I personally love what kitty is doing with the terminal, making it more accessible to everyone, and making leaving the terminal less and less necissary.
    Supporting the rendering protocols is a big part of making the core experience good, as allowing for things like image and markdown rendering is pretty awesome.
    - This however should never be forced. Although this will be within the core rendering engine, all functionality will only be implemented using plugins.

---

# 🗺️ Roadmap 🗺️

## Core Development
- [x] Basic Editor Functionality (insertions, deletions, etc.)
- [x] Selection Support
- [x] Multicursor Support
- [x] TreeSitter Rendering
- [x] Full Adjustment to using Layouts, then allowing passage of
  specific chunks for rendering. (Chunk system parameters)
- [x] Plugin Hooks (Replacing rendering systems, Adjusting how
  things render/work, adding new render calls to the statusline, etc.)
- [x] TreeSitter Indentation Queries
- [x] Adjust rendering system to instead use Extmarks (similarly to neovim)
    - Will make plugins that add highlighting or ghost text much easier
- [ ] Reimplement rendering engine to better work with extmarks, allowing for scrolling to work
    - Current Ideas:
        - Use a list of RenderEvents, that will persist throughout the frame, allowing rendering to better work
        - Store the byte start of each RenderLine type so I only have to look at the last state to know what to render for the new state
- [ ] Implement File dirty systems to prevent exit without forcing
    - [ ] Dirty Flag on Text Buffers
    - [ ] QuitForce, CloseBufferForce, etc
- [ ] Prevent overriding newer changes on file without w!
- [ ] Implement Reload File Command which will reload the file from disk
    - Prevent reloading without forcing if dirty flag is set
- [ ] Implement sending messages to the process using interprocess
  file communication systems
- [ ] Write out CLI systems for handling command-line arguments
  within plugins and handling custom systems
- [ ] Implement Kitty Rendering Protocol Support
    - Most likely within the chunk rendering to support
    Images and Text Scaling (Mainly for markdown)
- [ ] Lsp Support using plugin system
- [ ] Mouse Scrolling Support
    - Allow Mapping Scroll Wheel to a command (scroll up = "ml -1") or something
    This would allow for the most flexible system, and make mouse pretty strong
- [ ] Mouse Click & Drag Support (Commands to map actions onto bytes?)
    - This ones a doozey, as file rendering isn't static
    Maybe we can use rendering as a way to map a screen location
    onto a byte. Either way, this will be incredibly tricky

## Documentation & Refinement
- [x] Document core systems and sub modules
- [x] Go through systems and refactor code (More of this will need to be done)
    - Make everything more readable, and stop being afraid of adding more files :)
- [ ] Write out main wiki for writing configuration and plugins

## Stability
- [x] Cursors don't render on newline chars
- [ ] Outside of Zellij, a large number of characters
are rendered next to the location of the systems until they are replaced
probably an issue from how we setup the first buffers
- [ ] Tree-Sitter Auto Indent isn't quite right in implementation
(See multiline list items in markdown)
