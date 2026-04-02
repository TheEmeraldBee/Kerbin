use kerbin_core::*;
use lsp_types::{
    DocumentFormattingParams, FormattingOptions, TextDocumentIdentifier, TextEdit,
    WorkDoneProgressParams,
};
use tokio::io::AsyncWriteExt;

use crate::{text_edit::apply_text_edits, FormatterKind, JsonRpcMessage, LspManager, OpenedFile};

pub struct FormatPending {
    pub request_id: i32,
}

#[derive(State, Default)]
pub struct FormatState {
    pub pending: Option<FormatPending>,
}

#[derive(Debug, Clone, Command)]
pub enum FormatCommand {
    #[command(drop_ident, name = "lsp-format")]
    Format,
}

#[async_trait::async_trait]
impl Command<State> for FormatCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            FormatCommand::Format => format_current_buffer(state).await,
        }
    }
}

pub async fn format_current_buffer(state: &mut State) -> bool {
    let mut bufs = state.lock_state::<Buffers>().await;
    let mut lsps = state.lock_state::<LspManager>().await;
    let Some(mut buf) = bufs.cur_text_buffer_mut().await else { return false; };

    let Some(file) = buf.get_state::<OpenedFile>().await else {
        return false;
    };

    let lang = file.lang.clone();
    let uri = file.uri.clone();

    let Some(fmt_config) = lsps.lang_info_map.get(&lang).and_then(|i| i.format.clone()) else {
        return false;
    };

    match fmt_config.kind {
        FormatterKind::Lsp => send_lsp_format_request(&mut buf, &mut lsps, &lang, uri).await,
        FormatterKind::External(cmd, args) => {
            send_external_format_request(&mut buf, &cmd, &args).await
        }
    }
}

pub(crate) async fn send_lsp_format_request(
    buf: &mut TextBuffer,
    lsps: &mut LspManager,
    lang: &str,
    uri: lsp_types::Uri,
) -> bool {
    let tab_size = buf.indent_style.tab_width() as u32;
    let insert_spaces = matches!(buf.indent_style, IndentStyle::Spaces(_));

    let client = match lsps.get_or_create_client(lang).await {
        Some(c) => c,
        None => return false,
    };

    let params = DocumentFormattingParams {
        text_document: TextDocumentIdentifier { uri },
        options: FormattingOptions {
            tab_size,
            insert_spaces,
            ..Default::default()
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };

    let Ok(request_id) = client.request("textDocument/formatting", params).await else {
        return false;
    };

    let mut fmt_state = buf.get_or_insert_state_mut(FormatState::default).await;
    fmt_state.pending = Some(FormatPending { request_id });

    true
}

pub(crate) async fn send_external_format_request(
    buf: &mut TextBuffer,
    cmd: &str,
    args: &[String],
) -> bool {
    use std::process::Stdio;

    let content = match buf.slice_to_string(0, buf.len()) {
        Some(s) => s,
        None => return false,
    };

    let mut child = match tokio::process::Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(content.as_bytes()).await;
    }

    let output = match child.wait_with_output().await {
        Ok(o) => o,
        Err(_) => return false,
    };

    if !output.status.success() {
        return false;
    }

    let formatted = match String::from_utf8(output.stdout) {
        Ok(s) => s,
        Err(_) => return false,
    };

    if formatted == content {
        return true;
    }

    let total_chars = buf.len_chars();
    buf.start_change_group();
    buf.action(Delete { byte: 0, len: total_chars });
    buf.action(Insert { byte: 0, content: formatted });
    buf.commit_change_group();

    true
}

pub async fn handle_format(state: &State, msg: &JsonRpcMessage) {
    let JsonRpcMessage::Response(response) = msg else {
        return;
    };

    let bufs = state.lock_state::<Buffers>().await;

    let mut buffer = None;
    for buf in &bufs.buffers {
        let buf_guard = buf.read().await;
        if let Some(text_buf) = buf_guard.downcast::<TextBuffer>()
            && let Some(fmt_state) = text_buf.get_state::<FormatState>().await
            && let Some(pending) = &fmt_state.pending
            && pending.request_id == response.id
        {
            buffer = Some(buf.clone());
            break;
        }
    }

    let Some(buf_arc) = buffer else {
        return;
    };

    drop(bufs);

    let mut buf_guard = buf_arc.write_owned().await;
    let Some(buf) = buf_guard.downcast_mut::<TextBuffer>() else { return; };

    if let Some(mut fmt_state) = buf.get_state_mut::<FormatState>().await {
        fmt_state.pending = None;
    }

    let Some(result) = &response.result else {
        return;
    };

    let edits: Vec<TextEdit> = match serde_json::from_value(result.clone()) {
        Ok(e) => e,
        Err(_) => return,
    };

    apply_text_edits(buf, edits);
}

