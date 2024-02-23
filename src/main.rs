use std::collections::VecDeque;
use std::vec;

use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use simd_json as json;
use tracing_appender::rolling::Rotation;

mod kubectl_validate;
use kubectl_validate::KubectlValidateResponse;

use located_yaml::{Marker, Yaml, YamlElt};

use crate::kubectl_validate::DocumentResult;

#[derive(Debug)]
struct Backend {
    client: Client,
}

trait GetYamlMarker {
    fn get_marker(&self, segment: VecDeque<String>) -> Option<Marker>;
}

impl GetYamlMarker for Yaml {
    fn get_marker(&self, mut segments: VecDeque<String>) -> Option<Marker> {
        if segments.is_empty() {
            tracing::info!("Found last: {:?}", self.marker);
            return Some(self.marker);
        }
        match self {
            Yaml {
                yaml: YamlElt::Hash(ref current),
                ..
            } => {
                let next = current.get(&Yaml {
                    yaml: YamlElt::String(segments.pop_front().expect("Truste me, not emtpy")),
                    marker: Marker {
                        col: 0,
                        line: 0,
                        index: 0,
                    },
                })?;
                tracing::info!("Found next: {:?}", next);
                next.get_marker(segments)
            }
            _ => None,
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(true),
                        })),
                        ..TextDocumentSyncOptions::default()
                    },
                )),
                ..Default::default()
            },
        })
    }

    async fn did_save(&self, file: DidSaveTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file opened!")
            .await;

        let location = file.text_document.uri.clone();
        let command = vec!["-o", "json", location.path()];
        // TODO: locale kubectl-validate?
        let Ok(output) = tokio::process::Command::new("/Users/bruno.tavares/bin/kubectl-validate")
            .args(&command)
            .output()
            .await
        else {
            self.client
                .log_message(MessageType::ERROR, "failed to execute kubectl-validate")
                .await;
            return;
        };

        if output.status.success() {
            self.client
                .publish_diagnostics(file.text_document.uri.clone(), vec![], None)
                .await;
            self.client
                .log_message(
                    MessageType::INFO,
                    "kubectl-validate succeeded, skipping parsing.",
                )
                .await;
            return;
        }

        let Ok(payload) = String::from_utf8(output.stdout) else {
            self.client
                .log_message(MessageType::ERROR, "failed to read kubectl-validate")
                .await;
            return;
        };
        let mut buffer: Vec<_> = payload.bytes().collect();
        let Ok(results) = json::serde::from_slice::<KubectlValidateResponse>(&mut buffer) else {
            self.client
                .log_message(
                    MessageType::ERROR,
                    "failed to parse kubectl-validate output",
                )
                .await;
            return;
        };

        let Ok(spanned_yaml) = located_yaml::YamlLoader::load_from_str(
            &file.text.expect("We did request to receive the save text"),
        ) else {
            tracing::error!("Failed to parse YAML");
            return;
        };

        for (_filename, results) in results {
            for (index, result) in results.iter().enumerate() {
                if let DocumentResult::Failure(details) = result {
                    let mut diagnostics = Vec::with_capacity(details.details.causes.len());
                    tracing::info!(
                        "Found {} causes at index {}",
                        details.details.causes.len(),
                        index
                    );
                    tracing::info!(banana = ?spanned_yaml.docs.len(), "hi");
                    tracing::info!(banana = ?spanned_yaml.docs, "hi");
                    let Some(failed_doc) = &spanned_yaml.docs.get(index) else {
                        continue;
                    };
                    tracing::info!("{:?}", failed_doc);

                    for caused in details.details.causes.iter() {
                        let Some(field) = &caused.field else {
                            continue;
                        };
                        let segments = field.field.split(".").map(String::from).collect();
                        tracing::info!("{:?}", segments);
                        let Some(marker) = failed_doc.get_marker(segments) else {
                            continue;
                        };
                        tracing::info!("Found cause at {:?}", marker);
                        let diagnostic = Diagnostic {
                            range: Range {
                                start: Position {
                                    line: (marker.line - 1) as _,
                                    character: marker.col as _,
                                },
                                end: Position {
                                    line: marker.line as _,
                                    character: marker.col as _,
                                },
                            },
                            severity: Some(DiagnosticSeverity::ERROR),
                            source: Some("kubectl-validate".to_string()),
                            message: caused.message.clone(),
                            ..Diagnostic::default()
                        };
                        diagnostics.push(diagnostic);
                    }

                    if diagnostics.is_empty() {
                        let marker = failed_doc.marker;
                        let diagnostic = Diagnostic {
                            range: Range {
                                start: Position {
                                    line: (marker.line - 1) as _,
                                    character: marker.col as _,
                                },
                                end: Position {
                                    line: marker.line as _,
                                    character: marker.col as _,
                                },
                            },
                            severity: Some(DiagnosticSeverity::ERROR),
                            source: Some("kubectl-validate".to_string()),
                            message: details.message.clone(),
                            ..Diagnostic::default()
                        };
                        diagnostics.push(diagnostic);
                    }

                    self.client
                        .publish_diagnostics(file.text_document.uri.clone(), diagnostics, None)
                        .await;
                };
            }
        }
    }

    // async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
    //     Err(Error::method_not_found())
    // }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let (file_appender, _guard) = tracing_appender::non_blocking(
        tracing_appender::rolling::Builder::new()
            .max_log_files(1)
            .filename_prefix("sk8tls.log")
            .rotation(Rotation::HOURLY)
            .build("/tmp")
            .expect("Could not logger on /tmp/sk8tls.log"),
    );
    tracing_subscriber::fmt()
        .with_writer(file_appender)
        .with_ansi(false)
        .init();

    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let (service, socket) = LspService::new(|client| Backend { client });
    Server::new(stdin, stdout, socket).serve(service).await;
}
