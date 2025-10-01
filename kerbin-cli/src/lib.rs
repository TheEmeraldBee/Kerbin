use std::collections::HashMap;
use std::env;

/// Help information for a single flag or argument.
#[derive(Debug, Clone)]
pub struct ArgumentHelp {
    pub name: String,
    pub short: Option<char>,
    pub long: Option<String>,
    pub description: String,
    pub takes_value: bool,
}

/// Help information for a subcommand.
#[derive(Debug, Clone)]
pub struct CommandHelp {
    pub name: String,
    pub description: String,
    pub help: Help,
}

/// Complete help information for a command or subcommand.
#[derive(Debug, Clone, Default)]
pub struct Help {
    pub description: Option<String>,
    pub arguments: Vec<ArgumentHelp>,
    pub subcommands: Vec<CommandHelp>,
}

/// The result of parsing the command line arguments.
#[derive(Debug, Clone)]
pub struct CliResult {
    pub name: String,
    pub flags: HashMap<String, Option<String>>,
    pub subcommand: Option<Box<CliResult>>,
    pub help: Option<Help>,
}

impl Help {
    /// Creates a new parsable command type
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a description to the command
    pub fn description(mut self, desc: impl ToString) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    // --- Flag Builder Methods (takes_value = false) ---

    /// Registers a boolean flag with only a short name
    pub fn flag_short(mut self, name: impl ToString, short: char, desc: impl ToString) -> Self {
        self.arguments.push(ArgumentHelp {
            name: name.to_string(),
            short: Some(short),
            long: None,
            description: desc.to_string(),
            takes_value: false,
        });
        self
    }

    /// Registers a boolean flag with only a long name
    pub fn flag_long(
        mut self,
        name: impl ToString,
        long: impl ToString,
        desc: impl ToString,
    ) -> Self {
        self.arguments.push(ArgumentHelp {
            name: name.to_string(),
            short: None,
            long: Some(long.to_string()),
            description: desc.to_string(),
            takes_value: false,
        });
        self
    }

    /// Registers a boolean flag with both a long and short name
    pub fn flag_full(
        mut self,
        name: impl ToString,
        short: char,
        long: impl ToString,
        desc: impl ToString,
    ) -> Self {
        self.arguments.push(ArgumentHelp {
            name: name.to_string(),
            short: Some(short),
            long: Some(long.to_string()),
            description: desc.to_string(),
            takes_value: false,
        });
        self
    }

    // --- Argument Builder Methods (takes_value = true) ---

    /// Registers an argument (flag that takes a value) with only a short name
    pub fn arg_short(mut self, name: impl ToString, short: char, desc: impl ToString) -> Self {
        self.arguments.push(ArgumentHelp {
            name: name.to_string(),
            short: Some(short),
            long: None,
            description: desc.to_string(),
            takes_value: true,
        });
        self
    }

    /// Registers an argument (flag that takes a value) with only a long name
    pub fn arg_long(
        mut self,
        name: impl ToString,
        long: impl ToString,
        desc: impl ToString,
    ) -> Self {
        self.arguments.push(ArgumentHelp {
            name: name.to_string(),
            short: None,
            long: Some(long.to_string()),
            description: desc.to_string(),
            takes_value: true,
        });
        self
    }

    /// Registers an argument (flag that takes a value) with both a long and short name
    pub fn arg_full(
        mut self,
        name: impl ToString,
        short: char,
        long: impl ToString,
        desc: impl ToString,
    ) -> Self {
        self.arguments.push(ArgumentHelp {
            name: name.to_string(),
            short: Some(short),
            long: Some(long.to_string()),
            description: desc.to_string(),
            takes_value: true,
        });
        self
    }

    /// Registers a subcommand
    pub fn subcommand(mut self, name: impl ToString, desc: impl ToString, help: Help) -> Self {
        self.subcommands.push(CommandHelp {
            name: name.to_string(),
            description: desc.to_string(),
            help,
        });
        self
    }

    /// Prints the help message to stdout.
    pub fn print(&self, program_name: &str, indent: usize) {
        let prefix = "  ".repeat(indent);
        let arg_indent = "  "; // Consistent indentation for arguments/commands

        println!("{}Usage: {} [OPTIONS] [COMMAND]", prefix, program_name);
        println!();

        if let Some(desc) = &self.description {
            println!("{}{}", prefix, desc);
            println!();
        }

        if !self.arguments.is_empty() {
            println!("{}Options:", prefix);
            for arg in &self.arguments {
                let mut arg_str = String::new();
                let mut parts = Vec::new();

                // Build the flag/arg string
                if let Some(short) = arg.short {
                    parts.push(format!("-{}", short));
                }
                if let Some(long) = &arg.long {
                    parts.push(format!("--{}", long));
                }

                arg_str = parts.join(", ");

                if arg_str.is_empty() {
                    // Fallback to name if no short/long defined (shouldn't happen with current builders)
                    arg_str = arg.name.clone();
                }

                if arg.takes_value {
                    arg_str.push_str(" <VALUE>");
                }

                // Print with fixed width formatting for alignment
                println!("{}{}{:30} {}", prefix, arg_indent, arg_str, arg.description);
            }
            println!();
        }

        if !self.subcommands.is_empty() {
            println!("{}Commands:", prefix);
            for sub in &self.subcommands {
                println!(
                    "{}{}{:30} {}",
                    prefix, arg_indent, sub.name, sub.description
                );
            }
            println!();
            println!(
                "{}Use '{} <COMMAND> --help' for more information on a specific command.",
                prefix, program_name
            );
        }
    }

