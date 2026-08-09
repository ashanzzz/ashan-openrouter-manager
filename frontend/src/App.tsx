import { useEffect, useMemo, useRef, useState, type Dispatch, type ReactNode, type SetStateAction } from 'react'
import { api } from './api'
import type {
  ConnectionCheck,
  RankedModel,
  RoutingChannel,
  RoutingPoolStatus,
  Run,
  Scan,
  Settings,
  Status,
  SyncLogEntry,
  SyncProgress,
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
const healthPct = (value?: number | null) => `${((value || 0) * 100).toFixed(1)}%`
const healthTone = (value?: number | null, usable?: boolean | null) => (value == null ? 'muted' : usable === false ? 'bad' : value >= 0.999 ? 'ok' : value > 0 ? 'warn' : 'bad')
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
            <th>健康成功率</th>
            <th>最近检测</th>
            <th>上次检测时间</th>
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
                {model.health_attempts ? (
                  <div className="health-cell">
                    <Badge tone={healthTone(model.health_success_rate, model.usable)}>{healthPct(model.health_success_rate)}</Badge>
                    <small>{model.health_successes}/{model.health_attempts} · {model.usable ? '通过准入' : '未通过'}</small>
                  </div>
                ) : <Badge>未检测</Badge>}
              </td>
              <td>
                <div className="health-attempts" aria-label="最近健康检测">
                  {(model.health_checks || []).map((attempt) => (
                    <span
                      key={`${attempt.batch_id}-${attempt.attempt}`}
                      className={attempt.success ? 'success' : 'failed'}
                      title={`${attempt.success ? '成功' : '失败'} · ${attempt.latency_ms} ms${attempt.error ? ` · ${attempt.error}` : ''}`}
                    >
                      {attempt.success ? '✓' : '×'}
                    </span>
                  ))}
                  {!model.health_checks?.length && <span className="muted">—</span>}
                </div>
              </td>
              <td>
                <div className="last-check-cell">
                  <span>{fmt(model.last_checked_at)}</span>
                  {model.average_latency_ms != null && <small>成功请求均值 {model.average_latency_ms} ms</small>}
                </div>
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

const stageLabels: Record<string, string> = {
  configuration: '配置校验',
  openrouter_models: 'OpenRouter 模型',
  openrouter_benchmarks: 'Benchmark',
  ranking: '筛选与排名',
  preflight: '模型健康检测',
  newapi_connection: 'New API 连接/权限',
  newapi_conflicts: '渠道冲突检查',
  newapi_routing: '路由池检查',
  newapi_identity: '渠道身份校验',
  newapi_create: '渠道初始化',
  newapi_update: '渠道更新',
  newapi_test: '渠道测试',
  e2e: '端到端测试',
  rollback: '回滚',
  complete: '完成',
}

const categoryLabels: Record<string, string> = {
  configuration: '配置',
  openrouter_api: 'OpenRouter API',
  model_health: '模型健康',
  openrouter_permission: 'OpenRouter 权限',
  newapi_api: 'New API',
  newapi_schema: 'New API API Schema',
  newapi_permission: 'New API 权限',
  safety_conflict: '安全冲突',
  safety: '安全校验',
  legacy_channel: '历史渠道',
  manual_channel: '手动渠道',
  related_channel: '相关渠道',
  manager_orphan: '孤儿 AOM',
  routing: '路由池',
  network_or_internal: '网络/内部',
  selection: '模型选择',
  validation: '校验',
  rollback_failure: '回滚失败',
  result: '结果',
  lifecycle: '任务',
  no_change: '无变化',
}

function logTone(level: string) {
  if (level === 'success') return 'ok'
  if (level === 'warning') return 'warn'
  if (level === 'error') return 'bad'
  return 'muted'
}

function runStatusLabel(status: string) {
  if (status === 'running') return '执行中'
  if (status === 'failed') return '失败'
  if (status === 'no_change') return '无需更新'
  if (status === 'initialized') return '初始化完成'
  return '已完成'
}

function SyncLogPanel({ progress, onClose, compact = false }: { progress: SyncProgress; onClose?: () => void; compact?: boolean }) {
  const bottomRef = useRef<HTMLDivElement | null>(null)
  const running = progress.run.status === 'running'

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: 'nearest' })
  }, [progress.logs.length])

  return (
    <div className={`sync-log-panel ${compact ? 'compact' : ''}`}>
      <div className="sync-log-head">
        <div>
          <div className="sync-log-title-row">
            <strong>{running ? '实时同步日志' : '同步日志'}</strong>
            <Badge tone={running ? 'warn' : progress.run.status === 'failed' ? 'bad' : 'ok'}>{runStatusLabel(progress.run.status)}</Badge>
          </div>
          <span>Run {progress.run.id.slice(0, 8)} · {progress.run.trigger === 'manual' ? '手动同步' : '定时同步'} · {fmt(progress.run.started_at)}</span>
        </div>
        {onClose && !running && <button className="log-close" onClick={onClose}>关闭</button>}
      </div>
      <div className="sync-log-stream" role="log" aria-live="polite">
        {progress.logs.map((entry: SyncLogEntry) => (
          <div className={`sync-log-line ${entry.level}`} key={`${entry.run_id}-${entry.seq}`}>
            <span className="sync-log-time">{new Date(entry.timestamp).toLocaleTimeString()}</span>
            <span className={`sync-log-level ${entry.level}`} />
            <div className="sync-log-body">
              <div className="sync-log-meta">
                <Badge tone={logTone(entry.level)}>{stageLabels[entry.stage] || entry.stage}</Badge>
                <span>{categoryLabels[entry.category] || entry.category}</span>
              </div>
              <strong>{entry.message}</strong>
              {entry.detail && <pre>{entry.detail}</pre>}
            </div>
          </div>
        ))}
        {!progress.logs.length && <div className="sync-log-empty"><Spinner /> 正在等待第一条日志…</div>}
        <div ref={bottomRef} />
      </div>
      {progress.run.status === 'failed' && progress.run.error && (
        <div className="sync-log-diagnosis">
          <strong>最终错误</strong>
          <span>{progress.run.error}</span>
        </div>
      )}
    </div>
  )
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
  const [routing, setRouting] = useState<RoutingPoolStatus | null>(null)
  const [busy, setBusy] = useState<BusyAction>('')
  const [toast, setToast] = useState<Toast | null>(null)
  const [loadError, setLoadError] = useState('')
  const [activeSyncId, setActiveSyncId] = useState<string | null>(null)
  const [syncProgress, setSyncProgress] = useState<SyncProgress | null>(null)
  const [secrets, setSecrets] = useState<SecretDraft>({
    openrouter_api_key: '',
    newapi_admin_token: '',
    newapi_test_token: '',
  })

  const refreshRuntime = async () => {
    const [nextStatus, models, history, nextRouting] = await Promise.all([
      api.status(),
      api.models(),
      api.history(),
      api.routing().catch(() => null),
    ])
    setStatus(nextStatus)
    setScan(models.scan || null)
    setRuns(history.runs || [])
    setRouting(nextRouting)
  }

  const initialLoad = async () => {
    const [nextStatus, nextSettings, models, history, nextRouting] = await Promise.all([
      api.status(),
      api.settings(),
      api.models(),
      api.history(),
      api.routing().catch(() => null),
    ])
    setStatus(nextStatus)
    setSavedSettings(nextSettings)
    setSettings(nextSettings)
    setScan(models.scan || null)
    setRuns(history.runs || [])
    setRouting(nextRouting)
    if (nextStatus.active_sync_run_id) {
      setActiveSyncId(nextStatus.active_sync_run_id)
      setBusy('sync')
    }
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

  useEffect(() => {
    if (!activeSyncId) return
    let cancelled = false
    let timer: number | undefined

    const poll = async () => {
      try {
        const progress = await api.syncProgress(activeSyncId)
        if (cancelled) return
        setSyncProgress(progress)
        if (progress.run.status === 'running') {
          timer = window.setTimeout(() => void poll(), 750)
          return
        }

        setBusy('')
        setActiveSyncId(null)
        const selected = progress.run.selected_models.map((model, index) => `#${index + 1} ${model}`).join(' · ')
        if (progress.run.status === 'failed') {
          setToast({ tone: 'error', title: '同步失败', detail: progress.run.error || '请查看实时日志定位错误阶段。' })
        } else {
          setToast({
            tone: 'success',
            title: progress.run.changed ? '同步完成' : '无需更新',
            detail: selected || '同步任务已完成',
          })
        }
        await refreshRuntime().catch(() => undefined)
      } catch (error: any) {
        if (cancelled) return
        setBusy('')
        setActiveSyncId(null)
        setToast({ tone: 'error', title: '无法读取同步进度', detail: error.message })
      }
    }

    void poll()
    return () => {
      cancelled = true
      if (timer) window.clearTimeout(timer)
    }
  }, [activeSyncId])

  useEffect(() => {
    if (activeSyncId) return
    const timer = window.setInterval(() => {
      void api.status().then((nextStatus) => {
        setStatus(nextStatus)
        if (nextStatus.active_sync_run_id) {
          setActiveSyncId(nextStatus.active_sync_run_id)
          setBusy('sync')
        }
      }).catch(() => undefined)
    }, 5000)
    return () => window.clearInterval(timer)
  }, [activeSyncId])

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
    setToast(null)
    setSyncProgress(null)
    try {
      const response = await api.startSync(false)
      setActiveSyncId(response.run_id)
      const initial = await api.syncProgress(response.run_id).catch(() => null)
      if (initial) setSyncProgress(initial)
    } catch (error: any) {
      setBusy('')
      setToast({ tone: 'error', title: '无法启动同步', detail: error.message })
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

  const unsavedConnectionChanges =
    secrets.openrouter_api_key.trim().length > 0 ||
    secrets.newapi_admin_token.trim().length > 0 ||
    secrets.newapi_test_token.trim().length > 0 ||
    normalizeUrl(settings.newapi_base_url) !== normalizeUrl(savedSettings.newapi_base_url) ||
    settings.newapi_admin_user_id.trim() !== savedSettings.newapi_admin_user_id.trim()
  const unsavedRuleChanges = rulesSnapshot(settings) !== rulesSnapshot(savedSettings)
  const canScan = status.secrets.openrouter_api_key && !secrets.openrouter_api_key.trim() && !unsavedRuleChanges
  const canSync = status.configured && !unsavedConnectionChanges && !unsavedRuleChanges

  return (
    <div className="shell">
      <aside>
        <div className="brand">
          <div className="logo">A</div>
          <div>
            <b>Ashan OpenRouter</b>
            <span>Manager v3.0.8</span>
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
            <button onClick={runScan} disabled={!!busy || !canScan} title={!canScan ? '请先保存 OpenRouter Key 和模型规则后再检查' : undefined}>
              <ButtonContent busy={busy === 'scan'} idle="立即检查" loading="三轮检测中" />
            </button>
            <button
              className="primary"
              onClick={runSync}
              disabled={!!busy || !canSync}
              title={!canSync ? '请先保存所有连接与模型设置，再执行同步' : '重新扫描、实测 Top 3，并在需要时更新 New API'}
            >
              <ButtonContent busy={busy === 'sync'} idle="立即同步" loading="正在同步" />
            </button>
          </div>
        </header>

        {toast && <ToastBanner toast={toast} onClose={() => setToast(null)} />}
        {syncProgress && page !== 'settings' && (
          <div className="sync-log-global">
            <SyncLogPanel progress={syncProgress} onClose={() => setSyncProgress(null)} />
          </div>
        )}

        <section className="content">
          {page === 'overview' && (
            <OverviewPage status={status} settings={settings} scan={scan} routing={routing} />
          )}
          {page === 'models' && <ModelsPage scan={scan} settings={settings} />}
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
              onSync={runSync}
              canSync={canSync}
              syncProgress={syncProgress}
              clearSyncProgress={() => setSyncProgress(null)}
            />
          )}
        </section>
      </main>
    </div>
  )
}

