use lsp_types;

use jrsonnet_evaluator;
use jrsonnet_parser;
use jrsonnet_parser::peg::str::LineCol;

pub fn location_to_position(code: &str, line_col: &LineCol) -> lsp_types::Position {
    let lines = code.split('\n');

    let mut offset = line_col.offset;
    for line in lines {
        if offset <= line.len() {
            break;
        }
        offset -= line.len() + 1;
    }

    return lsp_types::Position {
        line: line_col.line as u32 - 1,
        character: offset as u32 - 1,
    };
}

pub fn parse(text: &str) -> Vec<lsp_types::Diagnostic> {
    let settings = jrsonnet_parser::ParserSettings::default();
    let parsed = jrsonnet_parser::parse(&text, &settings);

    let mut diagnostics = vec![];

    match parsed {
        Ok(ast) => {
            let context = jrsonnet_evaluator::Context::new();
            let _result = jrsonnet_evaluator::evaluate(context, &ast);
        }
        Err(err) => {
            let position_start = location_to_position(text, &err.location);
            let position_end = lsp_types::Position {
                line: position_start.line,
                character: position_start.character + 1,
            };
            diagnostics.push(lsp_types::Diagnostic {
                range: lsp_types::Range {
                    start: position_start,
                    end: position_end,
                },
                severity: Some(lsp_types::DiagnosticSeverity::Error),
                message: err.to_string(),
                ..lsp_types::Diagnostic::default()
            });
        }
    };
    return diagnostics;
}

#[cfg(test)]
mod tests {

    #[test]
    fn parse_simple_jsonnet() {
        let code = r#"
    {
      test1: 1,
      test2: 2.0,
      test3: 3,
    }
"#;

        let res = super::parse(&code);
        assert_eq!(res, vec![]);
    }

    #[test]
    fn parse_simple_jsonnet_parse_error() {
        let code = r#"
    {
      test1: 1,
      test2: 2.0
      test3: 3,
    }
"#;
        let res = super::parse(&code);
        assert_eq!(res, vec![]);
    }
}
