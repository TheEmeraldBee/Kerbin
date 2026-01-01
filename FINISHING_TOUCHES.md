# General Systems
- [ ] Add command to create dialogue with resulting command
  - This is a way for commands to ask for input from a prompt
  - This will make many more things possible, also simplify the UI engine.

  - Should look like `dialogue "o %1" "What File Would You Like To Open?"`
    - This would open a dialogue with the prompt, then run the command with that result
    - This should also allow for multiple dialogues for a command, meaning "o %1 %2 %3" could work, as long as all 3 dialogues are passed

# LSP
- [ ] Auto-import
- [ ] Goto Definition
- [ ] Find References
- [ ] Rename Symbol
- [ ] Auto-formatting
- [ ] Code Actions

- [ ] Lsp configuration setup

# Tree Sitter
- [ ] Locals Queries
- [ ] Rainbow Brackets?

# Performance
- [ ] Add caching for as much as possible, add Idle system checks

# UI Improvements
- [ ] LSP dedicated progress engine to highlight things

# Plugins
- [ ] Create a built-in file-selection menu like fzf is being used
  - This will create a better out of the box experience
- [ ] Re-write the tutor plugin from scratch with better checks

# Configuration
- [ ] Add more language grammars and LSPs to the default configuration

# Documentation
- [ ] Write plugin implementation examples
- [ ] Update **all** code documentation

# Comprehension
- [ ] Go through and simplify **all** systems, maybe rethink how some things work
