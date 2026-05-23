// Skill Service - Rust Implementation for Skill Management
//
// Supports loading skills from multiple sources:
// - Built-in: <app>/skills/
// - Global: ~/.minimaxcode/skills/
// - Project: <workspace>/skills/

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::process::Command;

fn hidden_cmd(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program.as_ref());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock as StdRwLock;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub allowed_tools: Vec<String>,
    pub version: String,
    pub source: String,
    pub path: String,
    pub scripts: Vec<String>,
    pub references: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMatch {
    pub name: String,
    pub score: f32,
    pub description: String,
    pub source: String,
}

pub struct SkillService {
    skills: Arc<RwLock<HashMap<String, Skill>>>,
    global_root: PathBuf,
    builtin_root: StdRwLock<Option<PathBuf>>,
}

impl SkillService {
    pub fn new() -> Self {
        let global_root = dirs::home_dir()
            .unwrap_or_default()
            .join(".minimaxcode")
            .join("skills");

        Self {
            skills: Arc::new(RwLock::new(HashMap::new())),
            global_root,
            builtin_root: StdRwLock::new(None),
        }
    }

    pub fn set_builtin_root(&self, path: PathBuf) {
        *self.builtin_root.write().unwrap() = Some(path);
    }

    pub async fn load_all_skills(&self) {
        let mut skills = self.skills.write().await;
        skills.clear();

        if self.global_root.exists() {
            self.load_skills_from_dir(&self.global_root, "global", &mut skills);
        }

        if let Some(ref builtin) = *self.builtin_root.read().unwrap() {
            if builtin.exists() {
                self.load_skills_from_dir(builtin, "builtin", &mut skills);
            }
        }
    }

    pub async fn load_project_skills(&self, project_path: &str) {
        let project_root = PathBuf::from(project_path).join("skills");
        if !project_root.exists() {
            return;
        }

        let mut skills = self.skills.write().await;
        self.load_skills_from_dir(&project_root, "project", &mut skills);
    }

    fn load_skills_from_dir(&self, root: &PathBuf, source: &str, skills: &mut HashMap<String, Skill>) {
        if !root.exists() {
            return;
        }

        let entries = match std::fs::read_dir(root) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let skill_md = path.join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }

            if let Ok(skill) = self.parse_skill(&path, source) {
                if !skills.contains_key(&skill.name) {
                    skills.insert(skill.name.clone(), skill);
                }
            }
        }
    }

    fn parse_skill(&self, path: &PathBuf, source: &str) -> Result<Skill, Box<dyn std::error::Error>> {
        let skill_md = path.join("SKILL.md");
        let content = std::fs::read_to_string(&skill_md)?;

        let name = path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let mut description = String::new();
        let allowed_tools = Vec::new();
        let mut version = "1.0.0".to_string();

        // Parse YAML frontmatter
        if let Some(start) = content.find("---") {
            if let Some(end) = content[2..].find("---") {
                let frontmatter = &content[start + 3..end + 2];
                for line in frontmatter.lines() {
                    if line.starts_with("description:") {
                        description = line.split(':').nth(1).map(|s| s.trim().to_string()).unwrap_or_default();
                    } else if line.starts_with("version:") {
                        version = line.split(':').nth(1).map(|s| s.trim().to_string()).unwrap_or_else(|| "1.0.0".to_string());
                    }
                }
            }
        }

        let scripts = self.list_dir_files(&path.join("scripts"));
        let references = self.list_dir_files(&path.join("references"));

        Ok(Skill {
            name,
            description,
            allowed_tools,
            version,
            source: source.to_string(),
            path: path.to_string_lossy().to_string(),
            scripts,
            references,
        })
    }

    fn list_dir_files(&self, dir: &PathBuf) -> Vec<String> {
        if !dir.exists() {
            return Vec::new();
        }
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_file())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn list_skills(&self, source_filter: Option<&str>) -> Vec<Skill> {
        let skills = self.skills.read().await;
        match source_filter {
            Some(source) => skills.values()
                .filter(|s| s.source == source)
                .cloned()
                .collect(),
            // Default: hide builtin skills from the list.
            // Builtins are discovered via automatic matching and loaded on-demand.
            None => skills.values()
                .filter(|s| s.source != "builtin")
                .cloned()
                .collect(),
        }
    }

    pub async fn get_skill(&self, name: &str) -> Option<Skill> {
        let skills = self.skills.read().await;
        skills.get(name).cloned()
    }

    pub async fn get_skill_content(&self, name: &str) -> Option<String> {
        let skills = self.skills.read().await;
        if let Some(skill) = skills.get(name) {
            let skill_md = PathBuf::from(&skill.path).join("SKILL.md");
            if let Ok(content) = std::fs::read_to_string(&skill_md) {
                if let Some(_start) = content.find("---") {
                    if let Some(end) = content[2..].find("---") {
                        return Some(content[end + 5..].trim().to_string());
                    }
                }
                return Some(content);
            }
        }
        None
    }

    pub async fn match_skills(&self, query: &str, top_k: usize) -> Vec<SkillMatch> {
        let skills = self.skills.read().await;
        let query_lower = query.to_lowercase();

        let mut scores: Vec<SkillMatch> = skills.values()
            .filter_map(|skill| {
                let name_lower = skill.name.to_lowercase();
                let desc_lower = skill.description.to_lowercase();

                let score = score_skill(&query_lower, &name_lower, &desc_lower);

                if score > 0.0 {
                    Some(SkillMatch {
                        name: skill.name.clone(),
                        score,
                        description: skill.description.clone(),
                        source: skill.source.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        scores.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(top_k);
        scores
    }

pub async fn execute_skill(&self, skill_name: &str, script_name: Option<&str>) -> Result<String, String> {
        let skills = self.skills.read().await;
        let skill = skills.get(skill_name).ok_or_else(|| format!("Skill not found: {}", skill_name))?;

        let skill_path = PathBuf::from(&skill.path);
        let scripts_dir = skill_path.join("scripts");

        match script_name {
            Some(script) => {
                let script_path = scripts_dir.join(script);
                if !script_path.exists() {
                    for ext in &[".py", ".sh", ".js", ".ts"] {
                        let candidate = scripts_dir.join(format!("{}{}", script, ext));
                        if candidate.exists() {
                            return self.run_script(&candidate);
                        }
                    }
                    Err(format!("Script not found: {}", script))
                } else {
                    self.run_script(&script_path)
                }
            }
            None => {
                let skill_md = skill_path.join("SKILL.md");
                std::fs::read_to_string(&skill_md)
                    .map_err(|e| e.to_string())
            }
        }
    }

    fn run_script(&self, script_path: &PathBuf) -> Result<String, String> {
        let ext = script_path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let output = match ext {
            "py" => hidden_cmd("python")
                .arg(script_path)
                .output(),
            "sh" => {
                let shell = if cfg!(windows) { "bash" } else { "sh" };
                hidden_cmd(shell)
                    .arg(script_path)
                    .output()
            }
            _ => hidden_cmd(script_path)
                .output(),
        };

        match output {
            Ok(result) => {
                if result.status.success() {
                    Ok(String::from_utf8_lossy(&result.stdout).to_string())
                } else {
                    Err(String::from_utf8_lossy(&result.stderr).to_string())
                }
            }
            Err(e) => Err(e.to_string()),
        }
    }
}

// ===== matching helpers (standalone, not in impl) =====

fn score_skill(query: &str, name: &str, desc: &str) -> f32 {
    if name == query { return 1.0; }
    if name.contains(query) || query.contains(name) { return 0.95; }
    if desc.contains(query) { return 0.85; }

    let query_tokens = tokenize(query);
    let name_tokens = tokenize(name);
    let desc_tokens = tokenize(desc);

    let name_score = token_jaccard(&query_tokens, &name_tokens);
    let desc_score = token_jaccard(&query_tokens, &desc_tokens);

    let q_bigrams = bigrams(query);
    let d_bigrams = bigrams(desc);
    let n_bigrams = bigrams(name);
    let bg_desc = bigram_jaccard(&q_bigrams, &d_bigrams);
    let bg_name = bigram_jaccard(&q_bigrams, &n_bigrams);

    let name_combined = f32::max(name_score, bg_name * 1.2);
    let desc_combined = f32::max(desc_score, bg_desc);

    let score = name_combined * 0.35 + desc_combined * 0.65;

    let has_token_hit = query_tokens.iter().any(|t| desc.contains(t.as_str()));
    let boost = if has_token_hit { 1.15 } else { 1.0 };

    (score * boost).min(0.84)
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_whitespace() || ch == '-' || ch == '_' || ch == '/' {
            if !current.is_empty() { tokens.push(current.clone()); current.clear(); }
        } else if ch.is_ascii_alphabetic() || ch.is_ascii_digit() {
            current.push(ch.to_ascii_lowercase());
        } else {
            if !current.is_empty() { tokens.push(current.clone()); current.clear(); }
            tokens.push(ch.to_string());
        }
    }
    if !current.is_empty() { tokens.push(current); }
    tokens
}

fn token_jaccard(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() { return 0.0; }
    let set_a: HashSet<&String> = a.iter().collect();
    let set_b: HashSet<&String> = b.iter().collect();
    let common = set_a.intersection(&set_b).count() as f32;
    let union = set_a.union(&set_b).count() as f32;
    common / union
}

fn bigrams(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 2 { return chars.iter().map(|c| c.to_string()).collect(); }
    chars.windows(2).map(|w| w.iter().collect()).collect()
}

fn bigram_jaccard(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() { return 0.0; }
    let set_a: HashSet<&String> = a.iter().collect();
    let set_b: HashSet<&String> = b.iter().collect();
    let common = set_a.intersection(&set_b).count() as f32;
    let union = set_a.union(&set_b).count() as f32;
    common / union
}

impl Default for SkillService {
    fn default() -> Self {
        Self::new()
    }
}