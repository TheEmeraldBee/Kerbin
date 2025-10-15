use lsp_types::*;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use tokio::io::AsyncWrite;

pub mod jsonrpc;
pub use jsonrpc::*;

pub mod client;
pub use client::*;

pub mod uriext;
pub use uriext::*;

pub mod facade;
pub use facade::*;

// State to track LSP data
#[derive(Default)]
struct AnalyzerState {
    diagnostics: HashMap<String, Vec<Diagnostic>>,
    progress_tokens: HashSet<String>,
}

impl AnalyzerState {
    fn is_indexing(&self) -> bool {
        !self.progress_tokens.is_empty()
    }
}

#[tokio::main]
async fn main() {
    let mut state = AnalyzerState::default();
    let mut client: LspClient<_, AnalyzerState> =
        LspClient::spawned("rust-analyzer", vec![]).await.unwrap();

    // Setup event handlers
    setup_handlers(&mut client);

    println!("=== Initializing Rust Analyzer ===\n");

    // Initialize
    let workspace = "/home/brightonlcox/Programming/rust/tools/kerbin/";
    let init_id = client
        .init(Uri::file_path(workspace).unwrap())
        .await
        .unwrap();
    client
        .wait_for_response::<InitializeResult>(init_id, &mut state)
        .await
        .unwrap();
    client.notification("initialized", json!({})).await.unwrap();

    println!("[✓] Initialized\n");

    // Open files
    println!("=== Opening Files ===\n");
    let files = vec![
        format!("{}/kerbin/src/main.rs", workspace),
        format!("{}/plugins/kerbin-lsp/src/client.rs", workspace),
        format!("{}/plugins/kerbin-lsp/src/facade.rs", workspace),
    ];

    for file in &files {
        client.open(file).await.unwrap();
        println!("[✓] Opened {}", file);
    }

    // Wait for indexing to complete
    println!("\n=== Waiting for Indexing ===\n");
    let start = std::time::Instant::now();
    while state.is_indexing() || start.elapsed().as_secs() < 3 {
        client.process_events(&mut state);
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        if start.elapsed().as_secs() > 10 {
            break;
        }
    }
    println!("[✓] Indexing complete\n");

    // Analyze the first file
    let target_file = &files[0];
    let target_uri = Uri::file_path(target_file).unwrap();

    println!("=== Analytics for {} ===\n", target_file);

    // Get document symbols
    if let Ok(Some(symbols)) = client
        .document_symbols(target_uri.clone(), &mut state)
        .await
    {
        print_symbols(&symbols);
    }

    // Get hover info at a specific position
    let hover_pos = Position {
        line: 10,
        character: 10,
    };
    if let Ok(Some(hover)) = client
        .hover(target_uri.clone(), hover_pos, &mut state)
        .await
    {
        print_hover(&hover, hover_pos);
    }

    // Get code lenses
    if let Ok(Some(lenses)) = client.code_lens(target_uri.clone(), &mut state).await {
        println!("\n[Code Lenses]");
        println!("  Found {} code lenses", lenses.len());
    }

    // Get completion at a position
    let completion_pos = Position {
        line: 5,
        character: 0,
    };
    if let Ok(Some(completions)) = client
        .completion(target_uri.clone(), completion_pos, &mut state)
        .await
    {
        print_completions(&completions);
    }

    // Get definition
    let def_pos = Position {
        line: 15,
        character: 10,
    };
    if let Ok(Some(definition)) = client
        .definition(target_uri.clone(), def_pos, &mut state)
        .await
    {
        print_definition(&definition, def_pos);
    }

    // Get references
    let ref_pos = Position {
        line: 10,
        character: 10,
    };
    if let Ok(Some(references)) = client
        .references(target_uri.clone(), ref_pos, &mut state)
        .await
    {
        println!(
            "\n[References] at line {}, col {}",
            ref_pos.line + 1,
            ref_pos.character + 1
        );
        println!("  Found {} references", references.len());
        for (i, loc) in references.iter().take(5).enumerate() {
            println!(
                "  {}. {}:{}:{}",
                i + 1,
                loc.uri.to_string(),
                loc.range.start.line + 1,
                loc.range.start.character + 1
            );
        }
    }

    // Print diagnostics
    print_diagnostics(&state);

    println!("\n=== Analysis Complete ===");
}

