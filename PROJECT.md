# Kerbin: The Space-Age Text Editor

![Screenshot](./assets/screenshot0.png)

**Command and Control • Ready for Take-Off**  

The ultimate editor for **ambitious** projects  
Engineered for **everyone** : Get it working out of the box  
Built for the **Mission Director** : Make it your own kind of **powerful**  
Kerbin is the stable launch pad for your unstable ideas  

---

# ✨ Showcase ✨

Here is a very crude basic demonstration of the editor as of Oct 20th of 2025.
This is a early build of the editor, so everything is still in progress, 
check back later for new demonstrations that are much better!

[![asciicast](https://asciinema.org/a/XkqyndsqkMIaNVDg4oTJX9I82.svg)](https://asciinema.org/a/XkqyndsqkMIaNVDg4oTJX9I82)

---

# 🚀 Installation 🚀

Install with:

#### Nushell
```nu
bash -c (curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/TheEmeraldBee/Kerbin/master/install.sh)
```

#### Zsh/Bash/Sh
```bash
bash -c "$(curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/TheEmeraldBee/Kerbin/master/install.sh)"
```

### Prerequisites
- **Rust** and **Cargo** (install from [rustup.rs](https://rustup.rs))
- **Git**

### Rebuild After Updates

Once installed, you can quickly rebuild with cached builds:

```bash
# Interactive rebuild (with confirmation)
kerbin-install --rebuild

# Fast rebuild (no prompts)
kerbin-install --rebuild --yes

# Clean rebuild (removes build cache)
kerbin-install --rebuild --clean

# Change version (fetches from git for updates as well)
kerbin-install --update
```

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
    - Kerbin's goal is to be usable by anyone, Vim, Kakoune, Emacs, etc.
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

# Thank You
- [helix](https://github.com/helix-editor/helix) : The default queries built into the config

---

# 🗺  Roadmap 🗺

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
- [x] Reimplement rendering engine to better work with extmarks, allowing for scrolling to work
    - Current Ideas:
        - Use a list of RenderEvents, that will persist throughout the frame, allowing rendering to better work
        - Store the byte start of each RenderLine type so I only have to look at the last state to know what to render for the new state
- [x] Implement File dirty systems to prevent exit without forcing
    - [x] Dirty Flag on Text Buffers
    - [x] QuitForce, CloseBufferForce, etc
- [x] Prevent overriding newer changes on file without w!
- [x] Implement Reload File Command which will reload the file from disk
    - Prevent reloading without forcing if dirty flag is set
- [x] Implement sending messages to the process using interprocess
file communication systems
    - This will be handled by the core editor CLI
    - The reason for this is so that using things like zellij with file managers
    like `fzf` or `yazi` within `zellij` or `tmux` will work to send items over the cli
- [x] Implement core CLI that can send commands over the file communication
    - This also needs a way of running the editor, passing commands, then wrapping the system back.
    Basically this should allow for sending commands to an existing editor, or start an editor with the commands
    - Also have a flag, `-q` that will run the commands, but append a ForceQuit command to the end
- [ ] Lsp Support using plugin system
    - Diagnostics
    - Hover
    - Autocompletions
- [ ] Copy/Paste Support (Registers)
    - [x] Ctrl-Shift-V / Cmd-V Paste Event Support
    - [ ] Clipboard Copy & Paste Commands
    - [x] Registers
- [ ] Keybinding System Reimagination **HIGHEST PRIORITY** (This one changes how a lot of systems need to work, better done earlier rather than later.)
    - [ ] Use similar system to hooks that allow for matching keybindings
        - Examples:
            - ctrl-(a|b) **matches second**
            - ctrl-a **matches first**
            - ctrl-\* **matches last**
        - This would make keybindings on their own much more powerful
        - This would make each keybind a valid "variable" in the keybinding
        - This would also apply to command templating, IE:
          ```toml
          [[keybind]]
          modes = ["n"]
          keys = ["\"", "*", "y"]
          commands = ["copy %1"] # Copy with the key-name (up, down, a, b, page-up, etc) to that register.
          desc = "Copy to register name"
          ```
    - [ ] Allow for `$(my-shell-expansion %0)` to use shell commands to translate things
          ```toml
          [[keybind]]
          modes = ["n"]
          keys = ["\"", "*", "y"]
          commands = ["copy $(%config_dir/translate-keybind-to-register.sh %1)"] # Copy with the key-name (up, down, a, b, page-up) (translated) to that register.
          desc = "Copy to register name (translated)"
          ```
        - This will allow for commands to be drastically more complex, while still being simple when wanted to be
        - Most importantly, these need to be able to fail, IE, when returning a bad return status, log it and don't run the command
- [ ] Command Templating (% based variables in commands)
    - Extend this to keybindings, allowing for keybindings to come from templates
    - Allow keybindings to be registered dynamically (kinda)
        - Lists of items stored in a template item would repeat that keybind system over and over
        - Examples:
          ```toml
          [[keybind]]
          modes = ["n"]
          keys = ["\"", "%used_registers", "p"]
          commands = ["paste %1"] # Paste the single value from %used_registers
          desc = "Pase used register"
          
          [[keybind]]
          modes = ["n"]
          keys = ["\"", "%used_registers", "p"]
          commands = ["paste %1"] # Paste the single value from %used_registers
          desc = "Pase used register (translated)"
          ```
        - In this example, we have the following that may become something like "-a-p and "-b-p, etc
- [ ] Editor Support for wrapped text
- [ ] Mouse Scrolling Support
    - Allow Mapping Scroll Wheel to a command (`scroll_up = "ml -1"`) or something
    This would allow for the most flexible system, and make mouse pretty strong
- [ ] Mouse Click & Drag Support (Commands to map actions onto bytes?)
    - This ones a doozey, as file rendering isn't static
    Maybe we can use rendering as a way to map a screen location
    onto a byte. Either way, this will be incredibly tricky

## Documentation & Refinement
- [x] Document core systems and sub modules
- [x] Go through systems and refactor code (More of this will need to be done)
    - Make everything more readable, and stop being afraid of adding more files :)
- [x] Write out Nix & Linux/Mac install scripts for making installation and updating easy
- [ ] Simplify acessing parts of the editor by adding more types that retrieve directly from states (like Chunk)
    - Add this for accessing the Current Buffer
    - Look into having this for something like CurrentBufferClient for LSP
        - Same for Tree-Sitter Grammars
- [ ] Write out main wiki for writing configuration and plugins

## Stability
- [x] Cursors don't render on newline chars
- [x] Cursor doesnt render on last character of file
- [x] Cursor action to delete at end of line causes crash
- [x] Outside of Zellij, a large number of characters
are rendered next to the location of the systems until they are replaced
probably an issue from how we setup the first buffers
- [x] Log Rendering and other types of rendering incorrectly block when they are empty
    - Requres update to ascii-forge 2.0.0
    - Update boxes from rendering to correctly fill states
- [ ] Scrolling inside of buffers with inline widgets is very broken.
We need to apply visual elements to the widgets to handle this system.
- [ ] There are a ton of minor performance issues that build up quickly
    - [ ] Cache more for the plugin's rendering
        - [ ] LSPs
        - [x] Tree-Sitter
    - [ ] Cache and store render-line differences
- [x] Tree-Sitter Auto Indent isn't quite right in implementation.
(See multiline list items in markdown)
