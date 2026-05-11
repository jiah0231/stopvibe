import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';

const STATUS_CACHE_KEY = 'stopvibe:last-status';
const DEFAULT_STATUS = { active: false, session: null };

function loadCachedStatus() {
  try {
    if (typeof window === 'undefined') return DEFAULT_STATUS;
    const raw = window.localStorage.getItem(STATUS_CACHE_KEY);
    if (!raw) return DEFAULT_STATUS;

    const cached = JSON.parse(raw);
    if (!cached?.session?.end_time) return DEFAULT_STATUS;

    const end = new Date(cached.session.end_time);
    if (Number.isNaN(end.getTime()) || end <= new Date()) {
      window.localStorage.removeItem(STATUS_CACHE_KEY);
      return DEFAULT_STATUS;
    }

    return { active: true, session: cached.session };
  } catch {
    return DEFAULT_STATUS;
  }
}

function cacheStatus(status) {
  try {
    if (typeof window === 'undefined') return;
    if (status.active && status.session) {
      window.localStorage.setItem(STATUS_CACHE_KEY, JSON.stringify(status));
    } else {
      window.localStorage.removeItem(STATUS_CACHE_KEY);
    }
  } catch {
    // localStorage is best-effort; the service remains the source of truth.
  }
}

function formatRemaining(endTime) {
  const end = new Date(endTime);
  const now = new Date();
  const diff = Math.max(0, Math.floor((end - now) / 1000));
  const h = Math.floor(diff / 3600);
  const m = Math.floor((diff % 3600) / 60);
  const s = diff % 60;
  return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

export default function App() {
  const [targets, setTargets] = useState([]);
  const [hours, setHours] = useState(1);
  const [minutes, setMinutes] = useState(0);
  const [status, setStatus] = useState(loadCachedStatus);
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState(null);
  const [countdown, setCountdown] = useState('');
  const failedPolls = useRef(0);

  const pollStatus = useCallback(async () => {
    try {
      const result = await invoke('get_status');
      setStatus(result);
      cacheStatus(result);
      setConnected(true);
      failedPolls.current = 0;
      setError(null);
      return true;
    } catch (e) {
      failedPolls.current += 1;
      if (failedPolls.current >= 3) {
        setConnected(false);
        setError(String(e));
      }
      return false;
    }
  }, []);

  const loadTargets = useCallback(async () => {
    try {
      const result = await invoke('get_default_targets');
      setTargets(result);
    } catch (e) {
      // Use hardcoded defaults if service unavailable
        setTargets([
          { name: 'Cursor', exe_names: ['cursor.exe'], cmdline_patterns: [], enabled: true },
          { name: 'Windsurf', exe_names: ['windsurf.exe'], cmdline_patterns: [], enabled: true },
          { name: 'VS Code (Copilot)', exe_names: ['code.exe'], cmdline_patterns: [], enabled: false },
        { name: 'Claude Code', exe_names: ['claude.exe'], cmdline_patterns: [], enabled: true },
        { name: 'Aider', exe_names: ['aider.exe'], cmdline_patterns: ['aider', '-m aider'], enabled: true },
        { name: 'OpenAI Codex CLI', exe_names: ['codex.exe'], cmdline_patterns: [], enabled: true },
        { name: 'Gemini CLI', exe_names: ['gemini.exe'], cmdline_patterns: [], enabled: true },
        { name: 'Goose', exe_names: ['goose.exe'], cmdline_patterns: [], enabled: true },
        { name: 'Kiro', exe_names: ['kiro.exe'], cmdline_patterns: [], enabled: true },
        { name: 'Trae', exe_names: ['trae.exe', 'trae-internal.exe'], cmdline_patterns: [], enabled: true },
      ]);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    let timeoutId = null;

    const tick = async () => {
      await pollStatus();
      if (!cancelled) {
        timeoutId = window.setTimeout(tick, 2000);
      }
    };

    const start = async () => {
      await loadTargets();
      if (!cancelled) {
        await tick();
      }
    };

    start();

    return () => {
      cancelled = true;
      if (timeoutId) {
        window.clearTimeout(timeoutId);
      }
    };
  }, [loadTargets, pollStatus]);

  useEffect(() => {
    if (!status.active || !status.session) {
      setCountdown('');
      return;
    }

    const updateCountdown = () => {
      setCountdown(formatRemaining(status.session.end_time));
    };

    updateCountdown();
    const timer = setInterval(updateCountdown, 1000);
    return () => clearInterval(timer);
  }, [status]);

  const toggleTarget = (index) => {
    setTargets(prev => prev.map((t, i) => i === index ? { ...t, enabled: !t.enabled } : t));
  };

  const startBlocking = async () => {
    const totalMinutes = hours * 60 + minutes;
    if (totalMinutes === 0) {
      setError('Duration must be at least 1 minute');
      return;
    }
    if (!targets.some(t => t.enabled)) {
      setError('Select at least one target');
      return;
    }
    try {
      await invoke('start_block', { durationMinutes: totalMinutes, targets });
      setError(null);
      await pollStatus();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <>
      <h1>StopVibe</h1>
      <p className="subtitle">Break free from vibe coding</p>

      <div className={`status-bar ${connected ? 'connected' : 'disconnected'}`}>
        {connected ? 'Service connected' : 'Service not connected'}
      </div>

      {!connected && (
        <button className="start-btn" onClick={async () => {
          setError(null);
          try {
            await invoke('install_service');
            setError(null);
            await pollStatus();
          } catch (e) {
            setError(String(e));
          }
        }} style={{marginBottom: '16px', background: '#1565c0'}}>
          Install & Start Service (requires Admin)
        </button>
      )}

      {status.active && status.session ? (
        <div className="active-session">
          <h2>BLOCKING ACTIVE</h2>
          <div className="countdown">{countdown}</div>
          <p className="info">
            Ends at: {new Date(status.session.end_time).toLocaleTimeString()}
          </p>
          <p className="info">This block cannot be removed until the timer expires.</p>
          <div className="blocked-targets">
            <h3>Blocked targets:</h3>
            <ul>
              {status.session.targets.filter(t => t.enabled).map((t, i) => (
                <li key={i}>{t.name}</li>
              ))}
            </ul>
          </div>
        </div>
      ) : (
        <>
          <div className="section-title">Duration</div>
          <div className="duration-picker">
            <label>
              Hours
              <input type="number" min="0" max="24" value={hours}
                onChange={e => setHours(Math.max(0, Math.min(24, parseInt(e.target.value) || 0)))} />
            </label>
            <label>
              Minutes
              <input type="number" min="0" max="59" value={minutes}
                onChange={e => setMinutes(Math.max(0, Math.min(59, parseInt(e.target.value) || 0)))} />
            </label>
          </div>

          <div className="section-title">Targets to block</div>
          <div className="targets-list">
            {targets.map((target, i) => (
              <div key={i} className="target-item" onClick={() => toggleTarget(i)}>
                <input type="checkbox" checked={target.enabled} readOnly />
                <span className="name">{target.name}</span>
                <span className="exes">{target.exe_names.join(', ')}</span>
              </div>
            ))}
          </div>

          <button className="start-btn" onClick={startBlocking} disabled={!connected}>
            Start Blocking
          </button>
        </>
      )}

      {error && <p className="error-msg">{error}</p>}
    </>
  );
}