function OverviewPage({ status, settings, scan, routing }: { status: Status; settings: Settings; scan: Scan | null; routing: RoutingPoolStatus | null }) {
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
          <span>下次自动执行 {status.scheduler.next_run_local || fmt(status.scheduler.next_run_at)}</span>
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
      <RoutingPoolCard routing={routing} enabledStatus={settings.enabled_status} />
    </>
  )
}

function routeModeLabel(mode: string) {
  switch (mode) {
    case 'manual_first': return '手动池优先'
    case 'managed_first': return 'AOM 自动池优先'
    case 'mixed_same_priority': return '同优先级混合'
    case 'manual_only': return '仅手动池'
    case 'managed_only': return '仅 AOM 自动池'
    case 'no_enabled_channels': return '暂无启用渠道'
    default: return '尚未读取'
  }
}

function RoutingChannelRow({ channel, enabledStatus, managed }: { channel: RoutingChannel; enabledStatus: number; managed: boolean }) {
  const enabled = channel.status === enabledStatus
  return (
    <div className="route-channel-row">
      <div className="route-channel-main">
        <span className={`route-owner-dot ${managed ? 'managed' : 'manual'}`} />
        <div>
          <strong>{channel.name || `Channel ${channel.id}`}</strong>
          <span>ID {channel.id} · priority {channel.priority} · weight {channel.weight}</span>
        </div>
      </div>
      <div className="route-channel-meta">
        <Badge tone={enabled ? 'ok' : 'muted'}>{enabled ? '启用' : `状态 ${channel.status}`}</Badge>
        <code>{channel.mapping_target || channel.models.join(', ') || '未设置映射'}</code>
      </div>
    </div>
  )
}

