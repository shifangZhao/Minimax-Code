import { marked, Renderer } from 'marked'
import hljs from 'highlight.js'

// Strip dangerous HTML from rendered markdown output.
// Defense-in-depth against AI-generated XSS (script tags, event handlers, javascript: URIs).
function sanitizeHtml(html: string): string {
  return html
    .replace(/<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi, '')
    .replace(/<iframe\b[^<]*(?:(?!<\/iframe>)<[^<]*)*<\/iframe>/gi, '')
    .replace(/<object\b[^<]*(?:(?!<\/object>)<[^<]*)*<\/object>/gi, '')
    .replace(/<embed\b[^>]*>/gi, '')
    .replace(/\son\w+\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)/gi, '')
    .replace(/href\s*=\s*(?:"javascript:[^"]*"|'javascript:[^']*')/gi, 'href=""')
    .replace(/src\s*=\s*(?:"javascript:[^"]*"|'javascript:[^']*')/gi, 'src=""')
}

// Custom renderer with syntax highlighting
const renderer = new Renderer()

renderer.link = function({ href, title, text }: { href: string; title?: string | null; text: string }) {
  const titleAttr = title ? ` title="${title}"` : ''
  return `<a href="${href}"${titleAttr} target="_blank" rel="noopener noreferrer">${text}</a>`
}

renderer.code = function({ text, lang }: { text: string, lang?: string }) {
  const language = lang && hljs.getLanguage(lang) ? lang : 'plaintext'
  let highlighted: string
  try {
    highlighted = hljs.highlight(text, { language }).value
  } catch (e) {
    highlighted = text
  }
  return `<pre><code class="hljs language-${language}">${highlighted}</code></pre>`
}

marked.use({
  renderer,
  gfm: true,
  breaks: true,
})

export function renderMarkdown(content: string): string {
  if (!content) return ''
  try {
    return sanitizeHtml(marked.parse(content) as string)
  } catch (e) {
    console.error('Markdown parse error:', e)
    return escapeHtml(content)
  }
}

// Escape HTML to prevent XSS
export function escapeHtml(text: string): string {
  const map: Record<string, string> = {
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#039;',
  }
  return text.replace(/[&<>"']/g, m => map[m])
}

// Highlight a single code block with language detection
export function renderHighlightedBlock(code: string, lang: string): string {
  const language = lang && hljs.getLanguage(lang) ? lang : 'plaintext'
  let highlighted: string
  try {
    highlighted = hljs.highlight(code, { language }).value
  } catch {
    highlighted = escapeHtml(code)
  }
  return `<pre><code class="hljs language-${language}">${highlighted}</code></pre>`
}

// Render search/replace as a side-by-side diff with syntax highlighting
export function renderSearchReplace(search: string, replace: string, path: string): string {
  const lang = langFromPath(path)
  const aLines = search.split('\n')
  const bLines = replace.split('\n')

  // Build old content HTML
  let oldHtml = ''
  for (const line of aLines) {
    const escaped = escapeHtml(line)
    const highlighted = lang ? hlLine(escaped, lang) : escaped
    oldHtml += `<div class="diff-line">${highlighted || '&nbsp;'}</div>`
  }

  // Build new content HTML
  let newHtml = ''
  for (const line of bLines) {
    const escaped = escapeHtml(line)
    const highlighted = lang ? hlLine(escaped, lang) : escaped
    newHtml += `<div class="diff-line">${highlighted || '&nbsp;'}</div>`
  }

  return `<div class="diff-view">` +
    `<div class="diff-old">${oldHtml}</div>` +
    `<div class="diff-new">${newHtml}</div>` +
    `</div>`
}

// Render search/replace and return old and new HTML parts separately
export function renderSearchReplaceParts(search: string, replace: string, path: string): { oldHtml: string, newHtml: string } {
  const lang = langFromPath(path)
  const aLines = search.split('\n')
  const bLines = replace.split('\n')

  // Build old content HTML
  let oldHtml = ''
  for (const line of aLines) {
    const escaped = escapeHtml(line)
    const highlighted = lang ? hlLine(escaped, lang) : escaped
    oldHtml += `<div class="diff-line">${highlighted || '&nbsp;'}</div>`
  }

  // Build new content HTML
  let newHtml = ''
  for (const line of bLines) {
    const escaped = escapeHtml(line)
    const highlighted = lang ? hlLine(escaped, lang) : escaped
    newHtml += `<div class="diff-line">${highlighted || '&nbsp;'}</div>`
  }

  return {
    oldHtml: `<div class="diff-old">${oldHtml}</div>`,
    newHtml: `<div class="diff-new">${newHtml}</div>`
  }
}

// Infer language from file path
export function langFromPath(path: string): string {
  const ext = path.split('.').pop()?.toLowerCase() || ''
  const langs: Record<string, string> = {
    ts: 'typescript', tsx: 'tsx', js: 'javascript', jsx: 'jsx',
    py: 'python', rs: 'rust', go: 'go', java: 'java', c: 'c',
    cpp: 'cpp', h: 'c', hpp: 'cpp', cs: 'csharp',
    vue: 'vue', svelte: 'svelte', json: 'json', yaml: 'yaml',
    toml: 'toml', md: 'markdown', sql: 'sql', sh: 'bash',
    css: 'css', html: 'html', xml: 'xml',
  }
  return langs[ext] || 'plaintext'
}

// Highlight a single line of code (for diff views)
export function hlLine(line: string, lang: string): string {
  if (!line.trim()) return ''
  try {
    if (lang && hljs.getLanguage(lang)) {
      return hljs.highlight(line, { language: lang }).value
    }
  } catch {}
  return escapeHtml(line)
}

// Parse code blocks from markdown
export interface CodeBlock {
  language: string
  code: string
}

export function extractCodeBlocks(content: string): CodeBlock[] {
  const blocks: CodeBlock[] = []
  const codeBlockRegex = /```(\w*)\n?([\s\S]*?)```/g
  let match

  while ((match = codeBlockRegex.exec(content)) !== null) {
    blocks.push({
      language: match[1] || 'plaintext',
      code: match[2].trim(),
    })
  }

  return blocks
}