mod utils;

use jrsonnet_parser::{LocExpr, ParseError};

use log::{error, trace, warn};
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    notification::{Notification as _, *},
    request::{Formatting, Request as RequestTrait},
    OneOf, *,
};

use std::{collections::HashMap, panic, process};

type Error = Box<dyn std::error::Error>;

fn main() {
    if let Err(err) = real_main() {
        error!("Error: {} ({:?})", err, err);
        error!("A fatal error has occured and rnix-lsp will shut down.");
        drop(err);
        process::exit(1);
    }
}
fn real_main() -> Result<(), Error> {
    env_logger::init();
    panic::set_hook(Box::new(move |panic| {
        error!("----- Panic -----");
        error!("{}", panic);
    }));

    let (connection, io_threads) = Connection::stdio();
    let capabilities = serde_json::to_value(&ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::Full),
                ..TextDocumentSyncOptions::default()
            },
        )),
        completion_provider: Some(CompletionOptions {
            ..CompletionOptions::default()
        }),
        definition_provider: Some(OneOf::<_, _>::Left(true)),
        document_formatting_provider: Some(OneOf::<_, _>::Left(true)),
        document_link_provider: Some(DocumentLinkOptions {
            resolve_provider: Some(false),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        }),
        rename_provider: Some(OneOf::<_, _>::Left(true)),
        selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
        ..ServerCapabilities::default()
    })
    .unwrap();

    connection.initialize(capabilities)?;

    App {
        files: HashMap::new(),
        conn: connection,
    }
    .main();

    io_threads.join()?;

    Ok(())
}

struct App {
    files: HashMap<Url, (Result<LocExpr, ParseError>, String)>,
    conn: Connection,
}
impl App {
    fn reply(&mut self, response: Response) {
        trace!("Sending response: {:#?}", response);
        self.conn.sender.send(Message::Response(response)).unwrap();
    }
    fn notify(&mut self, notification: Notification) {
        trace!("Sending notification: {:#?}", notification);
        self.conn
            .sender
            .send(Message::Notification(notification))
            .unwrap();
    }
    fn err<E>(&mut self, id: RequestId, err: E)
    where
        E: std::fmt::Display,
    {
        warn!("{}", err);
        self.reply(Response::new_err(
            id,
            ErrorCode::UnknownErrorCode as i32,
            err.to_string(),
        ));
    }
    fn main(&mut self) {
        while let Ok(msg) = self.conn.receiver.recv() {
            trace!("Message: {:#?}", msg);
            match msg {
                Message::Request(req) => {
                    let id = req.id.clone();
                    match self.conn.handle_shutdown(&req) {
                        Ok(true) => break,
                        Ok(false) => self.handle_request(req),
                        Err(err) => {
                            // This only fails if a shutdown was
                            // requested in the first place, so it
                            // should definitely break out of the
                            // loop.
                            self.err(id, err);
                            break;
                        }
                    }
                }
                Message::Notification(notification) => {
                    let _ = self.handle_notification(notification);
                }
                Message::Response(_) => (),
            }
        }
    }
    fn handle_request(&mut self, req: Request) {
        fn cast<Kind>(req: &mut Option<Request>) -> Option<(RequestId, Kind::Params)>
        where
            Kind: RequestTrait,
            Kind::Params: serde::de::DeserializeOwned,
        {
            match req.take().unwrap().extract::<Kind::Params>(Kind::METHOD) {
                Ok(value) => Some(value),
                Err(owned) => {
                    *req = Some(owned);
                    None
                }
            }
        }
        let mut req = Some(req);
        if let Some((id, params)) = cast::<Formatting>(&mut req) {
            let changes = match self.files.get(&params.text_document.uri) {
                Some((result, _code)) => match result {
                    Ok(ast) => {
                        error!("HELLOast {:?} ", ast);
                        vec![TextEdit {
                            range: Range {
                                start: Position {
                                    line: 0,
                                    character: 0,
                                },
                                end: Position {
                                    line: 0,
                                    character: 0,
                                },
                            },
                            new_text: "TODO: test".to_string(),
                            ..TextEdit::default()
                        }]
                    }
                    _ => vec![],
                },
                _ => vec![],
            };
            self.reply(Response::new_ok(id, changes));
        } else {
            let req = req.expect("internal error: req should have been wrapped in Some");

            self.reply(Response::new_err(
                req.id,
                ErrorCode::MethodNotFound as i32,
                format!("Unhandled method {}", req.method),
            ))
        }
    }
    fn handle_notification(&mut self, req: Notification) -> Result<(), Error> {
        let parser_settings = jrsonnet_parser::ParserSettings::default();
        match &*req.method {
            DidOpenTextDocument::METHOD => {
                let params: DidOpenTextDocumentParams = serde_json::from_value(req.params)?;
                let text = params.text_document.text;
                let parsed = jrsonnet_parser::parse(&text, &parser_settings);
                self.send_diagnostics(params.text_document.uri.clone(), &text, &parsed)?;
                self.files.insert(params.text_document.uri, (parsed, text));
            }
            DidChangeTextDocument::METHOD => {
                let params: DidChangeTextDocumentParams = serde_json::from_value(req.params)?;
                if let Some(change) = params.content_changes.into_iter().last() {
                    let parsed = jrsonnet_parser::parse(&change.text, &parser_settings);
                    self.send_diagnostics(params.text_document.uri.clone(), &change.text, &parsed)?;
                    self.files
                        .insert(params.text_document.uri, (parsed, change.text));
                }
            }
            _ => (),
        }
        Ok(())
    }
    fn send_diagnostics(
        &mut self,
        uri: Url,
        code: &str,
        _result: &Result<LocExpr, ParseError>,
    ) -> Result<(), Error> {
        let diagnostics = utils::parse(&code);
        self.notify(Notification::new(
            "textDocument/publishDiagnostics".into(),
            PublishDiagnosticsParams {
                uri,
                diagnostics,
                version: None,
            },
        ));
        Ok(())
    }
}