function RoutingPoolCard({ routing, enabledStatus }: { routing: RoutingPoolStatus | null; enabledStatus: number }) {
  return (
    <div className="card routing-card">
      <div className="section-head routing-head">
        <div>
          <p className="eyebrow">NEW API ROUTING POOL</p>
          <h3>手动渠道 + AOM 自动 Top3</h3>
          <p>相同统一别名允许共存。AOM 只修改自己登记的精确 Channel ID；手动渠道永久只读。</p>
        </div>
        <Badge tone={routing?.orphan_channels.length ? 'bad' : routing?.available ? 'ok' : 'muted'}>
          {routing?.orphan_channels.length ? `${routing.orphan_channels.length} 个孤儿 AOM` : routing?.available ? routeModeLabel(routing.route_mode) : '未读取'}
        </Badge>
      </div>

      {!routing || !routing.available ? (
        <div className="route-empty">
          <strong>暂时无法读取路由池</strong>
          <span>{routing?.error || routing?.message || '保存并测试 New API 后将显示手动池和自动池。'}</span>
        </div>
      ) : (
        <>
          <div className="route-summary-grid">
            <div><span>手动同别名</span><strong>{routing.manual_channels.length}</strong><small>启用 {routing.manual_enabled}</small></div>
            <div><span>AOM 受管</span><strong>{routing.managed_channels.length}</strong><small>启用 {routing.managed_enabled}</small></div>
            <div><span>手动最高优先级</span><strong>{routing.highest_manual_priority ?? '—'}</strong><small>AOM 不会修改</small></div>
            <div><span>AOM 最高优先级</span><strong>{routing.highest_managed_priority ?? '—'}</strong><small>由 Manager 设置控制</small></div>
          </div>
          <div className="route-mode-note">{routing.message}</div>
          {routing.orphan_channels.length > 0 && (
            <div className="route-orphan-warning">
              <strong>发现未登记的 AOM 身份渠道，自动同步会安全停止。</strong>
              <span>{routing.orphan_channels.map((c) => `ID ${c.id} ${c.name}`).join(' · ')}</span>
            </div>
          )}
          <div className="route-pools">
            <div className="route-pool">
              <div className="route-pool-title"><span className="route-owner-dot manual" /><div><strong>手动渠道池</strong><span>只读 · 不删除 · 不改 priority/weight</span></div></div>
              <div className="route-channel-list">
                {routing.manual_channels.map((channel) => <RoutingChannelRow key={`manual-${channel.id}`} channel={channel} enabledStatus={enabledStatus} managed={false} />)}
                {!routing.manual_channels.length && <div className="route-mini-empty">没有检测到同别名手动渠道</div>}
              </div>
            </div>
            <div className="route-pool">
              <div className="route-pool-title"><span className="route-owner-dot managed" /><div><strong>AOM 自动池</strong><span>只管理 SQLite 登记的 3 个精确 ID</span></div></div>
              <div className="route-channel-list">
                {routing.managed_channels.map((channel) => <RoutingChannelRow key={`managed-${channel.id}`} channel={channel} enabledStatus={enabledStatus} managed />)}
                {!routing.managed_channels.length && <div className="route-mini-empty">尚未初始化 AOM Top3</div>}
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  )
}

function ModelsPage({ scan, settings }: { scan: Scan | null; settings: Settings }) {
  const selected = scan?.selected || []
  return (
    <>
      <div className="health-overview card">
        <div>
          <p className="eyebrow">MODEL HEALTH ENGINE</p>
          <h2>模型健康检测</h2>
          <p>R1 / R2 / R3 使用完全相同的健康准入规则：达到门槛后，健康率不再参与排序，严格按原能力排名取前三名。</p>
        </div>
        <div className="health-rule-grid">
          <div><span>每轮检测</span><strong>{Math.max(3, settings.health_check_attempts)} 次</strong></div>
          <div><span>轮次间隔</span><strong>{settings.health_check_interval_seconds} 秒</strong></div>
          <div><span>最低成功率</span><strong>{(settings.health_min_success_rate * 100).toFixed(0)}%</strong></div>
          <div><span>上次完整检测</span><strong>{fmt(scan?.scanned_at)}</strong></div>
        </div>
      </div>

      {!!selected.length && (
        <div className="grid3 health-role-grid">
          {[0, 1, 2].map((index) => {
            const model = selected[index]
            return (
              <div className="card health-role-card" key={index}>
                <div className="role-label">{index === 0 ? 'R1 · 能力第 1' : `R${index + 1} · 能力第 ${index + 1}`}</div>
                <h3>{model?.name || '等待检测'}</h3>
                <code>{model?.id || '—'}</code>
                {model && (
                  <div className="role-health">
                    <Badge tone={healthTone(model.health_success_rate)}>{healthPct(model.health_success_rate)}</Badge>
                    <span>能力排名 #{model.rank} · {model.health_successes}/{model.health_attempts} 成功 · Intelligence {metric(model.intelligence_index)}</span>
                  </div>
                )}
              </div>
            )
          })}
        </div>
      )}

      <div className="card">
        <div className="section-head">
          <div>
            <h2>候选模型</h2>
            <p>{scan ? `扫描 ${scan.total_models} 个模型，符合免费基础条件 ${scan.free_models} 个；候选按能力排名保持原顺序` : '尚未执行扫描'}</p>
          </div>
        </div>
        {scan?.warning && <div className="warning">{scan.warning}</div>}
        {scan ? <ModelTable models={scan.ranked_candidates} /> : <div className="empty">点击“立即检查”后会执行至少 3 轮真实调用检测，默认总间隔约 2 分钟。</div>}
      </div>
    </>
  )
}

function HistoryPage({ runs }: { runs: Run[] }) {
  const [selected, setSelected] = useState<SyncProgress | null>(null)
  const [loadingId, setLoadingId] = useState<string | null>(null)
  const [logError, setLogError] = useState('')

  const viewLogs = async (runId: string) => {
    if (selected?.run.id === runId) {
      setSelected(null)
      return
    }
    setLoadingId(runId)
    setLogError('')
    try {
      setSelected(await api.syncProgress(runId))
    } catch (error: any) {
      setLogError(error.message || '读取运行日志失败')
    } finally {
      setLoadingId(null)
    }
  }

  return (
    <div className="card">
      <h2>最近 100 次运行</h2>
      <p className="history-intro">每次同步的阶段日志都会保存在 SQLite。失败时可直接查看是配置、权限、网络、模型测试还是渠道安全校验导致。</p>
      {logError && <div className="warning">{logError}</div>}
      <div className="runs">
        {runs.map((run) => (
          <div className="run-wrap" key={run.id}>
            <div className="run">
              <div>
                <Badge tone={run.status === 'running' ? 'warn' : run.status === 'failed' ? 'bad' : run.changed ? 'ok' : 'muted'}>
                  {run.status === 'running' ? '执行中' : run.status === 'failed' ? '失败' : run.changed ? '已更新' : '无变化'}
                </Badge>
                <b>{run.trigger === 'manual' ? '手动同步' : run.trigger === 'schedule' ? '定时同步' : run.trigger}</b>
                <span>{fmt(run.started_at)}</span>
              </div>
              <div className="run-actions">
                <p>{run.error || run.selected_models.join(' · ') || (run.status === 'running' ? '同步正在进行' : '没有模型变化')}</p>
                <button onClick={() => void viewLogs(run.id)} disabled={loadingId === run.id}>
                  {loadingId === run.id ? '读取中…' : selected?.run.id === run.id ? '收起日志' : '查看日志'}
                </button>
              </div>
            </div>
            {selected?.run.id === run.id && <SyncLogPanel progress={selected} compact onClose={() => setSelected(null)} />}
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
  onSync,
  canSync,
  syncProgress,
  clearSyncProgress,
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
  onSync: () => Promise<void>
  canSync: boolean
  syncProgress: SyncProgress | null
  clearSyncProgress: () => void
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
      setToast({ tone: 'success', title: '模型与自动化设置已保存', detail: settings.auto_sync ? '自动调度已重新计算，新的执行计划立即生效。' : '新的规则会在下一次检查或同步时生效。' })
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
            <p>健康率只负责准入；达到门槛的模型一律按能力排名决定 R1/R2/R3。</p>
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
        <div className="health-settings-block">
          <div className="settings-card-head compact-settings-head">
            <div>
              <h3>健康检测规则</h3>
              <p>默认三轮真实调用，间隔 60 秒；成功率达到门槛即可保留原能力排名。</p>
            </div>
            <Badge tone="ok">能力优先</Badge>
          </div>
          <div className="form-grid">
            <label>
              每个模型检测次数
              <input type="number" min={3} value={settings.health_check_attempts} onChange={(event) => set('health_check_attempts', Math.max(3, +event.target.value))} />
              <small>最少 3 次。</small>
            </label>
            <label>
              每轮间隔（秒）
              <input type="number" min={60} value={settings.health_check_interval_seconds} onChange={(event) => set('health_check_interval_seconds', Math.max(60, +event.target.value))} />
              <small>推荐 60 秒，用于跨时间验证偶发抖动。</small>
            </label>
            <label>
              最低成功率（%）
              <input type="number" min={1} max={100} step={1} value={Math.round(settings.health_min_success_rate * 100)} onChange={(event) => set('health_min_success_rate', Math.min(1, Math.max(0.01, +event.target.value / 100)))} />
              <small>默认 30%；3 次成功 1 次即 33.3%，可以入选。</small>
            </label>
            <div className="health-policy-note">
              <strong>选择策略</strong>
              <span>R1、R2、R3 使用同一规则：健康率达到门槛即视为合格，之后完全忽略健康率差异，严格按原能力排名取前三名。健康度只用于淘汰明显不可用模型和页面诊断。</span>
            </div>
          </div>
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

        <div className="schedule-mode" role="group" aria-label="自动同步方式">
          <button type="button" className={settings.schedule_mode === 'interval' ? 'active' : ''} onClick={() => set('schedule_mode', 'interval')}>
            按间隔
          </button>
          <button type="button" className={settings.schedule_mode === 'daily' ? 'active' : ''} onClick={() => set('schedule_mode', 'daily')}>
            每天固定时间
          </button>
        </div>

        {settings.schedule_mode === 'interval' ? (
          <label>
            同步周期
            <select value={settings.sync_interval_minutes} onChange={(event) => set('sync_interval_minutes', +event.target.value)}>
              <option value={60}>每 1 小时</option>
              <option value={180}>每 3 小时</option>
              <option value={360}>每 6 小时</option>
              <option value={720}>每 12 小时</option>
              <option value={1440}>每 24 小时</option>
            </select>
            <small>按间隔模式从服务启动或设置保存后开始计算下一次执行。</small>
          </label>
        ) : (
          <div className="form-grid schedule-grid">
            <label>
              每天执行时间
              <input type="time" step="60" value={settings.daily_sync_time} onChange={(event) => set('daily_sync_time', event.target.value)} />
              <small>例如 00:00 表示每天零点执行。</small>
            </label>
            <label>
              时区
              <select value={settings.schedule_timezone} onChange={(event) => set('schedule_timezone', event.target.value)}>
                <option value="Asia/Shanghai">Asia/Shanghai · 中国标准时间</option>
                <option value="Asia/Hong_Kong">Asia/Hong_Kong</option>
                <option value="Asia/Tokyo">Asia/Tokyo</option>
                <option value="America/Chicago">America/Chicago</option>
                <option value="America/New_York">America/New_York</option>
                <option value="Europe/London">Europe/London</option>
                <option value="UTC">UTC</option>
              </select>
              <small>固定时间以这里选择的时区为准，不依赖 Docker 主机时区。</small>
            </label>
          </div>
        )}

        <div className={`schedule-summary ${settings.auto_sync ? 'enabled' : ''}`}>
          <div>
            <span className="status-dot" />
            <div>
              <strong>{settings.auto_sync ? '自动同步已启用' : '自动同步未启用'}</strong>
              <span>
                {settings.schedule_mode === 'daily'
                  ? `每天 ${settings.daily_sync_time || '00:00'} · ${settings.schedule_timezone}`
                  : `每 ${Math.round(settings.sync_interval_minutes / 60)} 小时`}
              </span>
            </div>
          </div>
          <span className="next-run">下次执行：{rulesDirty ? '保存后重新计算' : status.scheduler.next_run_local || fmt(status.scheduler.next_run_at)}</span>
        </div>

        <div className="manual-sync-card">
          <div>
            <strong>立即同步</strong>
            <span>不等待定时任务。立即重新扫描、实测 Top 3，并仅在结果变化时更新 New API。</span>
          </div>
          <button className="primary" type="button" onClick={() => void onSync()} disabled={!!busy || !canSync} title={!canSync ? '请先保存连接配置和模型设置' : undefined}>
            <ButtonContent busy={busy === 'sync'} idle="立即同步" loading="正在扫描并同步" />
          </button>
        </div>

        {syncProgress && <SyncLogPanel progress={syncProgress} onClose={clearSyncProgress} />}

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