    /// Retrieves the ArgumentHelp struct by short or long name, or None.
    pub fn find_argument(&self, flag: &str) -> Option<&ArgumentHelp> {
        self.arguments.iter().find(|arg_help| {
            // Check short flag
            if flag.len() == 1 {
                if let Some(short) = arg_help.short {
                    if flag.chars().next() == Some(short) {
                        return true;
                    }
                }
            }
            // Check long flag
            if let Some(long) = &arg_help.long {
                if flag == long {
                    return true;
                }
            }
            false
        })
    }

    /// Checks if a short flag takes a value. Used for combined short flags like `-abc`.
    pub fn short_flag_takes_value(&self, flag_char: char) -> bool {
        self.arguments
            .iter()
            .any(|arg| arg.short == Some(flag_char) && arg.takes_value)
    }
}

impl CliResult {
    /// Parse command line arguments with help information
    /// Returns Ok(CliResult) or an Err(String) which should be treated as a program exit.
    pub fn from_args_with_help(help: Help) -> Result<Self, String> {
        let args: Vec<String> = env::args().collect();
        let program_name = args
            .get(0)
            .and_then(|p| p.split('/').last())
            .and_then(|p| p.split('\\').last())
            .unwrap_or("program")
            .to_string();

        Self::parse_args(&args[1..], program_name.clone(), Some(help), &program_name)
    }

