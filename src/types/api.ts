export interface Message {
  id: string
  role: 'user' | 'assistant'
  content: string
  thinking?: string
  timestamp?: number
}

export interface AgentMessage {
  from_agent: string
  to_agent: string
  message: string
  message_type: string
  created_at: number
  is_read: boolean
}

export interface AskOption {
  id: string
  text: string
}

export interface AskQuestion {
  id: string
  question: string
  options: AskOption[]
  multi_select: boolean
}

export interface AskRequest {
  questions: AskQuestion[]
}

export interface ChatResponse {
  id: string
  type: string
  role: string
  content: Array<{
    type: string
    text?: string
    [key: string]: any
  }>
  model: string
  stop_reason: string
  next_action?: string
}

export interface AgentConfig {
  apiKey: string | null
  model: string
  workspace: string
}