fn setup_handlers(client: &mut LspClient<impl AsyncWrite + Unpin + Send + 'static, AnalyzerState>) {
    // Track progress
    client.on_notification("$/progress", |state, msg| {
        if let JsonRpcMessage::Notification(notif) = msg {
            if let Some(token) = notif.params.get("token").and_then(|t| t.as_str()) {
                if let Some(value) = notif.params.get("value") {
                    if let Some(kind) = value.get("kind").and_then(|k| k.as_str()) {
                        match kind {
                            "begin" => {
                                state.progress_tokens.insert(token.to_string());
                                if let Some(title) = value.get("title").and_then(|t| t.as_str()) {
                                    println!("  [Progress] {}", title);
                                }
                            }
                            "end" => {
                                state.progress_tokens.remove(token);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    });

    // Capture diagnostics
    client.on_notification("textDocument/publishDiagnostics", |state, msg| {
        if let JsonRpcMessage::Notification(notif) = msg {
            if let Ok(params) =
                serde_json::from_value::<PublishDiagnosticsParams>(notif.params.clone())
            {
                state
                    .diagnostics
                    .insert(params.uri.to_string(), params.diagnostics);
            }
        }
    });
}

fn print_symbols(symbols: &DocumentSymbolResponse) {
    println!("[Document Symbols]");
    match symbols {
        DocumentSymbolResponse::Flat(flat) => {
            println!("  Found {} symbols", flat.len());
        }
        DocumentSymbolResponse::Nested(nested) => {
            println!("  Found {} top-level symbols\n", nested.len());
            for symbol in nested.iter().take(10) {
                println!("  • {} ({:?})", symbol.name, symbol.kind);
                if let Some(children) = &symbol.children {
                    for child in children.iter().take(3) {
                        println!("    └─ {} ({:?})", child.name, child.kind);
                    }
                }
            }
        }
    }
}

fn print_hover(hover: &Hover, pos: Position) {
    println!(
        "\n[Hover] at line {}, col {}",
        pos.line + 1,
        pos.character + 1
    );
    match &hover.contents {
        HoverContents::Scalar(content) => match content {
            MarkedString::String(s) => println!("  {}", s),
            MarkedString::LanguageString(ls) => println!("  [{}] {}", ls.language, ls.value),
        },
        HoverContents::Array(contents) => {
            for content in contents.iter().take(3) {
                match content {
                    MarkedString::String(s) => println!("  {}", s),
                    MarkedString::LanguageString(ls) => {
                        println!("  [{}] {}", ls.language, ls.value)
                    }
                }
            }
        }
        HoverContents::Markup(markup) => {
            let preview = markup.value.lines().take(5).collect::<Vec<_>>().join("\n");
            println!("  {}", preview);
        }
    }
}

fn print_completions(completions: &CompletionResponse) {
    println!("\n[Completions]");
    match completions {
        CompletionResponse::Array(items) => {
            println!("  Found {} completion items", items.len());
            for (i, item) in items.iter().take(10).enumerate() {
                println!("  {}. {} ({:?})", i + 1, item.label, item.kind);
            }
        }
        CompletionResponse::List(list) => {
            println!("  Found {} completion items", list.items.len());
            for (i, item) in list.items.iter().take(10).enumerate() {
                println!("  {}. {} ({:?})", i + 1, item.label, item.kind);
            }
        }
    }
}

fn print_definition(definition: &GotoDefinitionResponse, pos: Position) {
    println!(
        "\n[Definition] for symbol at line {}, col {}",
        pos.line + 1,
        pos.character + 1
    );
    match definition {
        GotoDefinitionResponse::Scalar(loc) => {
            println!(
                "  → {}:{}:{}", 
                loc.uri.to_string(),
                loc.range.start.line + 1,
                loc.range.start.character + 1
            );
        }
        GotoDefinitionResponse::Array(locs) => {
            for (i, loc) in locs.iter().enumerate() {
                println!(
                    "  {}. {}:{}:{}",
                    i + 1,
                    loc.uri.to_string(),
                    loc.range.start.line + 1,
                    loc.range.start.character + 1
                );
            }
        }
        GotoDefinitionResponse::Link(links) => {
            for (i, link) in links.iter().enumerate() {
                println!("  {}. {}", i + 1, link.target_uri.to_string());
            }
        }
    }
}

fn print_diagnostics(state: &AnalyzerState) {
    println!("\n=== Diagnostics ===\n");

    if state.diagnostics.is_empty() {
        println!("  No diagnostics");
        return;
    }

    for (uri, diagnostics) in &state.diagnostics {
        if diagnostics.is_empty() {
            continue;
        }

        println!("File: {}", uri);

        let errors = diagnostics
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
            .count();
        let warnings = diagnostics
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
            .count();

        println!("  Errors: {}, Warnings: {}", errors, warnings);

        for diag in diagnostics.iter().take(3) {
            let severity = match diag.severity {
                Some(DiagnosticSeverity::ERROR) => "ERROR",
                Some(DiagnosticSeverity::WARNING) => "WARN",
                _ => "INFO",
            };
            println!(
                "  [{}] Line {}: {}",
                severity,
                diag.range.start.line + 1,
                diag.message
            );
        }

        if diagnostics.len() > 3 {
            println!("  ... and {} more", diagnostics.len() - 3);
        }
        println!();
    }
}
