// LSP Manager — manages multiple LSP clients, provides read_lints functionality
//
// - Lazily starts LSP servers when files are touched
// - Detects process crashes and reconnects automatically
// - Supports npx fallback for npm-based servers
// - 16+ built-in server configurations

use crate::lsp_client::LspClient;
use crate::lsp_types::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

// ---- Server Config ----

struct ServerConfig {
    id: String,
    extensions: Vec<&'static str>,
    root_markers: Vec<&'static str>,
    cmd: &'static str,
    args: Vec<&'static str>,
}

fn builtin_servers() -> Vec<ServerConfig> {
    vec![
        // ---- TypeScript/JavaScript ----
        ServerConfig {
            id: "typescript".into(),
            extensions: vec!["ts", "tsx", "js", "jsx", "mjs", "cjs"],
            root_markers: vec!["package.json", "tsconfig.json", "jsconfig.json"],
            cmd: "typescript-language-server",
            args: vec!["--stdio"],
        },
        // ---- Vue ----
        ServerConfig {
            id: "vue".into(),
            extensions: vec!["vue"],
            root_markers: vec!["package.json", "vite.config.ts", "vite.config.js"],
            cmd: "vue-language-server",
            args: vec!["--stdio"],
        },
        // ---- Rust ----
        ServerConfig {
            id: "rust".into(),
            extensions: vec!["rs"],
            root_markers: vec!["Cargo.toml"],
            cmd: "rust-analyzer",
            args: vec![],
        },
        // ---- Python ----
        ServerConfig {
            id: "python".into(),
            extensions: vec!["py", "pyi"],
            root_markers: vec!["pyproject.toml", "setup.py", "setup.cfg", "requirements.txt"],
            cmd: "pyright-langserver",
            args: vec!["--stdio"],
        },
        // ---- Go ----
        ServerConfig {
            id: "gopls".into(),
            extensions: vec!["go"],
            root_markers: vec!["go.mod", "go.sum"],
            cmd: "gopls",
            args: vec![],
        },
        // ---- Svelte ----
        ServerConfig {
            id: "svelte".into(),
            extensions: vec!["svelte"],
            root_markers: vec!["package.json", "svelte.config.js"],
            cmd: "svelte-language-server",
            args: vec!["--stdio"],
        },
        // ---- C/C++ (clangd) ----
        ServerConfig {
            id: "clangd".into(),
            extensions: vec!["c", "h", "cpp", "cc", "cxx", "hpp", "hxx"],
            root_markers: vec!["compile_commands.json", "CMakeLists.txt", "Makefile", ".clangd"],
            cmd: "clangd",
            args: vec![],
        },
        // ---- Java (jdtls) ----
        ServerConfig {
            id: "java".into(),
            extensions: vec!["java"],
            root_markers: vec!["pom.xml", "build.gradle", "build.gradle.kts", ".project"],
            cmd: "jdtls",
            args: vec![],
        },
        // ---- C# (csharp-ls) ----
        ServerConfig {
            id: "csharp".into(),
            extensions: vec!["cs"],
            root_markers: vec!["*.sln", "*.csproj", "project.json"],
            cmd: "csharp-ls",
            args: vec![],
        },
        // ---- Kotlin ----
        ServerConfig {
            id: "kotlin".into(),
            extensions: vec!["kt", "kts"],
            root_markers: vec!["build.gradle", "build.gradle.kts"],
            cmd: "kotlin-language-server",
            args: vec![],
        },
        // ---- Dart/Flutter ----
        ServerConfig {
            id: "dart".into(),
            extensions: vec!["dart"],
            root_markers: vec!["pubspec.yaml", "analysis_options.yaml"],
            cmd: "dart",
            args: vec!["language-server", "--client-id=dart-code"],
        },
        // ---- Ruby ----
        ServerConfig {
            id: "ruby".into(),
            extensions: vec!["rb", "erb"],
            root_markers: vec!["Gemfile", "gems.rb", ".ruby-lsp"],
            cmd: "ruby-lsp",
            args: vec![],
        },
        // ---- CSS/SCSS/LESS (vscode-langservers) ----
        ServerConfig {
            id: "css".into(),
            extensions: vec!["css", "scss", "less"],
            root_markers: vec!["package.json", ".git"],
            cmd: "vscode-css-language-server",
            args: vec!["--stdio"],
        },
        // ---- HTML ----
        ServerConfig {
            id: "html".into(),
            extensions: vec!["html", "htm"],
            root_markers: vec![".git"],
            cmd: "vscode-html-language-server",
            args: vec!["--stdio"],
        },
        // ---- JSON ----
        ServerConfig {
            id: "json".into(),
            extensions: vec!["json", "jsonc"],
            root_markers: vec![".git"],
            cmd: "vscode-json-language-server",
            args: vec!["--stdio"],
        },
        // ---- YAML ----
        ServerConfig {
            id: "yaml".into(),
            extensions: vec!["yaml", "yml"],
            root_markers: vec![".git"],
            cmd: "yaml-language-server",
            args: vec!["--stdio"],
        },
        // ---- Bash ----
        ServerConfig {
            id: "bash".into(),
            extensions: vec!["sh", "bash", "zsh", "bashrc", "bash_profile"],
            root_markers: vec![".git"],
            cmd: "bash-language-server",
            args: vec!["start"],
        },
        // ---- Dockerfile ----
        ServerConfig {
            id: "dockerfile".into(),
            extensions: vec!["dockerfile"],
            root_markers: vec!["Dockerfile", "docker-compose.yml", "docker-compose.yaml"],
            cmd: "docker-langserver",
            args: vec!["--stdio"],
        },
    ]
}

