// Code Graph - Built-in code intelligence engine for the explore agent
//
// Extracts symbols and relationships from source code using regex patterns,
// builds an in-memory graph, and provides traversal/query operations.
//
// Supports: Rust, TypeScript, JavaScript, Python, Go, Java, C/C++, C#, Ruby, PHP, Kotlin, Swift, Dart

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

// ========== Types ==========

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum NodeKind {
    #[serde(rename = "function")]
    Function,
    #[serde(rename = "method")]
    Method,
    #[serde(rename = "class")]
    Class,
    #[serde(rename = "struct")]
    Struct,
    #[serde(rename = "interface")]
    Interface,
    #[serde(rename = "enum")]
    Enum,
    #[serde(rename = "variable")]
    Variable,
    #[serde(rename = "constant")]
    Constant,
    #[serde(rename = "module")]
    Module,
    #[serde(rename = "type")]
    Type,
    #[serde(rename = "component")]
    Component,
    #[serde(rename = "route")]
    Route,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    #[serde(rename = "calls")]
    Calls,
    #[serde(rename = "contains")]
    Contains,
    #[serde(rename = "imports")]
    Imports,
    #[serde(rename = "references")]
    References,
    #[serde(rename = "extends")]
    Extends,
    #[serde(rename = "implements")]
    Implements,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeNode {
    pub id: String,
    pub name: String,
    pub kind: NodeKind,
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(rename = "startLine")]
    pub start_line: u32,
    #[serde(rename = "endLine")]
    pub end_line: u32,
    pub signature: String,
    pub docstring: String,
    #[serde(rename = "isExported")]
    pub is_exported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEdge {
    #[serde(rename = "sourceId")]
    pub source_id: String,
    #[serde(rename = "targetId")]
    pub target_id: String,
    pub kind: EdgeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subgraph {
    pub nodes: Vec<CodeNode>,
    pub edges: Vec<CodeEdge>,
    pub roots: Vec<String>,
}

// ========== Extraction Result ==========

#[derive(Debug, Clone)]
struct ExtractedSymbol {
    name: String,
    kind: NodeKind,
    file_path: String,
    start_line: u32,
    end_line: u32,
    signature: String,
    docstring: String,
    is_exported: bool,
    parent_name: Option<String>, // for methods: class name
}

#[derive(Debug, Clone)]
struct ExtractedCall {
    file_path: String,
    _line: u32,
    caller_name: String,
    callee_name: String,
}

#[derive(Debug, Clone)]
struct ExtractedImport {
    file_path: String,
    _imported_name: String,
    source_path: String,
}

// ========== Language Detection ==========

fn detect_language(file_path: &str) -> Option<&'static str> {
    let ext = Path::new(file_path).extension()?.to_str()?;
    match ext {
        "rs" => Some("rust"),
        "ts" | "tsx" => Some("typescript"),
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        "py" | "pyi" => Some("python"),
        "go" => Some("go"),
        "java" => Some("java"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("cpp"),
        "cs" => Some("csharp"),
        "rb" => Some("ruby"),
        "php" => Some("php"),
        "kt" | "kts" => Some("kotlin"),
        "swift" => Some("swift"),
        "dart" => Some("dart"),
        "vue" => Some("vue"),
        "svelte" => Some("svelte"),
        "scala" => Some("scala"),
        _ => None,
    }
}

fn is_source_file(path: &str) -> bool {
    detect_language(path).is_some()
}

fn skip_dir(name: &str) -> bool {
    matches!(name, "node_modules" | "target" | ".git" | "dist" | "build" | ".next" | ".venv" | "__pycache__" | "vendor" | ".idea" | ".vscode" | "coverage" | ".nyc_output")
}

// ========== Regex Extractors per Language ==========

struct LangPatterns {
    function: Regex,
    _method: Regex,
    class: Regex,
    struct_def: Regex,
    interface: Regex,
    enum_def: Regex,
    _variable: Regex,
    constant: Regex,
    import: Regex,
    call: Regex,
    export: Regex,
    docstring: Regex,
    _extends: Regex,
    _implements: Regex,
}

fn patterns_for_language(lang: &str) -> LangPatterns {
    match lang {
        "rust" => LangPatterns {
            function: Regex::new(r"(?m)^\s*(pub\s+(?:async\s+)?)?fn\s+(\w+)\s*([<\(])").unwrap(),
            _method: Regex::new(r"(?m)^\s*(pub\s+)?fn\s+(\w+)\s*([<\(])").unwrap(),
            class: Regex::new(r"(?m)^\s*(pub\s+)?struct\s+(\w+)").unwrap(),
            struct_def: Regex::new(r"(?m)^\s*(pub\s+)?struct\s+(\w+)").unwrap(),
            interface: Regex::new(r"(?m)^\s*(pub\s+)?trait\s+(\w+)").unwrap(),
            enum_def: Regex::new(r"(?m)^\s*(pub\s+)?enum\s+(\w+)").unwrap(),
            _variable: Regex::new(r"(?m)^\s*(pub\s+)?(let\s+mut\s+|let\s+)(\w+)").unwrap(),
            constant: Regex::new(r"(?m)^\s*(pub\s+)?const\s+(\w+)\s*:").unwrap(),
            import: Regex::new(r"(?m)^\s*use\s+([\w:]+)(?:::[\{\w\s,}]+)?\s*;").unwrap(),
            call: Regex::new(r"(\w+)\s*\(").unwrap(),
            export: Regex::new(r"pub\s").unwrap(),
            docstring: Regex::new(r#"(?s)(/\*\*.*?\*/|///.*?(?:\n\s*///.*)*)"#).unwrap(),
            _extends: Regex::new(r"(?m)impl\s+(\w+)\s+for\s+(\w+)").unwrap(),
            _implements: Regex::new(r"(?m)impl\s+(\w+)\s+for\s+(\w+)").unwrap(),
        },
        "typescript" | "javascript" => LangPatterns {
            function: Regex::new(r"(?m)(?:export\s+)?(?:async\s+)?function\s+(\w+)\s*[<\(]").unwrap(),
            _method: Regex::new(r"(?m)(?:async\s+)?(\w+)\s*[<\(][^)]*\)\s*\{").unwrap(),
            class: Regex::new(r"(?m)(?:export\s+)?(?:abstract\s+)?class\s+(\w+)").unwrap(),
            struct_def: Regex::new(r"(?m)(?:export\s+)?interface\s+(\w+)").unwrap(),
            interface: Regex::new(r"(?m)(?:export\s+)?interface\s+(\w+)").unwrap(),
            enum_def: Regex::new(r"(?m)(?:export\s+)?enum\s+(\w+)").unwrap(),
            _variable: Regex::new(r"(?m)(?:export\s+)?(?:const|let|var)\s+(\w+)\s*=").unwrap(),
            constant: Regex::new(r"(?m)(?:export\s+)?const\s+(\w+)\s*[:=]").unwrap(),
            import: Regex::new(r#"(?m)^\s*import\s+(?:\{[^}]*\}|\*\s+as\s+\w+|\w+)\s+from\s+['"]([^'"]+)['"]"#).unwrap(),
            call: Regex::new(r"(\w+)\s*\(").unwrap(),
            export: Regex::new(r"export\s").unwrap(),
            docstring: Regex::new(r#"(?s)(/\*\*.*?\*/)"#).unwrap(),
            _extends: Regex::new(r"(?m)class\s+\w+\s+extends\s+(\w+)").unwrap(),
            _implements: Regex::new(r"(?m)class\s+\w+\s+implements\s+([\w\s,]+)").unwrap(),
        },
        "python" => LangPatterns {
            function: Regex::new(r"(?m)^\s*def\s+(\w+)\s*\(").unwrap(),
            _method: Regex::new(r"(?m)^\s*def\s+(\w+)\s*\(self[\s,\)]").unwrap(),
            class: Regex::new(r"(?m)^\s*class\s+(\w+)\s*[\(:]").unwrap(),
            struct_def: Regex::new(r"(?m)^\s*class\s+(\w+)\s*[\(:]").unwrap(),
            interface: Regex::new(r"(?m)^\s*class\s+(\w+).*ABC").unwrap(),
            enum_def: Regex::new(r"(?m)^\s*class\s+(\w+)\s*\(\s*enum\.Enum").unwrap(),
            _variable: Regex::new(r"(?m)^\s*(\w+)\s*=\s*").unwrap(),
            constant: Regex::new(r"(?m)^\s*([A-Z_][A-Z0-9_]*)\s*=").unwrap(),
            import: Regex::new(r"(?m)^\s*(?:from\s+([\w.]+)\s+)?import\s+([\w\s,]+)").unwrap(),
            call: Regex::new(r"(\w+)\s*\(").unwrap(),
            export: Regex::new(r"(?m)^\s*(?:__all__|def\s+\w+|class\s+\w+)").unwrap(),
            docstring: Regex::new(r#"(?s)(\"\"\".*?\"\"\")"#).unwrap(),
            _extends: Regex::new(r"(?m)class\s+(\w+)\s*\(\s*(\w+)").unwrap(),
            _implements: Regex::new(r"(?m)class\s+(\w+)\s*\(\s*\w+\s*,\s*(\w+)").unwrap(),
        },
        "go" => LangPatterns {
            function: Regex::new(r"(?m)^\s*func\s+(\w+)\s*\(").unwrap(),
            _method: Regex::new(r"(?m)^\s*func\s+\(\w+\s+\*?\w+\)\s+(\w+)\s*\(").unwrap(),
            class: Regex::new(r"(?m)^\s*type\s+(\w+)\s+struct\s*\{").unwrap(),
            struct_def: Regex::new(r"(?m)^\s*type\s+(\w+)\s+struct\s*\{").unwrap(),
            interface: Regex::new(r"(?m)^\s*type\s+(\w+)\s+interface\s*\{").unwrap(),
            enum_def: Regex::new(r"(?m)^\s*type\s+(\w+)\s+(?:int|string|float64|byte)").unwrap(),
            _variable: Regex::new(r"(?m)^\s*(?:var\s+)?(\w+)\s*:?=\s*").unwrap(),
            constant: Regex::new(r"(?m)^\s*const\s+(\w+)\s*=").unwrap(),
            import: Regex::new(r#"(?m)^\s*import\s+(?:\(\s*(?:[\s\S]*?)\s*\)|"([^"]+)")"#).unwrap(),
            call: Regex::new(r"(\w+)\s*\(").unwrap(),
            export: Regex::new(r"(?m)^[A-Z]\w*\s").unwrap(),
            docstring: Regex::new(r"(?s)(/\*.*?\*/)").unwrap(),
            _extends: Regex::new(r"(?m)type\s+\w+\s+struct\s*\{[^}]*\b(\w+)\b").unwrap(),
            _implements: Regex::new(r"(?m)var\s+_\s+(\w+)\s*=\s*\(\*?\w+\)\(nil\)").unwrap(),
        },
        "java" | "kotlin" | "scala" => LangPatterns {
            function: Regex::new(r"(?m)(?:public|private|protected|static|\s)*\s+\w+\s+(\w+)\s*\(").unwrap(),
            _method: Regex::new(r"(?m)(?:public|private|protected|static|\s)*\s+\w+\s+(\w+)\s*\(").unwrap(),
            class: Regex::new(r"(?m)(?:public\s+)?(?:abstract\s+)?(?:final\s+)?class\s+(\w+)").unwrap(),
            struct_def: Regex::new(r"(?m)(?:public\s+)?class\s+(\w+)").unwrap(),
            interface: Regex::new(r"(?m)(?:public\s+)?interface\s+(\w+)").unwrap(),
            enum_def: Regex::new(r"(?m)(?:public\s+)?enum\s+(\w+)").unwrap(),
            _variable: Regex::new(r"(?m)(?:private|public|protected)?\s+\w+\s+(\w+)\s*[=;]").unwrap(),
            constant: Regex::new(r"(?m)static\s+final\s+\w+\s+(\w+)\s*=").unwrap(),
            import: Regex::new(r"(?m)^\s*import\s+([\w.]+)").unwrap(),
            call: Regex::new(r"(\w+)\s*\(").unwrap(),
            export: Regex::new(r"public\s").unwrap(),
            docstring: Regex::new(r#"(?s)(/\*\*.*?\*/)"#).unwrap(),
            _extends: Regex::new(r"(?m)class\s+\w+\s+extends\s+(\w+)").unwrap(),
            _implements: Regex::new(r"(?m)class\s+\w+\s+implements\s+([\w\s,]+)").unwrap(),
        },
        // Fallback: generic patterns that work reasonably well for most languages
        _ => LangPatterns {
            function: Regex::new(r"(?m)(?:func(?:tion)?\s+)?(\w+)\s*\([^)]*\)\s*(?:\{|=>|->)").unwrap(),
            _method: Regex::new(r"(?m)\b(\w+)\s*\([^)]*\)\s*\{").unwrap(),
            class: Regex::new(r"(?m)(?:class|struct|interface|enum)\s+(\w+)").unwrap(),
            struct_def: Regex::new(r"(?m)(?:struct|type)\s+(\w+)").unwrap(),
            interface: Regex::new(r"(?m)(?:interface|trait|protocol)\s+(\w+)").unwrap(),
            enum_def: Regex::new(r"(?m)enum\s+(\w+)").unwrap(),
            _variable: Regex::new(r"(?m)(?:let|var|const|val)\s+(\w+)\s*[=:]").unwrap(),
            constant: Regex::new(r"(?m)(?:const|final|val)\s+(\w+)\s*[=:]").unwrap(),
            import: Regex::new(r#"(?m)(?:import|use|require|include)\s+.*?['"]?([\w./]+)['"]?"#).unwrap(),
            call: Regex::new(r"(\w+)\s*\(").unwrap(),
            export: Regex::new(r"(?m)(?:export|pub|public)\s").unwrap(),
            docstring: Regex::new(r#"(?s)(/\*\*.*?\*/|///[^\n]*|\"\"\".*?\"\"\")"#).unwrap(),
            _extends: Regex::new(r"(?m)(?:extends|:)\s+(\w+)").unwrap(),
            _implements: Regex::new(r"(?m)(?:implements|:\s*\w+\s*[,{])").unwrap(),
        },
    }
}

// ========== Graph ==========

#[derive(Debug, Clone, Default)]
pub struct CodeGraph {
    pub nodes: HashMap<String, CodeNode>,
    pub edges: Vec<CodeEdge>,
    pub files: HashSet<String>,
    project_name: String,
    // Indexes
    by_name: HashMap<String, Vec<String>>,
    by_kind: HashMap<NodeKind, Vec<String>>,
    by_file: HashMap<String, Vec<String>>,
    pub stats: GraphStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedGraph {
    nodes: Vec<CodeNode>,
    edges: Vec<CodeEdge>,
    files: Vec<String>,
    stats: GraphStats,
    project_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphStats {
    pub total_files: usize,
    pub total_nodes: usize,
    pub total_edges: usize,
    pub languages: Vec<String>,
    #[serde(rename = "nodesByKind")]
    pub nodes_by_kind: HashMap<String, usize>,
    pub build_time_ms: u64,
}

// ========== Public API ==========

impl CodeGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.edges.clear();
        self.files.clear();
        self.by_name.clear();
        self.by_kind.clear();
        self.by_file.clear();
        self.stats = GraphStats::default();
    }

    fn graph_dir() -> std::path::PathBuf {
        if cfg!(windows) {
            std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOMEDRIVE").and_then(|hd| {
                    std::env::var("HOMEPATH").map(|hp| format!("{}{}", hd, hp))
                }))
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
        } else {
            std::env::var("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
        }
        .join(".minimaxcode")
        .join("project mem")
    }

    /// Extract project name from path.
    /// Walks up to find `.git` first (the definitive project root). If not found,
    /// falls back to the highest-level project marker. This prevents subdirectories
    /// with their own markers (e.g. monorepo crates) from being treated as separate projects.
    fn project_name_from_path(root: &str) -> String {
        let start = if Path::new(root).is_dir() {
            Path::new(root).to_path_buf()
        } else {
            Path::new(root).parent().map(|p| p.to_path_buf()).unwrap_or_else(|| Path::new(root).to_path_buf())
        };

        // Phase 1: walk up looking for .git — the definitive project root anchor.
        // Limit to 6 levels to avoid hijacking by a home-directory dotfiles repo.
        {
            let mut current = start.clone();
            for _ in 0..6 {
                if current.join(".git").exists() {
                    return sanitize_name(
                        &current.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "unknown".to_string())
                    );
                }
                if let Some(parent) = current.parent() {
                    if parent == current { break; }
                    current = parent.to_path_buf();
                } else { break; }
            }
        }

        // Phase 2: no .git found — walk up looking for other project markers.
        let markers = [
            "package.json", "Cargo.toml", "go.mod", "pyproject.toml", "setup.py",
            "Gemfile", "pom.xml", "build.gradle", "build.gradle.kts", "CMakeLists.txt",
            "Makefile",
        ];
        {
            let mut current = start.clone();
            loop {
                for m in &markers {
                    if current.join(m).exists() {
                        return sanitize_name(
                            &current.file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "unknown".to_string())
                        );
                    }
                }
                if let Some(parent) = current.parent() {
                    if parent == current { break; }
                    current = parent.to_path_buf();
                } else { break; }
            }
        }

        // Phase 3: nothing found — use the start directory name.
        sanitize_name(
            &start.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        )
    }
}

    fn sanitize_name(name: &str) -> String {
        name.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect()
    }

impl CodeGraph {
    /// Save graph to disk
    pub fn save(&self) -> Result<String, String> {
        let dir = Self::graph_dir().join(&self.project_name);
        std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create dir: {}", e))?;
        let file_path = dir.join("code_graph.json");

        let persisted = PersistedGraph {
            nodes: self.nodes.values().cloned().collect(),
            edges: self.edges.clone(),
            files: self.files.iter().cloned().collect(),
            stats: self.stats.clone(),
            project_name: self.project_name.clone(),
        };

        let json = serde_json::to_string_pretty(&persisted)
            .map_err(|e| format!("Serialization failed: {}", e))?;
        std::fs::write(&file_path, json)
            .map_err(|e| format!("Failed to write: {}", e))?;

        Ok(file_path.to_string_lossy().to_string())
    }

    /// Load graph from disk. Returns true if loaded, false if no saved graph exists.
    pub fn load(&mut self, project_root: &str) -> Result<bool, String> {
        let project_name = Self::project_name_from_path(project_root);
        self.project_name = project_name;

        let file_path = Self::graph_dir().join(&self.project_name).join("code_graph.json");
        if !file_path.exists() {
            return Ok(false);
        }

        let json = std::fs::read_to_string(&file_path)
            .map_err(|e| format!("Failed to read: {}", e))?;
        let persisted: PersistedGraph = serde_json::from_str(&json)
            .map_err(|e| format!("Deserialization failed: {}", e))?;

        self.clear();
        self.project_name = persisted.project_name;

        // Rebuild indexes from loaded nodes
        for node in &persisted.nodes {
            self.by_name.entry(node.name.clone()).or_default().push(node.id.clone());
            self.by_kind.entry(node.kind.clone()).or_default().push(node.id.clone());
            self.by_file.entry(node.file_path.clone()).or_default().push(node.id.clone());
            self.nodes.insert(node.id.clone(), node.clone());
        }
        self.edges = persisted.edges;
        self.files = persisted.files.into_iter().collect();
        self.stats = persisted.stats;

        Ok(true)
    }

    /// Incremental sync: update graph for changed files (from git diff or git status)
    pub fn sync(&mut self, root: &str, changed_files: &[String]) -> Result<GraphStats, String> {
        let start = std::time::Instant::now();
        self.project_name = Self::project_name_from_path(root);

        // Remove old nodes and edges for changed files
        let mut remove_ids: HashSet<String> = HashSet::new();
        for file_path in changed_files {
            if let Some(ids) = self.by_file.remove(file_path) {
                for id in &ids {
                    remove_ids.insert(id.clone());
                    self.nodes.remove(id);
                }
            }
            self.files.remove(file_path);
        }

        // Remove edges involving removed nodes
        self.edges.retain(|e| !remove_ids.contains(&e.source_id) && !remove_ids.contains(&e.target_id));

        // Clean up by_name and by_kind indexes
        for ids in self.by_name.values_mut() {
            ids.retain(|id| !remove_ids.contains(id));
        }
        for ids in self.by_kind.values_mut() {
            ids.retain(|id| !remove_ids.contains(id));
        }

        // Re-extract symbols from changed files
        let mut symbols: Vec<ExtractedSymbol> = Vec::new();
        let mut calls: Vec<ExtractedCall> = Vec::new();
        let mut imports: Vec<ExtractedImport> = Vec::new();

        for file_path in changed_files {
            if let Some(lang) = detect_language(file_path) {
                let content = match std::fs::read_to_string(file_path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let patterns = patterns_for_language(lang);
                self.extract_symbols(file_path, &content, &patterns, &mut symbols);
                self.extract_calls(file_path, &content, &patterns, &mut calls);
                self.extract_imports(file_path, &content, &patterns, &mut imports);
            }
            if Path::new(file_path).exists() {
                self.files.insert(file_path.clone());
            }
        }

        // Rebuild nodes for changed files
        let mut file_nodes: HashMap<String, bool> = HashMap::new();
        for sym in &symbols {
            let id = format!("{}::{}", sym.file_path, sym.name);
            let node = CodeNode {
                id: id.clone(),
                name: sym.name.clone(),
                kind: sym.kind.clone(),
                file_path: sym.file_path.clone(),
                start_line: sym.start_line,
                end_line: sym.end_line,
                signature: sym.signature.clone(),
                docstring: sym.docstring.clone(),
                is_exported: sym.is_exported,
            };
            self.add_node_indexes(&node);
            self.nodes.insert(id.clone(), node);

            let file_id = format!("file::{}", sym.file_path);
            if !file_nodes.contains_key(&sym.file_path) {
                file_nodes.insert(sym.file_path.clone(), true);
            }
            self.edges.push(CodeEdge {
                source_id: file_id,
                target_id: id,
                kind: EdgeKind::Contains,
            });
        }

        // Rebuild call edges for changed files
        for call in &calls {
            let caller_id = format!("{}::{}", call.file_path, call.caller_name);
            if let Some(callee_ids) = self.by_name.get(&call.callee_name) {
                for callee_id in callee_ids {
                    if self.nodes.contains_key(callee_id) {
                        self.edges.push(CodeEdge {
                            source_id: caller_id.clone(),
                            target_id: callee_id.clone(),
                            kind: EdgeKind::Calls,
                        });
                        break;
                    }
                }
            }
        }

        // Rebuild import edges for changed files
        for imp in &imports {
            let resolved = self.resolve_import_path(&imp.file_path, &imp.source_path);
            if self.by_file.contains_key(&resolved) || self.files.contains(&resolved) {
                self.edges.push(CodeEdge {
                    source_id: format!("file::{}", imp.file_path),
                    target_id: format!("file::{}", resolved),
                    kind: EdgeKind::Imports,
                });
            }
        }

        // Update stats
        let mut nodes_by_kind: HashMap<String, usize> = HashMap::new();
        for node in self.nodes.values() {
            let kind_str = serde_json::to_string(&node.kind).unwrap_or_default();
            *nodes_by_kind.entry(kind_str.trim_matches('"').to_string()).or_default() += 1;
        }
        self.stats = GraphStats {
            total_files: self.files.len(),
            total_nodes: self.nodes.len(),
            total_edges: self.edges.len(),
            languages: self.stats.languages.clone(),
            nodes_by_kind,
            build_time_ms: start.elapsed().as_millis() as u64,
        };

        // Persist updated graph
        if let Err(e) = self.save() {
            eprintln!("[code_graph] Failed to save after sync: {}", e);
        }

        Ok(self.stats.clone())
    }

    /// Build graph from project directory (auto-saves to disk)
    pub fn build(&mut self, root: &str) -> Result<GraphStats, String> {
        let start = std::time::Instant::now();
        self.clear();

        let mut files: Vec<String> = Vec::new();
        self.collect_files(root, &mut files);

        let mut symbols: Vec<ExtractedSymbol> = Vec::new();
        let mut calls: Vec<ExtractedCall> = Vec::new();
        let mut imports: Vec<ExtractedImport> = Vec::new();
        let mut file_langs: HashMap<String, String> = HashMap::new();

        for file_path in &files {
            if let Some(lang) = detect_language(file_path) {
                file_langs.insert(file_path.clone(), lang.to_string());
                let content = match std::fs::read_to_string(file_path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let patterns = patterns_for_language(lang);
                self.extract_symbols(file_path, &content, &patterns, &mut symbols);
                self.extract_calls(file_path, &content, &patterns, &mut calls);
                self.extract_imports(file_path, &content, &patterns, &mut imports);
            }
            self.files.insert(file_path.clone());
        }

        // Build nodes
        let mut file_nodes: HashMap<String, String> = HashMap::new();
        for sym in &symbols {
            let id = format!("{}::{}", sym.file_path, sym.name);
            let node = CodeNode {
                id: id.clone(),
                name: sym.name.clone(),
                kind: sym.kind.clone(),
                file_path: sym.file_path.clone(),
                start_line: sym.start_line,
                end_line: sym.end_line,
                signature: sym.signature.clone(),
                docstring: sym.docstring.clone(),
                is_exported: sym.is_exported,
            };
            self.add_node_indexes(&node);
            self.nodes.insert(id.clone(), node);

            // File → contains → symbol
            let file_id = format!("file::{}", sym.file_path);
            if !file_nodes.contains_key(&sym.file_path) {
                file_nodes.insert(sym.file_path.clone(), file_id.clone());
                self.by_file.entry(sym.file_path.clone()).or_default();
            }
            self.edges.push(CodeEdge {
                source_id: file_id.clone(),
                target_id: id.clone(),
                kind: EdgeKind::Contains,
            });

            // Class → contains → method
            if let Some(ref parent) = sym.parent_name {
                let parent_id = format!("{}::{}", sym.file_path, parent);
                if self.nodes.contains_key(&parent_id) {
                    self.edges.push(CodeEdge {
                        source_id: parent_id,
                        target_id: id.clone(),
                        kind: EdgeKind::Contains,
                    });
                }
            }
        }

        // Build call edges (resolve call sites to known symbols)
        for call in &calls {
            let caller_id = format!("{}::{}", call.file_path, call.caller_name);
            // Find callee by name across all files
            if let Some(callee_ids) = self.by_name.get(&call.callee_name) {
                for callee_id in callee_ids {
                    if self.nodes.contains_key(callee_id) {
                        self.edges.push(CodeEdge {
                            source_id: caller_id.clone(),
                            target_id: callee_id.clone(),
                            kind: EdgeKind::Calls,
                        });
                        break; // Only connect to first match
                    }
                }
            }
        }

        // Build import edges
        for imp in &imports {
            // Try to find the imported file
            let resolved = self.resolve_import_path(&imp.file_path, &imp.source_path);
            let target_file_id = format!("file::{}", resolved);
            if self.by_file.contains_key(&resolved) || self.files.contains(&resolved) {
                self.edges.push(CodeEdge {
                    source_id: format!("file::{}", imp.file_path),
                    target_id: target_file_id,
                    kind: EdgeKind::Imports,
                });
            }
        }

        // Collect stats
        let mut nodes_by_kind: HashMap<String, usize> = HashMap::new();
        for node in self.nodes.values() {
            let kind_str = serde_json::to_string(&node.kind).unwrap_or_default();
            *nodes_by_kind.entry(kind_str.trim_matches('"').to_string()).or_default() += 1;
        }

        let mut languages: Vec<String> = file_langs.values().cloned().collect();
        languages.sort();
        languages.dedup();

        self.stats = GraphStats {
            total_files: self.files.len(),
            total_nodes: self.nodes.len(),
            total_edges: self.edges.len(),
            languages,
            nodes_by_kind,
            build_time_ms: start.elapsed().as_millis() as u64,
        };

        // Persist to disk
        if let Err(e) = self.save() {
            eprintln!("[code_graph] Failed to save: {}", e);
        }

        Ok(self.stats.clone())
    }

    /// Search nodes by name (fuzzy, substring match)
    pub fn search(&self, query: &str, top_k: usize) -> Vec<CodeNode> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<(&CodeNode, i32)> = Vec::new();

        for (name, ids) in &self.by_name {
            let name_lower = name.to_lowercase();
            let score = if name_lower == query_lower {
                100
            } else if name_lower.starts_with(&query_lower) {
                80
            } else if name_lower.contains(&query_lower) {
                50
            } else if self.fuzzy_match(&name_lower, &query_lower) {
                30
            } else {
                continue;
            };
            for id in ids {
                if let Some(node) = self.nodes.get(id) {
                    results.push((node, score));
                }
            }
        }

        results.sort_by(|a, b| b.1.cmp(&a.1));
        results.truncate(top_k);
        results.into_iter().map(|(n, _)| n.clone()).collect()
    }

    /// Get callers of a symbol (who calls this)
    pub fn get_callers(&self, node_id: &str, max_depth: usize) -> Subgraph {
        self.traverse_edges(node_id, &EdgeKind::Calls, true, max_depth)
    }

    /// Get callees of a symbol (what this calls)
    pub fn get_callees(&self, node_id: &str, max_depth: usize) -> Subgraph {
        self.traverse_edges(node_id, &EdgeKind::Calls, false, max_depth)
    }

    /// Deep explore: get comprehensive context around a symbol
    pub fn explore(&self, query: &str, max_nodes: usize) -> Subgraph {
        let ql = query.to_lowercase();
        let terms: Vec<&str> = ql.split_whitespace().collect();

        // Multi-channel search
        let mut scores: HashMap<String, i32> = HashMap::new();

        for term in &terms {
            // Exact match
            if let Some(ids) = self.by_name.get(*term) {
                for id in ids {
                    *scores.entry(id.clone()).or_default() += 100;
                }
            }
            // Substring match
            for (name, ids) in &self.by_name {
                if name.to_lowercase().contains(*term) {
                    for id in ids {
                        *scores.entry(id.clone()).or_default() += 40;
                    }
                }
            }
        }

        // Sort by score
        let mut scored: Vec<(String, i32)> = scores.into_iter().collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1));
        let found_ids: Vec<String> = scored.into_iter().map(|(id, _)| id).take(max_nodes).collect();

        if found_ids.is_empty() {
            return Subgraph { nodes: vec![], edges: vec![], roots: vec![] };
        }

        let roots = found_ids.clone();

        // Expand with callers/callees (1 level)
        let mut expanded_ids: HashSet<String> = found_ids.into_iter().collect();
        let starting_ids = expanded_ids.clone();

        for id in &starting_ids {
            for edge in &self.edges {
                if edge.kind == EdgeKind::Calls {
                    if &edge.source_id == id && expanded_ids.len() < max_nodes {
                        expanded_ids.insert(edge.target_id.clone());
                    }
                    if &edge.target_id == id && expanded_ids.len() < max_nodes {
                        expanded_ids.insert(edge.source_id.clone());
                    }
                }
            }
            // Also include contained symbols (parents + children)
            for edge in &self.edges {
                if edge.kind == EdgeKind::Contains {
                    if &edge.source_id == id || &edge.target_id == id {
                        if expanded_ids.len() < max_nodes {
                            expanded_ids.insert(edge.source_id.clone());
                            expanded_ids.insert(edge.target_id.clone());
                        }
                    }
                }
            }
        }

        // Collect nodes
        let nodes: Vec<CodeNode> = expanded_ids.iter()
            .filter_map(|id| self.nodes.get(id).cloned())
            .collect();

        // Collect relevant edges
        let node_ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
        let edges: Vec<CodeEdge> = self.edges.iter()
            .filter(|e| node_ids.contains(e.source_id.as_str()) && node_ids.contains(e.target_id.as_str()))
            .cloned()
            .collect();

        Subgraph { nodes, edges, roots }
    }

    /// Get all symbols in a file
    pub fn get_file_symbols(&self, file_path: &str) -> Vec<CodeNode> {
        self.by_file.get(file_path)
            .map(|ids| ids.iter().filter_map(|id| self.nodes.get(id).cloned()).collect())
            .unwrap_or_default()
    }

}

// ========== Internal Methods ==========

impl CodeGraph {
    fn collect_files(&self, dir: &str, files: &mut Vec<String>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || skip_dir(&name) {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                self.collect_files(&path.to_string_lossy(), files);
            } else if let Some(path_str) = path.to_str() {
                if is_source_file(path_str) {
                    files.push(path_str.to_string());
                }
            }
        }
    }

    fn extract_symbols(&self, file_path: &str, content: &str, p: &LangPatterns, out: &mut Vec<ExtractedSymbol>) {
        let _lines: Vec<&str> = content.lines().collect();

        // Extract functions
        for caps in p.function.captures_iter(content) {
            let name = caps.get(2).or_else(|| caps.get(1)).map(|m| m.as_str()).unwrap_or("");
            if name.is_empty() || is_keyword(name) { continue; }
            let full_match = caps.get(0).map(|m| m.as_str()).unwrap_or("");
            let line_num = content[..caps.get(0).unwrap().start()].lines().count() as u32 + 1;
            out.push(ExtractedSymbol {
                name: name.to_string(),
                kind: NodeKind::Function,
                file_path: file_path.to_string(),
                start_line: line_num,
                end_line: line_num + 1,
                signature: full_match.lines().next().unwrap_or("").trim().to_string(),
                docstring: self.extract_docstring_before(content, caps.get(0).unwrap().start(), p),
                is_exported: self.check_export(content, caps.get(0).unwrap().start(), p),
                parent_name: None,
            });
        }

        // Extract classes/structs
        let class_re = &p.class;
        for caps in class_re.captures_iter(content) {
            let name = caps.get(2).or_else(|| caps.get(1)).map(|m| m.as_str()).unwrap_or("");
            if name.is_empty() || is_keyword(name) { continue; }
            let line_num = content[..caps.get(0).unwrap().start()].lines().count() as u32 + 1;
            let end_line = self.find_closing_brace(content, caps.get(0).unwrap().start());

            // Determine kind
            let kind = if p.interface.is_match(content) && content[..caps.get(0).unwrap().end()].contains("interface") {
                NodeKind::Interface
            } else if p.enum_def.is_match(content) && content[..caps.get(0).unwrap().end()].contains("enum") {
                NodeKind::Enum
            } else if p.struct_def.is_match(content) {
                NodeKind::Struct
            } else {
                NodeKind::Class
            };

            out.push(ExtractedSymbol {
                name: name.to_string(),
                kind,
                file_path: file_path.to_string(),
                start_line: line_num,
                end_line,
                signature: caps.get(0).map(|m| m.as_str()).unwrap_or("").lines().next().unwrap_or("").trim().to_string(),
                docstring: self.extract_docstring_before(content, caps.get(0).unwrap().start(), p),
                is_exported: self.check_export(content, caps.get(0).unwrap().start(), p),
                parent_name: None,
            });
        }

        // Extract variables/constants
        for caps in p.constant.captures_iter(content) {
            let name = caps.get(2).or_else(|| caps.get(1)).map(|m| m.as_str()).unwrap_or("");
            if name.is_empty() || name.len() == 1 || is_keyword(name) { continue; }
            let line_num = content[..caps.get(0).unwrap().start()].lines().count() as u32 + 1;
            out.push(ExtractedSymbol {
                name: name.to_string(),
                kind: if name.chars().all(|c| c.is_uppercase() || c == '_') { NodeKind::Constant } else { NodeKind::Variable },
                file_path: file_path.to_string(),
                start_line: line_num,
                end_line: line_num + 1,
                signature: caps.get(0).map(|m| m.as_str()).unwrap_or("").trim().to_string(),
                docstring: String::new(),
                is_exported: self.check_export(content, caps.get(0).unwrap().start(), p),
                parent_name: None,
            });
        }
    }

    fn extract_calls(&self, file_path: &str, content: &str, p: &LangPatterns, out: &mut Vec<ExtractedCall>) {
        let mut seen = HashSet::new();

        // Build a set of known function names from the file to filter noise
        let known_names: HashSet<&str> = p.function.captures_iter(content)
            .filter_map(|c| c.get(2).or_else(|| c.get(1)).map(|m| m.as_str()))
            .collect();

        for caps in p.call.captures_iter(content) {
            let name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            if name.is_empty() || is_keyword(name) || name.len() < 2 { continue; }
            if name.chars().next().map_or(true, |c| c.is_lowercase() || c == '_') == false && known_names.contains(name) == false {
                continue; // Skip likely type names
            }
            let line_num = content[..caps.get(0).unwrap().start()].lines().count() as u32 + 1;
            let key = format!("{}:{}:{}", line_num, file_path, name);
            if seen.contains(&key) { continue; }
            seen.insert(key);
            out.push(ExtractedCall {
                file_path: file_path.to_string(),
                _line: line_num,
                caller_name: self.find_enclosing_function(content, caps.get(0).unwrap().start()),
                callee_name: name.to_string(),
            });
        }
    }

    fn extract_imports(&self, file_path: &str, content: &str, p: &LangPatterns, out: &mut Vec<ExtractedImport>) {
        for caps in p.import.captures_iter(content) {
            let source = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            if source.is_empty() || source.len() > 200 { continue; }
            out.push(ExtractedImport {
                file_path: file_path.to_string(),
                _imported_name: String::new(),
                source_path: source.to_string(),
            });
        }
    }

    fn find_enclosing_function(&self, content: &str, pos: usize) -> String {
        let prefix = &content[..pos];
        // Walk backwards to find a function/class definition
        for line in prefix.lines().rev() {
            let trimmed = line.trim();
            if trimmed.starts_with("fn ") || trimmed.starts_with("def ") || trimmed.starts_with("func ") {
                if let Some(name) = trimmed.split_whitespace().nth(1) {
                    return name.trim_end_matches('(').to_string();
                }
            }
            if trimmed.starts_with("function ") {
                if let Some(name) = trimmed.split_whitespace().nth(1) {
                    return name.trim_end_matches('(').trim_end_matches('<').to_string();
                }
            }
        }
        // Fallback: file name
        String::new()
    }

    fn find_closing_brace(&self, content: &str, start: usize) -> u32 {
        let mut depth = 0;
        let mut started = false;
        let mut line_count = content[..start].lines().count() as u32 + 1;
        for ch in content[start..].chars() {
            if ch == '\n' { line_count += 1; }
            if ch == '{' { started = true; depth += 1; }
            if ch == '}' && started { depth -= 1; if depth == 0 { return line_count; } }
        }
        line_count
    }

    fn extract_docstring_before(&self, content: &str, pos: usize, p: &LangPatterns) -> String {
        let prefix = &content[..pos];
        if let Some(caps) = p.docstring.captures(prefix) {
            if let Some(m) = caps.get(0) {
                let doc = m.as_str().trim().to_string();
                if m.end() + 5 >= pos { // Docstring must be close to the definition
                    return doc.lines()
                        .map(|l| l.trim().trim_start_matches("///").trim_start_matches("//").trim_start_matches("/*").trim_start_matches('*').trim())
                        .collect::<Vec<_>>()
                        .join(" ");
                }
            }
        }
        // Python style: look for """...""" before def
        if let Some(triple) = prefix.rfind("\"\"\"") {
            if triple > 0 {
                if let Some(start) = prefix[..triple].rfind("\"\"\"") {
                    let doc = prefix[start..triple + 3].trim().to_string();
                    return doc.trim_matches('"').trim().to_string();
                }
            }
        }
        String::new()
    }

    fn check_export(&self, _content: &str, _pos: usize, p: &LangPatterns) -> bool {
        // Simple heuristic: check if the line contains export/pub keywords
        // More accurate check would look at the specific line
        p.export.is_match(_content)
    }

    fn add_node_indexes(&mut self, node: &CodeNode) {
        self.by_name.entry(node.name.clone()).or_default().push(node.id.clone());
        self.by_kind.entry(node.kind.clone()).or_default().push(node.id.clone());
        self.by_file.entry(node.file_path.clone()).or_default().push(node.id.clone());
    }

    fn traverse_edges(&self, node_id: &str, kind: &EdgeKind, incoming: bool, max_depth: usize) -> Subgraph {
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();
        let mut sub_nodes: Vec<CodeNode> = Vec::new();
        let mut sub_edges: Vec<CodeEdge> = Vec::new();

        queue.push_back((node_id.to_string(), 0));
        visited.insert(node_id.to_string());

        if let Some(node) = self.nodes.get(node_id) {
            sub_nodes.push(node.clone());
        }

        while let Some((current_id, depth)) = queue.pop_front() {
            if depth >= max_depth { continue; }

            for edge in &self.edges {
                if edge.kind != *kind { continue; }

                let neighbor_id = if incoming {
                    if edge.target_id == current_id { &edge.source_id } else { continue }
                } else {
                    if edge.source_id == current_id { &edge.target_id } else { continue }
                };

                sub_edges.push(edge.clone());

                if !visited.contains(neighbor_id) {
                    visited.insert(neighbor_id.clone());
                    if let Some(node) = self.nodes.get(neighbor_id) {
                        sub_nodes.push(node.clone());
                    }
                    queue.push_back((neighbor_id.clone(), depth + 1));
                }
            }
        }

        Subgraph { nodes: sub_nodes, edges: sub_edges, roots: vec![node_id.to_string()] }
    }

    fn resolve_import_path(&self, current_file: &str, import_path: &str) -> String {
        if import_path.starts_with('.') {
            // Relative import
            let parent = Path::new(current_file).parent().unwrap_or(Path::new("."));
            let resolved = parent.join(import_path);
            // Try common extensions
            for ext in &["ts", "tsx", "js", "jsx", "py", "rs", "go", "java"] {
                let with_ext = resolved.with_extension(ext);
                let path_str = with_ext.to_string_lossy().to_string();
                if self.files.contains(&path_str) {
                    return path_str;
                }
            }
            // Try index files
            for ext in &["ts", "js", "py"] {
                let index = resolved.join(format!("index.{}", ext));
                let path_str = index.to_string_lossy().to_string();
                if self.files.contains(&path_str) {
                    return path_str;
                }
            }
            resolved.to_string_lossy().to_string()
        } else {
            // Package import - try to resolve relative to project root
            import_path.to_string()
        }
    }

    fn fuzzy_match(&self, name: &str, query: &str) -> bool {
        let mut qi = query.chars().peekable();
        for nc in name.chars() {
            if let Some(&qc) = qi.peek() {
                if nc == qc {
                    qi.next();
                }
            }
        }
        qi.peek().is_none()
    }
}

fn is_keyword(s: &str) -> bool {
    matches!(s,
        // Control flow
        "if" | "else" | "for" | "while" | "return" | "break" | "continue"
        | "switch" | "case" | "default" | "try" | "catch" | "finally" | "throw"
        | "do" | "loop" | "match" | "when" | "goto"
        // Declarations
        | "new" | "delete" | "typeof" | "instanceof" | "in" | "of" | "as" | "from"
        | "async" | "await" | "yield" | "let" | "var" | "const" | "function"
        | "class" | "extends" | "super" | "this" | "self" | "static" | "pub" | "fn"
        | "struct" | "enum" | "impl" | "trait" | "type" | "interface" | "import"
        | "export" | "public" | "private" | "protected" | "void"
        | "lambda" | "with" | "unless" | "until"
        // Types
        | "int" | "bool" | "float" | "double" | "char" | "str" | "String" | "number"
        | "long" | "short" | "byte" | "synchronized" | "volatile" | "transient"
        | "native" | "strictfp"
        // Values
        | "true" | "false" | "None" | "null" | "undefined"
        | "Ok" | "Err" | "Some" | "pass"
        // Functions
        | "println" | "print" | "printf" | "echo" | "require" | "module" | "include"
        | "define" | "panic" | "assert" | "raise" | "toString" | "equals" | "hashCode"
        | "wait" | "notify" | "notifyAll"
        // SQL/DB
        | "select" | "insert" | "update" | "create" | "drop" | "alter" | "where" | "join" | "on"
        // Logical
        | "and" | "or" | "not" | "is"
        // Array/String methods
        | "map" | "filter" | "reduce" | "forEach" | "push" | "pop" | "shift"
        | "unshift" | "splice" | "slice" | "sort" | "reverse" | "concat" | "split"
        | "replace" | "indexOf" | "lastIndexOf" | "includes" | "startsWith"
        | "endsWith" | "trim" | "toLowerCase" | "toUpperCase" | "parseInt"
        | "parseFloat"
        // Promise
        | "then" | "resolve" | "reject" | "all" | "race"
        // Misc
        | "elif" | "else if" | "isinstanceof"
    )
}