    fn parse_args(
        args: &[String],
        name: String,
        help: Option<Help>,
        full_command: &str,
    ) -> Result<Self, String> {
        let mut flags = HashMap::new();
        let mut subcommand = None;
        let mut i = 0;

        while i < args.len() {
            let arg = &args[i];

            if arg == "--help" || arg == "-h" {
                if let Some(h) = help {
                    h.print(full_command, 0);
                    // Return a special error to signal a clean exit after printing help
                    return Err("Help requested".to_string());
                }
            }

            if arg.starts_with("--") {
                // --- Long flag (e.g., --verbose or --count 10) ---
                let flag_name = arg.trim_start_matches("--").to_string();
                let arg_help = help.as_ref().and_then(|h| h.find_argument(&flag_name));

                // 1. Validate flag
                if arg_help.is_none() {
                    let error_msg = format!("error: unexpected argument '--{}' found", flag_name);
                    eprintln!("{}", error_msg);
                    eprintln!();
                    if let Some(h) = help {
                        h.print(full_command, 0);
                    }
                    return Err(error_msg);
                }
                let arg_help = arg_help.unwrap();

                // 2. Check for value
                if arg_help.takes_value {
                    if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                        // Found a value
                        flags.insert(flag_name, Some(args[i + 1].clone()));
                        i += 2;
                    } else {
                        // Missing a required value
                        let error_msg = format!(
                            "error: argument '--{}' requires a value but none was supplied",
                            flag_name
                        );
                        eprintln!("{}", error_msg);
                        eprintln!();
                        if let Some(h) = help {
                            h.print(full_command, 0);
                        }
                        return Err(error_msg);
                    }
                } else {
                    // Boolean flag (takes_value = false)
                    flags.insert(flag_name, None);
                    i += 1;
                }
            } else if arg.starts_with('-') && arg.len() > 1 {
                // --- Short flag(s) (e.g., -v or -f 10 or -abc) ---
                let flag_chars = arg.trim_start_matches('-');
                let mut is_value_found = false;

                // Check for single short flag that takes a value (e.g., -c 10)
                if flag_chars.len() == 1 {
                    let flag_char = flag_chars.chars().next().unwrap();
                    let flag_str = flag_char.to_string();
                    let arg_help = help.as_ref().and_then(|h| h.find_argument(&flag_str));

                    if let Some(h) = arg_help {
                        if h.takes_value {
                            if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                                flags.insert(h.name.clone(), Some(args[i + 1].clone()));
                                i += 2;
                                is_value_found = true;
                            } else {
                                // Missing required value
                                let error_msg = format!(
                                    "error: argument '-{}' requires a value but none was supplied",
                                    flag_char
                                );
                                eprintln!("{}", error_msg);
                                eprintln!();
                                if let Some(h) = help {
                                    h.print(full_command, 0);
                                }
                                return Err(error_msg);
                            }
                        }
                    }
                }

                if !is_value_found {
                    // Handle single boolean flag (-v) or combined boolean flags (-abc)
                    for c in flag_chars.chars() {
                        let flag_str = c.to_string();
                        let arg_help = help.as_ref().and_then(|h| h.find_argument(&flag_str));

                        // 1. Validate flag
                        if arg_help.is_none() {
                            let error_msg = format!("error: unexpected argument '-{}' found", c);
                            eprintln!("{}", error_msg);
                            eprintln!();
                            if let Some(h) = help {
                                h.print(full_command, 0);
                            }
                            return Err(error_msg);
                        }

                        // 2. Error if a flag in a combination requires a value (e.g., -avc where -v takes a value)
                        if help.as_ref().map_or(false, |h| h.short_flag_takes_value(c)) {
                            let error_msg = format!(
                                "error: flag '-{}' requires a value and cannot be combined with other short flags.",
                                c
                            );
                            eprintln!("{}", error_msg);
                            eprintln!();
                            if let Some(h) = help {
                                h.print(full_command, 0);
                            }
                            return Err(error_msg);
                        }

                        // Insert as boolean flag (None value) using its canonical name
                        if let Some(h) = arg_help {
                            flags.insert(h.name.clone(), None);
                        }
                    }
                    i += 1;
                }
            } else {
                // --- Not a flag, treat as subcommand ---
                let sub_name = arg.clone();
                let sub_args = &args[i + 1..];

                // 1. Validate subcommand
                let subcommand_help_entry = help
                    .as_ref()
                    .and_then(|h| h.subcommands.iter().find(|s| s.name == sub_name));

                if subcommand_help_entry.is_none() {
                    let error_msg = format!("error: unrecognized subcommand '{}'", sub_name);
                    eprintln!("{}", error_msg);
                    eprintln!();
                    if let Some(h) = help {
                        h.print(full_command, 0);
                    }
                    return Err(error_msg);
                }

                // 2. Recurse into subcommand
                let sub_help = subcommand_help_entry.map(|s| s.help.clone());
                let new_full_command = format!("{} {}", full_command, sub_name);

                // The recursive call handles the rest of the arguments
                match Self::parse_args(sub_args, sub_name, sub_help, &new_full_command) {
                    Ok(parsed_subcommand) => {
                        subcommand = Some(Box::new(parsed_subcommand));
                        break; // Stop processing arguments, as they were consumed by the subcommand
                    }
                    Err(e) => return Err(e), // Propagate error
                }
            }
        }

        Ok(CliResult {
            name,
            flags,
            subcommand,
            help,
        })
    }

    /// Get the name of the immediate subcommand, if any
    pub fn subcommand_name(&self) -> Option<&str> {
        self.subcommand.as_ref().map(|s| s.name.as_str())
    }

    /// Get a reference to the subcommand by name (searches recursively)
    pub fn get_subcommand(&self, name: &str) -> Option<&CliResult> {
        if let Some(ref sub) = self.subcommand {
            if sub.name == name {
                return Some(sub);
            }
            return sub.get_subcommand(name);
        }
        None
    }

    /// Check if a flag is present (regardless of whether it has a value).
    /// Use the flag's canonical name (e.g., "verbose" not "v").
    pub fn has_flag(&self, flag: &str) -> bool {
        self.flags.contains_key(flag)
    }

    /// Get a boolean flag (returns true if flag is present with no value).
    /// Use the flag's canonical name.
    pub fn get_bool(&self, flag: &str) -> bool {
        matches!(self.flags.get(flag), Some(None))
    }

    /// Get a flag's value as a string (returns None if flag not present or has no value).
    /// Use the flag's canonical name.
    pub fn get_value(&self, flag: &str) -> Option<&str> {
        self.flags.get(flag)?.as_deref()
    }

    /// Get a flag's value or a default if not present
    /// Use the flag's canonical name.
    pub fn get_value_or(&self, flag: &str, default: &str) -> String {
        self.get_value(flag)
            .map(|s| s.to_string())
            .unwrap_or_else(|| default.to_string())
    }

    /// Get all flag names (canonical names)
    pub fn flag_names(&self) -> Vec<&str> {
        self.flags.keys().map(|s| s.as_str()).collect()
    }

    /// Print help information if available
    pub fn print_help(&self) {
        if let Some(help) = &self.help {
            help.print(&self.name, 0);
        } else {
            println!("No help available for command: {}", self.name);
        }
    }

    /// Pretty print the parsed command structure
    pub fn print(&self, indent: usize) {
        let prefix = "  ".repeat(indent);
        println!("{}Command: {}", prefix, self.name);

        if !self.flags.is_empty() {
            println!("{}Flags:", prefix);
            let mut sorted_flags: Vec<_> = self.flags.iter().collect();
            sorted_flags.sort_by_key(|(k, _)| *k);

            for (key, value) in sorted_flags {
                match value {
                    Some(v) => println!("{}  {} = {:?}", prefix, key, v),
                    None => println!("{}  {} (boolean flag)", prefix, key),
                }
            }
        }

        if let Some(sub) = &self.subcommand {
            println!("{}Subcommand:", prefix);
            sub.print(indent + 1);
        }
    }
}
