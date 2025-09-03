# Kerbin
Kerbin is a [Helix](https://helix-editor.com/) and [Neovim](https://neovim.io/) inspired editor written
with the goal of easy, fast, and effective extensibility, on top of good roots.

# Showcase
There's nothing here yet! Check back in a little bit when the editor is a bit more fleshed out (around the 10th or 12th of September)

# Comparisons
## What makes it different from **Helix**?
Unlike Helix, **Kerbin** Has plugins. They are written using pure rust using ffi. Plugins will be able to be written, installed, and 
worked with as simply as calling a shell command and writing some code.

## What makes it similar to **Helix**?
Similarly to Helix, **Kerbin** uses TOML to configure. This tops everything else on the market, as rust code isn't required for about 90% of use cases.
**Kerbin** also comes with major default plugins and configuration, with a similar goal of helix to not be neccissary to add plugins for many users.

## What makes it different from **Neovim**?
Unlike Neovim, **Kerbin's** Plugins are written in rust, making them faster, memory safe\*, and be better interopped with the editor itself.
This allows for writing cpu "heavy" plugins easily, as well as hooking into the render engine incredibly easily

## What makes it similar to **Neovim**?
Similarly to neovim, the whole idea is that the editor is functional, but built for extensibility. Meaning you can install the editor, and it is quite lightweight,
but installing plugins is what makes the editor whole.

### But This Contradicts The Statement Before?
Not quite, as plugins are coming directly from you're config, some of the major plugins (LSPs, TreeSitter, Vim/Helix/Kakoune Bindings) are Plugins that are managed
by the editor itself, and come preadded to the default config, making it much more "out of the box".

# Development Progress
Currently, development is going very strongly. Most of the major pieces of the editor are implemented and functional. The editor in it's current state is installable,
and usable if you don't care about lsp support. Of course there will be bugs, as with all **alpha** software, but I am very active with issues, so if you find something
please reach out and we should be able to help. If you're more curious about next steps, look below at the progress checklist (may be slightly out of date)

## Progress Checklist (for v0.1.0)
Below is the current project checklist. It is being implemented in order in the list, but it is subject to change if I feel I've missed an important piece
This checklist is for when I feel the project will be ready for use in my own environment
- [x] Basic Editor Functionality (insertions, deletions, etc.)
- [x] Selection Support
- [x] Multicursor Support
- [x] TreeSitter Rendering (Currently In the Core, not a plugin (which i want it to be), requires Hooks to be done)
- [ ] Full Adjustment to using Layouts, then allowing passage of specific chunks for rendering. (Chunk system parameters)
- [x] Plugin Hooks (Replacing rendering systems, Adjusting how things render/work, adding new render calls to the statusline, etc)
- [ ] Lsp Support
