# Kerbin: The Space-Age Text Editor

![Screenshot](./assets/screenshot0.png)

**Command and Control • Ready for Take-Off**  

The ultimate editor for **ambitious** projects  
Engineered for **everyone** : Get it working out of the box  
Built for the **Mission Director** : Make it your own kind of **powerful**  
Kerbin is the stable launch pad for your unstable ideas  

# What is it?
Kerbin is a hyper customizable text editor written in pure rust.

## Why use it over ...?
Kerbin isn't hyper opinionated, so unlike tools like `helix` and `vim`,
Kerbin is built to be changed. You shouldn't just be using kerbin as it comes,
though it's built to do so with. You should install more modules (plugins), and
utilize the strong and customizable config system to build this for yourself!

## Unique Concepts
Kerbin is built with using commands as a tool. Whether those commands are built in,
like `bind` or `insert`, or they're from your shell, like `yazi` or `fzf`, kerbin
is built to be used with them. Internal commands are incredibly customizable, and
there are built in IPC systems that heavily encourage tool composition.

As well, kerbin uses a concept I call the `mode stack`. This allows you to build compositions
of modes, overwrite keybinds, or just add some commands to prefix. This allows for incredibly
powerful configuration. Like in the core config, you have the `MULTICURSOR` mode that allows
users to use many cursors with ease!

Modes are defined by users. There is only a default `n` (`NORMAL`) mode, and then you define all other modes.
Though many of these modes are built in for you in the default configurations.

Plugins aren't meant to be changed constantly. You should choose your plugins, build your editor with them in mind,
then use the command-based config to build your options and how you'd use the plugins.

For example, tree-sitter is built into the editor as a plugin, allowing for Native speeds, and then all config
comes from commands within the `.kb` files in your configuration

# Installation

Install `booster`, kerbin's installation and update manager
```bash
cargo install kerbin-booster 
```

Run the interactive installation system
```bash
booster install
```

Follow the instructions in `booster`
Finally, run `kerbin`

# First Time Usage
Type `:tutor` into your terminal after running `kerbin`
This will give you tutoring on how to use `kerbin`
