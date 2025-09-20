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

# 💡 Key Comparisons 💡

## How Kerbin Differs from **Helix**

Unlike Helix, **Kerbin** embraces a robust plugin system. Plugins
are written in pure Rust, leveraging FFI for unparalleled speed, memory
safety, and deep integration with the editor. This design allows you
to write, install, and manage plugins with the simplicity of a shell
command, empowering you to extend Kerbin's capabilities effortlessly.

## How Kerbin is Similar to **Helix**

**Kerbin** shares Helix's philosophy on configuration, utilizing TOML for
straightforward setup. This approach eliminates the need for Rust code in
most use cases, prioritizing user convenience. Furthermore,
Kerbin comes pre-packaged with essential default plugins and
configurations, mirroring Helix's goal of providing a rich,
out-of-the-box experience that many users won't need to customize
extensively.

---

## How Kerbin Differs from **Neovim**

A key differentiator from Neovim is **Kerbin's** plugin architecture.
By writing plugins in Rust, you're able to achieve superior performance, memory
safety\, and seamless interoperability with the editor's core. This
empowers developers to create CPU-intensive plugins and hook into the
rendering engine with remarkable ease, unlocking new possibilities for
custom functionality.

## How Kerbin is Similar to **Neovim**

Similar to Neovim, **Kerbin** is built on the premise that the editor
is fundamentally functional but designed for deep extensibility. While
lightweight on its own, the true power of Kerbin is unleashed through
its plugin system, allowing you to tailor it precisely to your workflow.

### Contradictory Statements?

This might seem to contradict the previous statement, but it doesn't.
While custom plugins are managed via your configuration, several core
plugins such as Language Server Protocol (LSP) support, TreeSitter,
and keybindings (Vim/Helix/Kakoune) are managed directly by the editor
and come pre-added to the default configuration. This ensures a rich
and functional experience right out of the box.

---

# 🗺️ Roadmap 🗺️

Below is the current project checklist. Items are being implemented in
order, but the list is subject to change if an important piece is
deemed missing or prioritized differently.

### Core Development
*   [x] Basic Editor Functionality (insertions, deletions, etc.)
*   [x] Selection Support
*   [x] Multicursor Support
*   [x] TreeSitter Rendering
*   [x] Full Adjustment to using Layouts, then allowing passage of
    specific chunks for rendering. (Chunk system parameters)
*   [x] Plugin Hooks (Replacing rendering systems, Adjusting how
    things render/work, adding new render calls to the statusline, etc.)
*   [x] TreeSitter Indentation Queries

### Documentation & Refinement
*   [x] Document core systems and sub modules
*   [ ] Go through systems and refactor code
*   [ ] Write out design document
*   [ ] Write out main wiki for writing configuration and plugins

### Stability & Enhancements
*   [ ] Fix major bugs
*   [ ] Implement sending messages to the process using interprocess
    file communication systems
*   [ ] Write out CLI systems for handling command-line arguments
    within plugins and handling custom systems
*   [ ] Lsp Support using plugin system