// ---- Manager ----

pub struct LspManager {
    root: String,
    clients: Vec<Arc<Mutex<LspClient>>>,
    ext_index: HashMap<String, usize>,
    configs: Vec<ServerConfig>,
}

impl LspManager {
    pub fn new(root: &str) -> Self {
        Self {
            root: root.to_string(),
            clients: Vec::new(),
            ext_index: HashMap::new(),
            configs: builtin_servers(),
        }
    }

    /// Get or create the LSP client for a given file extension.
    fn get_or_start_client(&mut self, ext: &str) -> Option<Arc<Mutex<LspClient>>> {
        if let Some(&idx) = self.ext_index.get(ext) {
            return Some(self.clients[idx].clone());
        }

        let config = self.configs.iter().find(|c| c.extensions.contains(&ext))?;
        let root = find_root(&self.root, &config.root_markers).unwrap_or_else(|| self.root.clone());

        eprintln!("[lsp_manager] Starting {} for {} (root: {})", config.id, ext, root);

        let args: Vec<String> = config.args.iter().map(|s| s.to_string()).collect();
        let client = LspClient::spawn_with_fallback(&config.id, &root, config.cmd, &args)?;
        let client = Arc::new(Mutex::new(client));

        if let Err(e) = client.lock().unwrap().initialize() {
            eprintln!("[lsp_manager] {} init failed: {}", config.id, e);
            return None;
        }

        let idx = self.clients.len();
        self.clients.push(client.clone());
        for server_ext in &config.extensions {
            self.ext_index.insert(server_ext.to_string(), idx);
        }

        Some(client)
    }

    /// Touch a file: notify the LSP server. Sends didOpen (first time)
    /// or didChange (subsequent). Returns diagnostics for the file.
    pub fn touch_file(&mut self, file_path: &str) -> Result<Vec<LspDiagnostic>, String> {
        let ext = Path::new(file_path).extension().and_then(|e| e.to_str()).unwrap_or("");

        let client_arc = match self.get_or_start_client(ext) {
            Some(c) => c,
            None => return Ok(Vec::new()),
        };

        let mut client = client_arc.lock().unwrap();

        // Check if process crashed and reconnect
        if !client.is_alive() {
            eprintln!("[lsp_manager] {} process dead, reconnecting...", client.server_id);
            if let Err(e) = client.reconnect() {
                eprintln!("[lsp_manager] {} reconnect failed: {}", client.server_id, e);
                return Ok(Vec::new());
            }
        }

        let content = std::fs::read_to_string(file_path)
            .map_err(|e| format!("Cannot read {}: {}", file_path, e))?;
        let language_id = ext_to_lang_id(ext);

        client.touch_file(file_path, &content, language_id);
        let diags = client.wait_for_diagnostics(file_path, 5000);
        Ok(diags)
    }

    /// Read lint results for a file or all files.
    pub fn read_lints(&self, file_path: Option<&str>) -> Vec<FileDiagnostics> {
        if let Some(path) = file_path {
            let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("");
            if let Some(&idx) = self.ext_index.get(ext) {
                let client = self.clients[idx].lock().unwrap();
                let diags = client.get_diagnostics(path);
                vec![FileDiagnostics { file: path.to_string(), diagnostics: diags }]
            } else {
                vec![]
            }
        } else {
            let mut results = Vec::new();
            for client_arc in &self.clients {
                let client = client_arc.lock().unwrap();
                for (file, diagnostics) in client.all_diagnostics() {
                    if !diagnostics.is_empty() {
                        results.push(FileDiagnostics { file, diagnostics });
                    }
                }
            }
            results
        }
    }

    /// Shut down all LSP clients.
    pub fn shutdown(&mut self) {
        for client in &self.clients {
            client.lock().unwrap().shutdown();
        }
        self.clients.clear();
        self.ext_index.clear();
    }
}

impl Drop for LspManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ---- Helpers ----

fn find_root(base: &str, markers: &[&str]) -> Option<String> {
    let mut current = Path::new(base).to_path_buf();
    loop {
        for marker in markers {
            if marker.contains('*') {
                // Glob pattern — try to match any file
                let pattern = marker;
                if let Ok(entries) = std::fs::read_dir(&current) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let marker_no_star = pattern.replace('*', "");
                        if name.contains(&marker_no_star) {
                            return Some(current.to_string_lossy().to_string());
                        }
                    }
                }
            } else if current.join(marker).exists() {
                return Some(current.to_string_lossy().to_string());
            }
        }
        if let Some(parent) = current.parent() {
            if parent == current {
                break;
            }
            current = parent.to_path_buf();
        } else {
            break;
        }
    }
    None
}

fn ext_to_lang_id(ext: &str) -> &'static str {
    match ext {
        "ts" => "typescript",
        "tsx" => "typescriptreact",
        "js" => "javascript",
        "jsx" => "javascriptreact",
        "mjs" => "javascript",
        "cjs" => "javascript",
        "rs" => "rust",
        "py" | "pyi" => "python",
        "go" => "go",
        "vue" => "vue",
        "svelte" => "svelte",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "cs" => "csharp",
        "dart" => "dart",
        "rb" | "erb" => "ruby",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => "cpp",
        "css" => "css",
        "scss" => "scss",
        "less" => "less",
        "html" | "htm" => "html",
        "json" | "jsonc" => "json",
        "yaml" | "yml" => "yaml",
        "sh" | "bash" | "zsh" => "shellscript",
        "dockerfile" => "dockerfile",
        _ => "plaintext",
    }
}
