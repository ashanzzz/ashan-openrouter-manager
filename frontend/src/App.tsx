import { useEffect, useMemo, useState, type Dispatch, type ReactNode, type SetStateAction } from 'react'
import { api } from './api'
import type {
  ConnectionCheck,
  RankedModel,
  Run,
  Scan,
  Settings,
  Status,
} from './types'

type Page = 'overview' | 'models' | 'history' | 'settings'
type BusyAction = '' | 'scan' | 'sync' | 'save-connections' | 'save-settings' | 'test-openrouter' | 'test-newapi'
type ToastTone = 'success' | 'error' | 'warning' | 'info'

type Toast = {
  tone: ToastTone
  title: string
  detail?: string
}

type SecretDraft = {
  openrouter_api_key: string
  newapi_admin_token: string
  newapi_test_token: string
}

const fmt = (value?: string | null) => (value ? new Date(value).toLocaleString() : '—')
const metric = (value?: number | null) => (value == null ? '—' : value.toFixed(1))
const normalizeUrl = (value: string) => value.trim().replace(/\/+$/, '')

function Badge({ children, tone = 'muted' }: { children: ReactNode; tone?: string }) {
  return <span className={`badge ${tone}`}>{children}</span>
}

function Spinner() {
  return <span className="spinner" aria-hidden="true" />
}

function ButtonContent({ busy, idle, loading }: { busy: boolean; idle: string; loading: string }) {
  return busy ? (
    <>
      <Spinner /> {loading}
    </>
  ) : (
    <>{idle}</>
  )
}

function ToastBanner({ toast, onClose }: { toast: Toast; onClose: () => void }) {
  return (
    <div className={`toast ${toast.tone}`} role="status" aria-live="polite">
      <div className="toast-icon">{toast.tone === 'success' ? '✓' : toast.tone === 'error' ? '!' : 'i'}</div>
      <div className="toast-copy">
        <strong>{toast.title}</strong>
        {toast.detail && <span>{toast.detail}</span>}
      </div>
      <button className="toast-close" onClick={onClose} aria-label="关闭提示">
        ×
      </button>
    </div>
  )
}

