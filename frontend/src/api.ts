import type {
  ConnectionSaveResponse,
  ConnectionTestResult,
  ConnectionUpdate,
  Run,
  Scan,
  Settings,
  Status,
  SyncProgress,
  SyncStartResponse,
} from './types'

export class ApiError extends Error {
  status: number

  constructor(message: string, status: number) {
    super(message)
    this.name = 'ApiError'
    this.status = status
  }
}

async function request<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await fetch(url, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers || {}),
    },
  })
  const body = await response.json().catch(() => ({}))
  if (!response.ok || body?.ok === false) {
    throw new ApiError(body?.error || `HTTP ${response.status}`, response.status)
  }
  return body as T
}

export const api = {
  status: () => request<Status>('/api/status'),
  settings: () => request<Settings>('/api/settings'),
  saveSettings: (value: Settings) =>
    request<{ ok: true; message: string; settings: Settings }>('/api/settings', {
      method: 'PUT',
      body: JSON.stringify(value),
    }),
  saveConnections: (value: ConnectionUpdate) =>
    request<ConnectionSaveResponse>('/api/connections', {
      method: 'PUT',
      body: JSON.stringify(value),
    }),
  models: () => request<{ scan?: Scan | null }>('/api/models'),
  history: () => request<{ runs: Run[] }>('/api/history'),
  scan: () => request<{ ok: true; scan: Scan }>('/api/scan', { method: 'POST', body: '{}' }),
  sync: (force = false) =>
    request<{ ok: true; run: Run }>('/api/sync', {
      method: 'POST',
      body: JSON.stringify({ force }),
    }),
  startSync: (force = false) =>
    request<SyncStartResponse>('/api/sync/start', {
      method: 'POST',
      body: JSON.stringify({ force }),
    }),
  syncProgress: (runId: string) => request<SyncProgress>(`/api/sync/${encodeURIComponent(runId)}`),
  testOpenRouter: () =>
    request<ConnectionTestResult>('/api/test/openrouter', { method: 'POST', body: '{}' }),
  testNewApi: () =>
    request<ConnectionTestResult>('/api/test/newapi', { method: 'POST', body: '{}' }),
}
