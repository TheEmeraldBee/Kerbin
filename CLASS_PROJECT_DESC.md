# Project Proposal - CS3390R
**Project Github**: [Kerbin](https://github.com/TheEmeraldBee/Kerbin/tree/master)
I am, for this project, hoping to work on my personal text editor, Kerbin
which is a terminal based text editor similar to Nvim, Kakoune, and Helix
it's already in a pretty good state, but still has a bunch to work on to
get to a point I'm happy with. I started the project about 2 weeks before
the start of the semester, so it hasn't been being worked on for too long.
For bare bones, the current project contains TreeSitter (syntax highlighting),
Motions, Regex,
Plugins, Commands, Inter Process Communication, Multicursors, and way too much
more, so I'll go with saying
that my base piece of the project is a functional editor
For the goals of this project, I would like to have working LSP support, as well
as finish fixing any known bugs that exist. I anticipate these two pieces to take
about 20-30 hours combined, and many of the projects have taken 20-30 minutes,
so the requirements should be met for expectations
As some stretch goals, I would like to implement markdown rendering systems
so that working with markdown in the editor has color and organization that
makes working with it much easier. This alone should be about 40 hours, so
should be a lot of work for a stretch goal. My other stretch goal would be to
implement
line wrapping. It's pretty simple, but would be annoying enough to implement fully
that I
want to do it as a stretch goal, and I also wouldn't use it personally.

## Crates
- Ropey
    - Used for storing files within the editor, very fast insertions using a rope style of text storage
- Tokio
    - Async runtime for handling quick task scheduling and running
- Tracing
    - Used for logging to the .kerbin/kerbin.log file
- Ascii Forge
    - The terminal rendering engine I wrote, will be used for all terminal rendering
- LspTypes
    - The types used by the LSP specification

## Structs & Enums 
- LspClient
    - Client state and writer/reader storage for the lsp client
- Diagnostics
    - Stores results of diagnostic information
- OpenedFile
    - Tracks buffers that are open to prevent re-checks
- ProcessLspEventsCommand
    - A command to force checking of lsp events and dispatch Handlers
- HandlerEntry
    - A handler that describes how to handle a goal
- HandlerSet
    - A state that describes a bunch of handlers
- LspHandlerManager
    - Stores handlers for file-types as well as global handlers
- HoverInfo
    - Stores a Buffer's hover information (hoverdata, position, etc)
- HoverState
    - Stores hoverinfo and requests for Hovers on buffers
- HoverCommand
    - A list of commands able to be run by the editor for using hovers
- LangInfo
    - Info about how a language is defined in the editor
- LspManager
    - Storage for the language infos and extension map
- CompletionInfo
    - Stores information about current autocomplete request and items
- CompletionState
    - Stores the state of completion for a buffer
- CompletionCommand
    - A list of commands for interacting with autocomplete
- RequestInfo
    - Stores information about a pending request
- EventHandler
    - Type alias for a function that handles LSP events
- ClientFacade
    - Trait for simplifying client interaction
- UriExt
    - Trait for working with Uris

### Below all types are related to using JsonRpc requirements
- JsonRpcRequest
- JsonRpcNotification
- JsonRpcResponse
- JsonRpcServerRequest
- JsonRpcMessage

## Timeline
- Technical Specification
    - Most of this is already in the readme, but I'll have 
    a full file written out by the 7th of november
    - Partial Showing of the project could be done whenever, I recently implemented hover,
    still some bugs, but I will have it ready to be checked and shown by the 20th of November

# Final Writeup
I really enjoyed working on this part of kerbin because it taught me how to use and understand LSPs.
I found that working on this project took a lot more work than I had initially expected because of the expectations
that I had for the functionality required many changes in other locations.

My biggest difficulty came from just implementing the LspClient type as they are super complicated and pretty hard
to wrap your head around without just spending a couple hours messing around with them.