function ModelTable({ models }: { models: RankedModel[] }) {
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            <th>#</th>
            <th>模型</th>
            <th>Intelligence</th>
            <th>Coding</th>
            <th>Agentic</th>
            <th>Context</th>
            <th>测试</th>
          </tr>
        </thead>
        <tbody>
          {models.map((model) => (
            <tr key={model.id}>
              <td>{model.rank}</td>
              <td>
                <strong>{model.name}</strong>
                <small>{model.id}</small>
              </td>
              <td>{metric(model.intelligence_index)}</td>
              <td>{metric(model.coding_index)}</td>
              <td>{metric(model.agentic_index)}</td>
              <td>{Math.round(model.context_length / 1024)}K</td>
              <td>
                {model.usable === true ? (
                  <Badge tone="ok">可用</Badge>
                ) : model.usable === false ? (
                  <Badge tone="bad">失败</Badge>
                ) : (
                  <Badge>未测试</Badge>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function connectionPresentation(check: ConnectionCheck, configured: boolean) {
  if (!configured) return { state: 'unconfigured', label: '未配置', detail: '请先完成连接配置' }
  if (check.state === 'connected') {
    return {
      state: 'connected',
      label: '已连接',
      detail: [check.detail, check.latency_ms != null ? `${check.latency_ms} ms` : null].filter(Boolean).join(' · '),
    }
  }
  if (check.state === 'failed') return { state: 'failed', label: '连接失败', detail: check.message }
  return { state: 'configured', label: '已配置 · 未测试', detail: '建议执行一次连接测试' }
}

function ConnectionStatus({ check, configured, compact = false }: { check: ConnectionCheck; configured: boolean; compact?: boolean }) {
  const view = connectionPresentation(check, configured)
  return (
    <div className={`connection-status ${view.state} ${compact ? 'compact' : ''}`}>
      <span className="status-dot" />
      <div>
        <strong>{view.label}</strong>
        {!compact && view.detail && <span>{view.detail}</span>}
      </div>
    </div>
  )
}

function rulesSnapshot(settings: Settings) {
  const { newapi_base_url: _base, newapi_admin_user_id: _user, ...rules } = settings
  return JSON.stringify(rules)
}

export default function App() {
  const [page, setPage] = useState<Page>('overview')
  const [status, setStatus] = useState<Status | null>(null)
  const [savedSettings, setSavedSettings] = useState<Settings | null>(null)
  const [settings, setSettings] = useState<Settings | null>(null)
  const [scan, setScan] = useState<Scan | null>(null)
  const [runs, setRuns] = useState<Run[]>([])
  const [busy, setBusy] = useState<BusyAction>('')
  const [toast, setToast] = useState<Toast | null>(null)
  const [loadError, setLoadError] = useState('')
  const [secrets, setSecrets] = useState<SecretDraft>({
    openrouter_api_key: '',
    newapi_admin_token: '',
    newapi_test_token: '',
  })

  const refreshRuntime = async () => {
    const [nextStatus, models, history] = await Promise.all([api.status(), api.models(), api.history()])
    setStatus(nextStatus)
    setScan(models.scan || null)
    setRuns(history.runs || [])
  }

  const initialLoad = async () => {
    const [nextStatus, nextSettings, models, history] = await Promise.all([
      api.status(),
      api.settings(),
      api.models(),
      api.history(),
    ])
    setStatus(nextStatus)
    setSavedSettings(nextSettings)
    setSettings(nextSettings)
    setScan(models.scan || null)
    setRuns(history.runs || [])
  }

  const loadApplication = async () => {
    setLoadError('')
    try {
      await initialLoad()
    } catch (error: any) {
      setLoadError(error.message || '无法连接应用后端')
    }
  }

  useEffect(() => {
    void loadApplication()
  }, [])

  useEffect(() => {
    if (!toast) return
    const timer = window.setTimeout(() => setToast(null), 6000)
    return () => window.clearTimeout(timer)
  }, [toast])

  const runScan = async () => {
    setBusy('scan')
    try {
      const response = await api.scan()
      setScan(response.scan)
      setToast({
        tone: 'success',
        title: '检查完成',
        detail: `已扫描 ${response.scan.total_models} 个模型，选出 ${response.scan.selected.length} 个可用模型`,
      })
      await refreshRuntime()
    } catch (error: any) {
      setToast({ tone: 'error', title: '检查失败', detail: error.message })
    } finally {
      setBusy('')
    }
  }

  const runSync = async () => {
    setBusy('sync')
    try {
      const response = await api.sync(false)
      setToast({
        tone: 'success',
        title: response.run.changed ? '同步完成' : '无需更新',
        detail: response.run.changed ? 'New API 的 3 条受管渠道已更新并验证' : '当前 Top 3 与线上配置一致',
      })
      await refreshRuntime()
    } catch (error: any) {
      setToast({ tone: 'error', title: '同步失败', detail: error.message })
    } finally {
      setBusy('')
    }
  }

  if (!status || !settings || !savedSettings) {
    return (
      <div className="loading">
        {loadError ? (
          <div className="load-error">
            <div className="load-error-icon">!</div>
            <h2>无法载入 OpenRouter Manager</h2>
            <p>{loadError}</p>
            <button className="primary" onClick={() => void loadApplication()}>重新连接</button>
          </div>
        ) : (
          <div className="loading-inline"><Spinner /> 正在载入 OpenRouter Manager…</div>
        )}
      </div>
    )
  }

  const canScan = status.secrets.openrouter_api_key
  const canSync = status.configured

  return (
    <div className="shell">
      <aside>
        <div className="brand">
          <div className="logo">A</div>
          <div>
            <b>Ashan OpenRouter</b>
            <span>Manager v3.0.2</span>
          </div>
        </div>
        <nav>
          {(
            [
              ['overview', '总览'],
              ['models', '模型'],
              ['history', '历史'],
              ['settings', '设置'],
            ] as [Page, string][]
          ).map(([id, label]) => (
            <button key={id} className={page === id ? 'active' : ''} onClick={() => setPage(id)}>
              {label}
            </button>
          ))}
        </nav>
        <div className="side-status">
          <i className={status.configured ? 'green' : 'amber'} />
          <div>
            <b>{status.configured ? '基础配置完成' : '等待配置'}</b>
            <span>{status.scheduler.enabled ? '自动同步已开启' : '自动同步未开启'}</span>
          </div>
        </div>
      </aside>

      <main>
        <header>
          <div>
            <p className="eyebrow">OPENROUTER · NEW API</p>
            <h1>{page === 'overview' ? '总览' : page === 'models' ? '模型' : page === 'history' ? '运行历史' : '设置'}</h1>
          </div>
          <div className="actions">
            <button onClick={runScan} disabled={!!busy || !canScan} title={!canScan ? '请先保存 OpenRouter API Key' : undefined}>
              <ButtonContent busy={busy === 'scan'} idle="立即检查" loading="正在检查" />
            </button>
            <button
              className="primary"
              onClick={runSync}
              disabled={!!busy || !canSync}
              title={!canSync ? '请先完成 OpenRouter 与 New API 基础配置' : undefined}
            >
              <ButtonContent busy={busy === 'sync'} idle="立即同步" loading="正在同步" />
            </button>
          </div>
        </header>

        {toast && <ToastBanner toast={toast} onClose={() => setToast(null)} />}

        <section className="content">
          {page === 'overview' && (
            <OverviewPage status={status} settings={settings} scan={scan} />
          )}
          {page === 'models' && <ModelsPage scan={scan} />}
          {page === 'history' && <HistoryPage runs={runs} />}
          {page === 'settings' && (
            <SettingsPage
              settings={settings}
              savedSettings={savedSettings}
              setSettings={setSettings}
              setSavedSettings={setSavedSettings}
              status={status}
              setStatus={setStatus}
              secrets={secrets}
              setSecrets={setSecrets}
              busy={busy}
              setBusy={setBusy}
              setToast={setToast}
              refreshRuntime={refreshRuntime}
            />
          )}
        </section>
      </main>
    </div>
  )
}

function OverviewPage({ status, settings, scan }: { status: Status; settings: Settings; scan: Scan | null }) {
  const openrouterConfigured = status.secrets.openrouter_api_key
  const newapiConfigured = status.secrets.newapi_admin_token && !!settings.newapi_base_url.trim()

  return (
    <>
      <div className="hero card">
        <div>
          <p className="eyebrow">稳定模型入口</p>
          <h2>{settings.alias_model}</h2>
          <p>只有找到 3 个通过真实调用测试的免费模型后，系统才会修改 New API。</p>
        </div>
        <div className="health">
          <Badge tone={status.configured ? 'ok' : 'warn'}>{status.configured ? '运行就绪' : '需要配置'}</Badge>
          <span>下次自动执行 {fmt(status.scheduler.next_run_at)}</span>
        </div>
      </div>

      <div className="grid3">
        {[0, 1, 2].map((index) => {
          const current = status.current_models[index]
          const model = scan?.selected[index]
          return (
            <div className="card model-card" key={index}>
              <div className="rank">#{index + 1}</div>
              <h3>{model?.name || current?.model_id || '等待首次同步'}</h3>
              <code>{model?.id || current?.model_id || '—'}</code>
              <div className="metrics">
                <span>
                  Intelligence <b>{metric(model?.intelligence_index)}</b>
                </span>
                <span>
                  Context <b>{model ? `${Math.round(model.context_length / 1024)}K` : '—'}</b>
                </span>
              </div>
            </div>
          )
        })}
      </div>

      <div className="grid2">
        <div className="card">
          <div className="section-head compact-head">
            <div>
              <h3>连接状态</h3>
              <p>配置是否存在与真实连接状态分开显示。</p>
            </div>
          </div>
          <div className="connection-summary-list">
            <div>
              <span className="service-name">OpenRouter</span>
              <ConnectionStatus check={status.connections.openrouter} configured={openrouterConfigured} compact />
            </div>
            <div>
              <span className="service-name">New API</span>
              <ConnectionStatus check={status.connections.newapi} configured={newapiConfigured} compact />
            </div>
          </div>
        </div>
        <div className="card">
          <h3>最近状态</h3>
          <p>
            上次扫描 <b>{fmt(status.last_scan?.scanned_at)}</b>
          </p>
          <p>
            上次同步 <b>{fmt(status.last_run?.ended_at)}</b>
          </p>
          <p>
            结果{' '}
            <Badge tone={status.last_run?.status === 'failed' ? 'bad' : status.last_run ? 'ok' : 'muted'}>
              {status.last_run?.status || '—'}
            </Badge>
          </p>
        </div>
      </div>
    </>
  )
}

function ModelsPage({ scan }: { scan: Scan | null }) {
  return (
    <div className="card">
      <div className="section-head">
        <div>
          <h2>候选模型</h2>
          <p>{scan ? `扫描 ${scan.total_models} 个模型，符合免费基础条件 ${scan.free_models} 个` : '尚未执行扫描'}</p>
        </div>
      </div>
      {scan?.warning && <div className="warning">{scan.warning}</div>}
      {scan ? <ModelTable models={scan.ranked_candidates} /> : <div className="empty">点击“立即检查”生成候选模型。</div>}
    </div>
  )
}

function HistoryPage({ runs }: { runs: Run[] }) {
  return (
    <div className="card">
      <h2>最近 100 次运行</h2>
      <div className="runs">
        {runs.map((run) => (
          <div className="run" key={run.id}>
            <div>
              <Badge tone={run.status === 'failed' ? 'bad' : run.changed ? 'ok' : 'muted'}>{run.status}</Badge>
              <b>{run.trigger}</b>
              <span>{fmt(run.started_at)}</span>
            </div>
            <p>{run.error || run.selected_models.join(' · ') || '没有模型变化'}</p>
          </div>
        ))}
        {!runs.length && <div className="empty">暂无运行记录。</div>}
      </div>
    </div>
  )
}

function SettingsPage({
  settings,
  savedSettings,
  setSettings,
  setSavedSettings,
  status,
  setStatus,
  secrets,
  setSecrets,
  busy,
  setBusy,
  setToast,
  refreshRuntime,
}: {
  settings: Settings
  savedSettings: Settings
  setSettings: Dispatch<SetStateAction<Settings | null>>
  setSavedSettings: Dispatch<SetStateAction<Settings | null>>
  status: Status
  setStatus: Dispatch<SetStateAction<Status | null>>
  secrets: SecretDraft
  setSecrets: Dispatch<SetStateAction<SecretDraft>>
  busy: BusyAction
  setBusy: Dispatch<SetStateAction<BusyAction>>
  setToast: Dispatch<SetStateAction<Toast | null>>
  refreshRuntime: () => Promise<void>
}) {
  const set = (key: keyof Settings, value: any) => setSettings((current) => (current ? { ...current, [key]: value } : current))

  const openrouterDirty = secrets.openrouter_api_key.trim().length > 0
  const newapiDirty =
    normalizeUrl(settings.newapi_base_url) !== normalizeUrl(savedSettings.newapi_base_url) ||
    settings.newapi_admin_user_id.trim() !== savedSettings.newapi_admin_user_id.trim() ||
    secrets.newapi_admin_token.trim().length > 0 ||
    secrets.newapi_test_token.trim().length > 0
  const connectionDirty = openrouterDirty || newapiDirty
  const rulesDirty = useMemo(() => rulesSnapshot(settings) !== rulesSnapshot(savedSettings), [settings, savedSettings])
  const managedLocked = status.current_models.length > 0

  const openrouterConfigured = status.secrets.openrouter_api_key
  const newapiConfigured = status.secrets.newapi_admin_token && !!savedSettings.newapi_base_url.trim()

  const saveConnections = async () => {
    setBusy('save-connections')
    try {
      const response = await api.saveConnections({
        newapi_base_url: settings.newapi_base_url,
        newapi_admin_user_id: settings.newapi_admin_user_id,
        openrouter_api_key: secrets.openrouter_api_key || undefined,
        newapi_admin_token: secrets.newapi_admin_token || undefined,
        newapi_test_token: secrets.newapi_test_token || undefined,
      })

      setSavedSettings(response.settings)
      setSettings((current) =>
        current
          ? {
              ...current,
              newapi_base_url: response.settings.newapi_base_url,
              newapi_admin_user_id: response.settings.newapi_admin_user_id,
            }
          : current,
      )
      setSecrets({ openrouter_api_key: '', newapi_admin_token: '', newapi_test_token: '' })
      setStatus((current) =>
        current
          ? { ...current, secrets: response.secrets, connections: response.connections, configured: response.secrets.openrouter_api_key && response.secrets.newapi_admin_token && !!response.settings.newapi_base_url.trim() }
          : current,
      )
      setToast({ tone: 'success', title: '连接配置已保存', detail: '现在可以分别测试 OpenRouter 和 New API 连接。' })
      await refreshRuntime()
    } catch (error: any) {
      setToast({ tone: 'error', title: '保存连接配置失败', detail: error.message })
    } finally {
      setBusy('')
    }
  }

  const testOpenRouter = async () => {
    if (openrouterDirty) {
      setToast({ tone: 'warning', title: '请先保存 OpenRouter Key', detail: '测试连接只使用已保存的密钥，避免测试结果与实际运行配置不一致。' })
      return
    }
    setBusy('test-openrouter')
    try {
      const response = await api.testOpenRouter()
      setToast({ tone: 'success', title: response.message, detail: `${response.detail} · ${response.latency_ms} ms` })
      await refreshRuntime()
    } catch (error: any) {
      setToast({ tone: 'error', title: 'OpenRouter 连接失败', detail: error.message })
      await refreshRuntime().catch(() => undefined)
    } finally {
      setBusy('')
    }
  }

  const testNewApi = async () => {
    if (newapiDirty) {
      setToast({ tone: 'warning', title: '请先保存 New API 配置', detail: '地址、管理员 ID 或 Token 有未保存更改。保存后再测试，结果才与实际运行配置一致。' })
      return
    }
    setBusy('test-newapi')
    try {
      const response = await api.testNewApi()
      setToast({ tone: 'success', title: response.message, detail: `${response.detail} · ${response.latency_ms} ms` })
      await refreshRuntime()
    } catch (error: any) {
      setToast({ tone: 'error', title: 'New API 连接失败', detail: error.message })
      await refreshRuntime().catch(() => undefined)
    } finally {
      setBusy('')
    }
  }

  const saveRules = async () => {
    setBusy('save-settings')
    try {
      const response = await api.saveSettings(settings)
      const connectionDraft = {
        newapi_base_url: settings.newapi_base_url,
        newapi_admin_user_id: settings.newapi_admin_user_id,
      }
      setSavedSettings(response.settings)
      setSettings({ ...response.settings, ...connectionDraft })
      setToast({ tone: 'success', title: '模型与自动化设置已保存', detail: '新的规则会在下一次检查或同步时生效。' })
      await refreshRuntime()
    } catch (error: any) {
      setToast({ tone: 'error', title: '保存设置失败', detail: error.message })
    } finally {
      setBusy('')
    }
  }

  return (
    <div className="settings">
      <div className="card connection-card">
        <div className="settings-card-head">
          <div>
            <p className="eyebrow">CONNECTIONS</p>
            <h2>连接配置</h2>
            <p>连接信息独立保存。修改后先保存，再测试，避免测试配置与实际运行配置不一致。</p>
          </div>
          {connectionDirty ? <Badge tone="warn">有未保存更改</Badge> : <Badge tone="ok">已保存</Badge>}
        </div>

        <div className="connection-grid">
          <section className="service-panel">
            <div className="service-head">
              <div>
                <span className="service-kicker">模型源</span>
                <h3>OpenRouter</h3>
              </div>
              <ConnectionStatus check={status.connections.openrouter} configured={openrouterConfigured} />
            </div>

            <label>
              API Key
              <small>{status.secrets.openrouter_api_key ? '已安全保存；留空表示继续使用当前密钥' : '尚未配置'}</small>
              <input
                type="password"
                value={secrets.openrouter_api_key}
                onChange={(event) => setSecrets({ ...secrets, openrouter_api_key: event.target.value })}
                placeholder={status.secrets.openrouter_api_key ? '••••••••••••••••' : 'sk-or-v1-...'}
                autoComplete="new-password"
              />
            </label>

            <div className="service-actions">
              <button onClick={testOpenRouter} disabled={!!busy || !openrouterConfigured || openrouterDirty}>
                <ButtonContent busy={busy === 'test-openrouter'} idle="测试连接" loading="正在连接" />
              </button>
              {openrouterDirty && <span className="inline-hint">先保存新密钥</span>}
            </div>
          </section>

          <section className="service-panel">
            <div className="service-head">
              <div>
                <span className="service-kicker">模型网关</span>
                <h3>New API</h3>
              </div>
              <ConnectionStatus check={status.connections.newapi} configured={newapiConfigured} />
            </div>

            <label>
              New API 地址
              <small>例如 http://192.168.8.11:3001；结尾的 / 会自动规范化</small>
              <input
                value={settings.newapi_base_url}
                onChange={(event) => set('newapi_base_url', event.target.value)}
                placeholder="http://192.168.8.11:3001"
                disabled={managedLocked}
              />
            </label>
            <div className="form-grid connection-fields">
              <label>
                管理员 Token
                <small>{status.secrets.newapi_admin_token ? '已安全保存；留空保持不变' : '尚未配置'}</small>
                <input
                  type="password"
                  value={secrets.newapi_admin_token}
                  onChange={(event) => setSecrets({ ...secrets, newapi_admin_token: event.target.value })}
                  placeholder={status.secrets.newapi_admin_token ? '••••••••••••••••' : '输入管理员 Token'}
                  autoComplete="new-password"
                />
              </label>
              <label>
                管理员用户 ID
                <small>{managedLocked ? '受管渠道创建后已锁定' : '通常为 1'}</small>
                <input
                  value={settings.newapi_admin_user_id}
                  onChange={(event) => set('newapi_admin_user_id', event.target.value)}
                  disabled={managedLocked}
                />
              </label>
            </div>

            <div className="service-actions">
              <button onClick={testNewApi} disabled={!!busy || !newapiConfigured || newapiDirty}>
                <ButtonContent busy={busy === 'test-newapi'} idle="测试连接" loading="正在连接" />
              </button>
              {newapiDirty && <span className="inline-hint">先保存当前地址 / Token</span>}
            </div>
          </section>
        </div>

        <div className="save-bar">
          <div>
            <strong>{connectionDirty ? '连接配置尚未保存' : '连接配置已保存'}</strong>
            <span>{connectionDirty ? '保存后，测试按钮会针对已落盘的实际配置执行。' : '密钥以加密形式存储，页面不会回显明文。'}</span>
          </div>
          <button className="primary" onClick={saveConnections} disabled={!!busy || !connectionDirty}>
            <ButtonContent busy={busy === 'save-connections'} idle="保存连接配置" loading="正在保存" />
          </button>
        </div>
      </div>

      <div className="card">
        <div className="settings-card-head">
          <div>
            <p className="eyebrow">SELECTION</p>
            <h2>模型规则</h2>
            <p>控制候选池、上下文长度和排序策略。</p>
          </div>
          {rulesDirty ? <Badge tone="warn">有未保存更改</Badge> : <Badge>无更改</Badge>}
        </div>
        <div className="form-grid">
          <label>
            最低 Context
            <input type="number" value={settings.min_context_length} onChange={(event) => set('min_context_length', +event.target.value)} />
          </label>
          <label>
            候选池
            <input type="number" value={settings.candidate_pool} onChange={(event) => set('candidate_pool', +event.target.value)} />
          </label>
          <label>
            并发测试
            <input type="number" value={settings.preflight_concurrency} onChange={(event) => set('preflight_concurrency', +event.target.value)} />
          </label>
          <label>
            排名方式
            <select value={settings.ranking_mode} onChange={(event) => set('ranking_mode', event.target.value)}>
              <option value="intelligence_first">智力优先</option>
              <option value="weighted">综合评分</option>
            </select>
          </label>
        </div>
        <label>
          排除模型，每行一个
          <textarea
            value={settings.exclude_models.join('\n')}
            onChange={(event) => set('exclude_models', event.target.value.split('\n').map((item) => item.trim()).filter(Boolean))}
          />
        </label>
        <label>
          额外候选模型，每行一个
          <textarea
            value={settings.include_models.join('\n')}
            onChange={(event) => set('include_models', event.target.value.split('\n').map((item) => item.trim()).filter(Boolean))}
          />
        </label>
      </div>

      <div className="card">
        <div className="settings-card-head">
          <div>
            <p className="eyebrow">AUTOMATION</p>
            <h2>自动同步</h2>
            <p>只有基础配置完成后，定时同步才会真正执行。</p>
          </div>
        </div>
        <div className="toggle">
          <input type="checkbox" checked={settings.auto_sync} onChange={(event) => set('auto_sync', event.target.checked)} />
          <span>启用自动同步</span>
        </div>
        <label>
          同步周期
          <select value={settings.sync_interval_minutes} onChange={(event) => set('sync_interval_minutes', +event.target.value)}>
            <option value={60}>1 小时</option>
            <option value={180}>3 小时</option>
            <option value={360}>6 小时</option>
            <option value={720}>12 小时</option>
            <option value={1440}>24 小时</option>
          </select>
        </label>
        <div className="card-footer-actions">
          <button className="primary" onClick={saveRules} disabled={!!busy || !rulesDirty}>
            <ButtonContent busy={busy === 'save-settings'} idle="保存模型与自动化设置" loading="正在保存" />
          </button>
        </div>
      </div>

      <details className="card">
        <summary>高级设置</summary>
        <div className="form-grid top">
          <label>
            统一模型名
            <input value={settings.alias_model} onChange={(event) => set('alias_model', event.target.value)} disabled={managedLocked} />
          </label>
          <label>
            受管分组
            <input value={settings.managed_group} onChange={(event) => set('managed_group', event.target.value)} disabled={managedLocked} />
          </label>
          <label>
            受管标签
            <input value={settings.managed_tag} onChange={(event) => set('managed_tag', event.target.value)} disabled={managedLocked} />
          </label>
          <label>
            渠道前缀
            <input value={settings.channel_name_prefix} onChange={(event) => set('channel_name_prefix', event.target.value)} disabled={managedLocked} />
          </label>
        </div>
        <p className="muted">首次创建受管渠道后，这些身份字段会在界面和后端同时锁定。</p>
      </details>
    </div>
  )
}
