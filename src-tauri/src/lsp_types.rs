// Minimal LSP types — enough for diagnostics without depending on the full lsp-types crate

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspRange {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspDiagnostic {
    pub range: LspRange,
    pub severity: Option<u32>, // 1=Error, 2=Warning, 3=Info, 4=Hint
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishDiagnosticsParams {
    pub uri: String,
    pub diagnostics: Vec<LspDiagnostic>,
}

#[derive(Debug, Clone)]
pub struct FileDiagnostics {
    pub file: String,
    pub diagnostics: Vec<LspDiagnostic>,
}

pub fn severity_label(severity: Option<u32>) -> &'static str {
    match severity {
        Some(1) => "ERROR",
        Some(2) => "WARN",
        Some(3) => "INFO",
        Some(4) => "HINT",
        _ => "ERROR",
    }
}

pub fn format_diagnostics(diags: &[LspDiagnostic]) -> String {
    diags
        .iter()
        .map(|d| {
            let sev = severity_label(d.severity);
            let line = d.range.start.line + 1;
            let col = d.range.start.character + 1;
            format!("{} [{}:{}] {}", sev, line, col, d.message)
        })
        .collect::<Vec<_>>()
        .join("\n")
}
