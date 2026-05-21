import { invoke } from '@tauri-apps/api/core'

export interface GroupChat {
  id: number
  name: string
  created_at: string
}

export interface AgentSession {
  id: number
  group_chat_id: number
  agent_type: string
  created_at: string
}

export interface ChatMessage {
  id: number
  session_id: number
  role: string
  content: string
  thinking?: string
  tool_calls?: string
  attachments?: string  // JSON array of {name, path, kind}
  raw_json?: string  // full JSON of content block array for cache-aware reconstruction
  created_at: string
}

export const db = {
  async createGroupChat(name: string): Promise<number> {
    return invoke('create_group_chat', { name })
  },

  async getGroupChats(): Promise<GroupChat[]> {
    return invoke('get_group_chats')
  },

  async deleteGroupChat(id: number): Promise<void> {
    return invoke('delete_group_chat', { id })
  },

  async renameGroupChat(id: number, name: string): Promise<void> {
    return invoke('rename_group_chat', { id, name })
  },

  async createAgentSession(groupChatId: number, agentType: string): Promise<number> {
    return invoke('create_agent_session', { groupChatId, agentType })
  },

  async getAgentSessions(groupChatId: number, agentType?: string): Promise<AgentSession[]> {
    const params: Record<string, any> = { groupChatId }
    if (agentType !== undefined) {
      params.agentType = agentType
    }
    return invoke('get_agent_sessions', params)
  },

  async addMessage(sessionId: number, role: string, content: string, toolCalls?: string, thinking?: string, attachments?: string, rawJson?: string): Promise<number> {
    return invoke('add_message', { sessionId, role, content, toolCalls: toolCalls || null, thinking: thinking || null, attachments: attachments || null, rawJson: rawJson || null })
  },

  async getMessages(sessionId: number): Promise<ChatMessage[]> {
    return invoke('get_messages', { sessionId })
  },

  async deleteMessage(id: number): Promise<void> {
    return invoke('delete_message', { id })
  },

  async clearSessionHistory(sessionId: number): Promise<void> {
    return invoke('clear_session_history', { sessionId })
  },

  async readFile(path: string): Promise<string> {
    return invoke('read_file', { path })
  },

  async writeFile(path: string, content: string): Promise<void> {
    return invoke('write_file', { path, content })
  },

  async listDir(path: string): Promise<Array<{ name: string; path: string; is_dir: boolean }>> {
    return invoke('list_dir', { path })
  },

  async createDir(path: string): Promise<void> {
    return invoke('create_dir', { path })
  },

  async removePath(path: string): Promise<void> {
    return invoke('remove_path', { path })
  },

  async runCommand(command: string, cwd?: string): Promise<{ stdout: string; stderr: string; exit_code: number }> {
    return invoke('run_command', { command, cwd })
  },

  async gitStatus(repoPath: string): Promise<{ stdout: string; stderr: string; exit_code: number }> {
    return invoke('git_status', { repoPath })
  },

  async gitLog(repoPath: string, count?: number): Promise<{ stdout: string; stderr: string; exit_code: number }> {
    return invoke('git_log', { repoPath, count })
  },

  async gitDiff(repoPath: string, target?: string): Promise<{ stdout: string; stderr: string; exit_code: number }> {
    return invoke('git_diff', { repoPath, target })
  },

  async gitBranch(repoPath: string): Promise<{ stdout: string; stderr: string; exit_code: number }> {
    return invoke('git_branch', { repoPath })
  },

  async gitCheckout(repoPath: string, branch: string): Promise<{ stdout: string; stderr: string; exit_code: number }> {
    return invoke('git_checkout', { repoPath, branch })
  },

  async gitCommit(repoPath: string, message: string): Promise<{ stdout: string; stderr: string; exit_code: number }> {
    return invoke('git_commit', { repoPath, message })
  },

  async gitStash(repoPath: string): Promise<{ stdout: string; stderr: string; exit_code: number }> {
    return invoke('git_stash', { repoPath })
  },

  async gitStashPop(repoPath: string): Promise<{ stdout: string; stderr: string; exit_code: number }> {
    return invoke('git_stash_pop', { repoPath })
  },

  async searchInDir(path: string, pattern: string, fileFilter?: string): Promise<Array<{ file: string; lines: Array<{ line_num: number; content: string }> }>> {
    return invoke('search_in_dir', { path, pattern, fileFilter })
  },

  async getEnvInfo(repoPath: string): Promise<{
    system: string; arch: string; node_version?: string; npm_version?: string;
    python_version?: string; rust_version?: string; cargo_version?: string;
    has_git: boolean; package_manager?: string;
  }> {
    return invoke('get_env_info', { repoPath })
  },

  async analyzeProjectStructure(repoPath: string): Promise<{
    root_files: string[]; src_dirs: string[]; config_files: string[];
    has_tests: boolean; is_monorepo: boolean;
  }> {
    return invoke('analyze_project_structure', { repoPath })
  },

  async readFiles(paths: string[]): Promise<Array<{ path: string; content: string; success: boolean; error?: string }>> {
    return invoke('read_files', { paths })
  },

  async writeFiles(files: Array<{ path: string; content: string }>): Promise<Array<{ path: string; success: boolean; error?: string }>> {
    return invoke('write_files', { files })
  },

  async spawnProcess(command: string, cwd?: string): Promise<number> {
    return invoke('spawn_process', { command, cwd })
  },

  async killProcess(pid: number): Promise<void> {
    return invoke('kill_process', { pid })
  },

  async findReplaceInFiles(dir: string, find: string, replace: string, fileFilter?: string, useRegex?: boolean): Promise<Array<{ file: string; replacements: number }>> {
    return invoke('find_replace_in_files', { dir, find, replace, fileFilter, useRegex })
  },

  async applyPatch(repoPath: string, patchContent: string): Promise<{ stdout: string; stderr: string; exit_code: number }> {
    return invoke('apply_patch', { repoPath, patchContent })
  },

  async createPatch(repoPath: string, target?: string, outputPath?: string): Promise<string> {
    return invoke('create_patch', { repoPath, target, outputPath })
  },

  async runTests(repoPath: string, testFramework: string): Promise<{ passed: number; failed: number; total: number; duration_ms: number; output: string }> {
    return invoke('run_tests', { repoPath, testFramework })
  },

  async modifyFiles(files: Array<{ path: string; new_content?: string; replacements: Array<{ find: string; replace: string }> }>): Promise<Array<{ path: string; success: boolean; error?: string }>> {
    return invoke('modify_files', { files })
  },
}