# Kerbin: The Space-Age Text Editor

![Screenshot](./assets/screenshot0.png)

**Command and Control • Ready for Take-Off**  

The ultimate editor for **ambitious** projects  
Engineered for **everyone** : Get it working out of the box  
Built for the **Mission Director** : Make it your own kind of **powerful**  
Kerbin is the stable launch pad for your unstable ideas  

---

# ✨ Showcase ✨

*There's nothing here yet!* 
Check back in a bit for full demonstrations
when the editor is more fleshed out (around mid October).

---

# 🚀 Installation 🚀

## Quick Install (Shell)

Install Kerbin with a single command:

```bash
curl -sSL https://raw.githubusercontent.com/TheEmeraldBee/Kerbin/master/install.sh | bash
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
```

The installer uses a persistent build directory at `~/.kerbin_build` to cache compilation artifacts, making rebuilds significantly faster.

---

## Nix Installation

### Understanding Nix Installation Options

Nix offers several ways to use Kerbin, each with different trade-offs. Here's what each approach looks like:

#### 1. Quick Run (No Installation)
```bash
nix run github:TheEmeraldBee/Kerbin
```
**What happens:**
- Nix downloads and builds Kerbin
- Runs it immediately
- Everything is stored in `/nix/store/` (immutable)
- Nothing persists in your home directory except config
- Next time you run this, Nix reuses the cached build

**Use case:** Try Kerbin without committing to installation

---

#### 2. NixOS System Configuration

Add Kerbin to your system packages in `/etc/nixos/configuration.nix`:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    kerbin.url = "github:TheEmeraldBee/Kerbin";
  };

  outputs = { self, nixpkgs, kerbin, ... }: {
    nixosConfigurations.yourSystem = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        {
          environment.systemPackages = [
            kerbin.packages.x86_64-linux.default
          ];
        }
      ];
    };
  };
}
```

Then rebuild:
```bash
sudo nixos-rebuild switch
```

**What happens:**
- Kerbin installed system-wide for all users
- Available after rebuilding your system
- Managed declaratively with your system config
- Atomic updates (old version kept until garbage collected)

**Use case:** NixOS users who want Kerbin as part of their system

---

#### 3. Home Manager Integration

Add to your Home Manager configuration (`~/.config/home-manager/home.nix`):

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    kerbin.url = "github:TheEmeraldBee/Kerbin";
    home-manager.url = "github:nix-community/home-manager";
  };

  outputs = { self, nixpkgs, kerbin, home-manager, ... }: {
    homeConfigurations.yourUser = home-manager.lib.homeManagerConfiguration {
      pkgs = nixpkgs.legacyPackages.x86_64-linux;
      modules = [
        {
          home.packages = [
            kerbin.packages.x86_64-linux.default
          ];
        }
      ];
    };
  };
}
```

Then switch:
```bash
home-manager switch
```

**What happens:**
- Kerbin installed per-user via Home Manager
- Part of your declarative home configuration
- Automatically available in PATH
- Can track specific versions or commits

**Use case:** Managing your entire user environment declaratively

### Nix vs Shell Installer Comparison

| Aspect | Shell Installer | Nix |
|--------|----------------|-----|
| **Location** | `~/.kerbin/` | `/nix/store/` |
| **Requires sudo** | No | No |
| **Build cache** | `~/.kerbin/build/` | `/nix/store/` (shared) |
| **Updates** | `kerbin-install -r -u` | `nix profile upgrade` |
| **Rollback** | Manual | `nix profile rollback` |
| **Disk space** | One cache per user | Shared across system |
| **Config location** | You choose | `~/.config/kerbin/` (XDG) |
| **Reproducible** | No (depends on system) | Yes (fully reproducible) |

---

### Hybrid Approach: Best of Both Worlds

You can use **Nix for the toolchain** but **shell installer for flexibility**:

```bash
# Use Nix-provided tools to run the shell installer
nix run github:TheEmeraldBee/Kerbin#install

# This gives you:
# - Nix's reproducible cargo/rustc/git
# - Shell installer's flexibility and caching at ~/.kerbin/
# - Fast rebuilds with kerbin-install --rebuild
```

After this initial setup:
```bash
# Fast rebuild (uses ~/.kerbin/build/ cache)
~/.kerbin/bin/kerbin-install --rebuild

# Update and rebuild
~/.kerbin/bin/kerbin-install --rebuild --update

# Add to PATH for convenience
echo 'export PATH="$HOME/.kerbin/bin:$PATH"' >> ~/.bashrc
```

---

### Which Installation Method Should I Use?

- **Just trying it?** → `nix run github:TheEmeraldBee/Kerbin`
- **Daily use, want it always available?** → `nix profile install`
- **NixOS user?** → Add to `configuration.nix` or `home.nix`
- **Need fast rebuilds for config changes?** → Shell installer
- **Want both reproducibility AND flexibility?** → Hybrid approach (Nix + shell installer)

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
    - This also needs a way of running the editor, passing commands, then wrapping the system back
    Basically this should allow for sending commands to an existing editor, or start an editor with the commands
    - Also have a flag, `-q` that will run the commands, but append a ForceQuit command to the end
- [ ] Lsp Support using plugin system
- [ ] Mouse Scrolling Support
    - Allow Mapping Scroll Wheel to a command (scroll_up = "ml -1") or something
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
- [x] Tree-Sitter Auto Indent isn't quite right in implementation.
(See multiline list items in markdown)
