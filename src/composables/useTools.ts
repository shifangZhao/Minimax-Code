import { invoke } from '@tauri-apps/api/core'
import { db } from '../services/db'

function errorMsg(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}

export interface ToolResult {
  success: boolean
  data?: unknown
  error?: string
}

// ========== File Tools ==========

export const fileTools = {
  async readFile(path: string): Promise<ToolResult> {
    try {
      const content = await db.readFile(path)
      return { success: true, data: content }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },

  async writeFile(path: string, content: string): Promise<ToolResult> {
    try {
      await db.writeFile(path, content)
      return { success: true, data: `Written: ${path}` }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },

  async readFiles(paths: string[]): Promise<ToolResult> {
    try {
      const results = await db.readFiles(paths)
      return { success: true, data: results }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },

  async writeFiles(files: Array<{ path: string; content: string }>): Promise<ToolResult> {
    try {
      const results = await db.writeFiles(files)
      return { success: true, data: results }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },

  async listDir(path: string): Promise<ToolResult> {
    try {
      const entries = await db.listDir(path)
      const formatted = entries.map(e => `${e.is_dir ? '[DIR]' : '[FILE]'} ${e.name}`)
      return { success: true, data: formatted.join('\n') }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },

  async createDir(path: string): Promise<ToolResult> {
    try {
      await db.createDir(path)
      return { success: true, data: `Created: ${path}` }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },

  async removePath(path: string): Promise<ToolResult> {
    try {
      await db.removePath(path)
      return { success: true, data: `Removed: ${path}` }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },
}

// ========== Git Tools ==========

export const gitTools = {
  async status(repoPath: string): Promise<ToolResult> {
    try {
      const result = await db.gitStatus(repoPath)
      return { success: result.exit_code === 0, data: result.stdout || result.stderr }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },

  async log(repoPath: string, count?: number): Promise<ToolResult> {
    try {
      const result = await db.gitLog(repoPath, count)
      return { success: result.exit_code === 0, data: result.stdout || result.stderr }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },

  async diff(repoPath: string, target?: string): Promise<ToolResult> {
    try {
      const result = await db.gitDiff(repoPath, target)
      return { success: result.exit_code === 0, data: result.stdout || result.stderr }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },

  async branch(repoPath: string): Promise<ToolResult> {
    try {
      const result = await db.gitBranch(repoPath)
      return { success: result.exit_code === 0, data: result.stdout || result.stderr }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },

  async checkout(repoPath: string, branch: string): Promise<ToolResult> {
    try {
      const result = await db.gitCheckout(repoPath, branch)
      return { success: result.exit_code === 0, data: result.stdout || result.stderr }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },

  async commit(repoPath: string, message: string): Promise<ToolResult> {
    try {
      const result = await db.gitCommit(repoPath, message)
      return { success: result.exit_code === 0, data: result.stdout || result.stderr }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },

  async stash(repoPath: string): Promise<ToolResult> {
    try {
      const result = await db.gitStash(repoPath)
      return { success: result.exit_code === 0, data: result.stdout || result.stderr }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },

  async stashPop(repoPath: string): Promise<ToolResult> {
    try {
      const result = await db.gitStashPop(repoPath)
      return { success: result.exit_code === 0, data: result.stdout || result.stderr }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },
}

// ========== Search Tools ==========

export const searchTools = {
  async grep(path: string, pattern: string, fileFilter?: string): Promise<ToolResult> {
    try {
      const matches = await db.searchInDir(path, pattern, fileFilter)
      const formatted = matches.map(m => {
        const lines = m.lines.map(l => `  ${l.line_num}: ${l.content}`).join('\n')
        return `${m.file}\n${lines}`
      }).join('\n\n')
      return { success: true, data: formatted || 'No matches found' }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },
}

// ========== Env & Project Tools ==========

export const envTools = {
  async getInfo(repoPath: string): Promise<ToolResult> {
    try {
      const info = await db.getEnvInfo(repoPath)
      return { success: true, data: info }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },

  async analyzeStructure(repoPath: string): Promise<ToolResult> {
    try {
      const structure = await db.analyzeProjectStructure(repoPath)
      return { success: true, data: structure }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },
}

// ========== Process Tools ==========

export const processTools = {
  async spawn(command: string, cwd?: string): Promise<ToolResult> {
    try {
      const pid = await db.spawnProcess(command, cwd)
      return { success: true, data: { pid } }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },

  async kill(pid: number): Promise<ToolResult> {
    try {
      await db.killProcess(pid)
      return { success: true, data: `Killed process ${pid}` }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },
}

// ========== Terminal Tool ==========

export const terminalTools = {
  async run(command: string, cwd?: string): Promise<ToolResult> {
    try {
      const result = await db.runCommand(command, cwd)
      return {
        success: result.exit_code === 0,
        data: result.stdout || result.stderr,
        error: result.exit_code !== 0 ? `Exit ${result.exit_code}` : undefined,
      }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },
}

// ========== Find & Replace Tools ==========

export const replaceTools = {
  async findReplace(dir: string, find: string, replace: string, fileFilter?: string, useRegex?: boolean): Promise<ToolResult> {
    try {
      const results = await db.findReplaceInFiles(dir, find, replace, fileFilter, useRegex)
      const total = results.reduce((sum, r) => sum + r.replacements, 0)
      return { success: true, data: `${results.length} files modified, ${total} replacements` }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },
}

// ========== Patch / Diff Tools ==========

export const patchTools = {
  async apply(repoPath: string, patchContent: string): Promise<ToolResult> {
    try {
      const result = await db.applyPatch(repoPath, patchContent)
      return {
        success: result.exit_code === 0,
        data: result.stdout || result.stderr,
        error: result.exit_code !== 0 ? `Exit ${result.exit_code}` : undefined,
      }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },

  async create(repoPath: string, target?: string, outputPath?: string): Promise<ToolResult> {
    try {
      const patch = await db.createPatch(repoPath, target, outputPath)
      return { success: true, data: patch }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },
}

// ========== Test Tools ==========

export const testTools = {
  async run(repoPath: string, framework: 'jest' | 'pytest' | 'cargo' | 'npm'): Promise<ToolResult> {
    try {
      const result = await db.runTests(repoPath, framework)
      const summary = `${result.passed} passed, ${result.failed} failed, ${result.total} total`
      return {
        success: result.failed === 0,
        data: `${summary}\n\n${result.output}`,
        error: result.failed > 0 ? summary : undefined,
      }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },
}

// ========== Code Modification Tools ==========

export const codeModTools = {
  async modify(files: Array<{ path: string; new_content?: string; replacements: Array<{ find: string; replace: string }> }>): Promise<ToolResult> {
    try {
      const results = await db.modifyFiles(files)
      const succeeded = results.filter(r => r.success).length
      return {
        success: succeeded === results.length,
        data: `${succeeded}/${results.length} files modified`,
        error: succeeded < results.length ? results.filter(r => !r.success).map(r => r.error).join(', ') : undefined,
      }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },
}

// ========== Web Search Tools ==========

export const webSearchTools = {
  async search(query: string): Promise<ToolResult> {
    try {
      const result = await invoke<{ results: Array<{ title: string; link: string; snippet: string }> }>('web_search', { query })
      const formatted = result.results.map(r => `[${r.title}](${r.link})\n  ${r.snippet}`).join('\n\n')
      return {
        success: true,
        data: formatted || 'No results found',
      }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },
}

// ========== Image Understanding Tools ==========

export const imageTools = {
  async understand(imageUrl: string, prompt: string): Promise<ToolResult> {
    try {
      const result = await invoke<string>('understand_image', { prompt, imageUrl })
      return {
        success: true,
        data: result,
      }
    } catch (e) {
      return { success: false, error: errorMsg(e) }
    }
  },
}

// ========== Unified Tools Export ==========

export const tools = {
  file: fileTools,
  git: gitTools,
  search: searchTools,
  env: envTools,
  process: processTools,
  terminal: terminalTools,
  replace: replaceTools,
  patch: patchTools,
  test: testTools,
  codeMod: codeModTools,
  webSearch: webSearchTools,
  image: imageTools,
}

export function useTools() {
  return { tools }
}