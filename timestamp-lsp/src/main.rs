use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Local, Utc};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

struct Backend {
    client: Client,
    docs: Mutex<HashMap<Url, String>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "timestamp-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "timestamp-lsp ready")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.docs
            .lock()
            .unwrap()
            .insert(params.text_document.uri, params.text_document.text);
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.pop() {
            self.docs
                .lock()
                .unwrap()
                .insert(params.text_document.uri, change.text);
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.docs.lock().unwrap().remove(&params.text_document.uri);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let pos = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;

        let text = {
            let docs = self.docs.lock().unwrap();
            match docs.get(&uri) {
                Some(t) => t.clone(),
                None => return Ok(None),
            }
        };

        let line = match text.lines().nth(pos.line as usize) {
            Some(l) => l,
            None => return Ok(None),
        };

        let Some((token, start, end)) = digit_token_at(line, pos.character as usize) else {
            return Ok(None);
        };

        let Some(content) = format_hover(&token) else {
            return Ok(None);
        };

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: Some(Range {
                start: Position {
                    line: pos.line,
                    character: start as u32,
                },
                end: Position {
                    line: pos.line,
                    character: end as u32,
                },
            }),
        }))
    }
}

fn digit_token_at(line: &str, col: usize) -> Option<(String, usize, usize)> {
    let chars: Vec<char> = line.chars().collect();
    if col > chars.len() {
        return None;
    }
    let mut start = col;
    while start > 0 && chars[start - 1].is_ascii_digit() {
        start -= 1;
    }
    let mut end = col;
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }
    if start == end {
        return None;
    }
    Some((chars[start..end].iter().collect(), start, end))
}

fn format_hover(token: &str) -> Option<String> {
    let (utc, unit) = match token.len() {
        10 => {
            let n: i64 = token.parse().ok()?;
            (DateTime::<Utc>::from_timestamp(n, 0)?, "seconds")
        }
        13 => {
            let n: i64 = token.parse().ok()?;
            (DateTime::<Utc>::from_timestamp_millis(n)?, "milliseconds")
        }
        _ => return None,
    };
    let local: DateTime<Local> = utc.with_timezone(&Local);
    let offset = local.offset().to_string();
    Some(format!(
        "**Timestamp** (Unix {unit}): `{token}`\n\n\
         - **UTC**: `{}`\n\
         - **Local** (UTC{offset}): `{}`",
        utc.format("%Y-%m-%d %H:%M:%S"),
        local.format("%Y-%m-%d %H:%M:%S"),
    ))
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        docs: Mutex::new(HashMap::new()),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_10_digit_token_in_middle() {
        let line = "ts: 1716700000 end";
        let (tok, s, e) = digit_token_at(line, 8).unwrap();
        assert_eq!(tok, "1716700000");
        assert_eq!(s, 4);
        assert_eq!(e, 14);
    }

    #[test]
    fn detects_token_at_boundary() {
        let line = "1716700000";
        let (tok, _, _) = digit_token_at(line, 10).unwrap();
        assert_eq!(tok, "1716700000");
        let (tok, _, _) = digit_token_at(line, 0).unwrap();
        assert_eq!(tok, "1716700000");
    }

    #[test]
    fn no_digit_at_cursor() {
        assert!(digit_token_at("hello world", 3).is_none());
    }

    #[test]
    fn formats_seconds() {
        let s = format_hover("1716700000").unwrap();
        assert!(s.contains("Unix seconds"));
        assert!(s.contains("UTC"));
        assert!(s.contains("2024-05-26 06:26:40"));
    }

    #[test]
    fn formats_milliseconds() {
        let s = format_hover("1716700000000").unwrap();
        assert!(s.contains("Unix milliseconds"));
        assert!(s.contains("2024-05-26 06:26:40"));
    }

    #[test]
    fn rejects_other_lengths() {
        assert!(format_hover("123").is_none());
        assert!(format_hover("123456789").is_none());
        assert!(format_hover("12345678901").is_none());
    }
}
