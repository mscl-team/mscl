import { useEffect, useState } from "react";
import { SkinPage } from "./SkinPage";
import { useT } from "../lib/i18n";
import { Icon } from "../components/Icon";
import { api, type RuntimeMetrics } from "../lib/api";

type FeatureTab = "skin" | "monitor";

interface Sample {
  cpu: number[];
  memory: number[];
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 MB";
  const mb = bytes / 1024 / 1024;
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${mb.toFixed(0)} MB`;
}

function formatUptime(seconds: number): string {
  if (!seconds) return "刚刚启动";
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

function Sparkline({
  values,
  max,
  color,
  fill,
}: {
  values: number[];
  max: number;
  color: string;
  fill: string;
}) {
  const width = 220;
  const height = 54;
  const safeMax = Math.max(max, 1);
  const points = values
    .map((value, index) => {
      const x = values.length <= 1 ? 0 : (index / (values.length - 1)) * width;
      const y = height - Math.min(value / safeMax, 1) * (height - 6) - 3;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  const area = `0,${height} ${points} ${width},${height}`;

  return (
    <svg className="sparkline" viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none">
      <polygon points={area} fill={fill} />
      {points && (
        <polyline
          points={points}
          fill="none"
          stroke={color}
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      )}
    </svg>
  );
}

export function FeaturesPage() {
  const t = useT();
  const [tab, setTab] = useState<FeatureTab>("skin");
  const [metrics, setMetrics] = useState<RuntimeMetrics | null>(null);
  const [history, setHistory] = useState<Record<string, Sample>>({});
  const [message, setMessage] = useState("");

  useEffect(() => {
    if (tab !== "monitor") return;
    let active = true;

    const poll = async () => {
      try {
        const next = await api.getRuntimeMetrics();
        if (!active) return;
        setMetrics(next);
        setHistory((prev) => {
          const updated: Record<string, Sample> = { ...prev };
          for (const instance of next.instances) {
            const old = updated[instance.run_id] ?? { cpu: [], memory: [] };
            updated[instance.run_id] = {
              cpu: [...old.cpu, instance.cpu_percent].slice(-30),
              memory: [...old.memory, instance.memory_bytes / 1024 / 1024].slice(-30),
            };
          }
          return updated;
        });
        setMessage("");
      } catch (error) {
        if (active) setMessage(String(error));
      }
    };

    void poll();
    const timer = window.setInterval(poll, 2000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [tab]);

  const instances = metrics?.instances ?? [];

  return (
    <div className="features-page">
      <div className="page features-tabs-page">
        <div className="settings-tabs" role="tablist">
          <button
            role="tab"
            aria-selected={tab === "skin"}
            className={`settings-tab ${tab === "skin" ? "settings-tab-active" : ""}`}
            onClick={() => setTab("skin")}
          >
            {t("features.wardrobe")}
          </button>
          <button
            role="tab"
            aria-selected={tab === "monitor"}
            className={`settings-tab ${tab === "monitor" ? "settings-tab-active" : ""}`}
            onClick={() => setTab("monitor")}
          >
            {t("features.monitor")}
          </button>
        </div>
      </div>

      {tab === "skin" && <SkinPage />}

      {tab === "monitor" && (
        <div className="page features-system-page">
          <header className="page-header features-monitor-head">
            <div>
              <h1>{t("features.monitor")}</h1>
              <p>{t("features.monitorSubtitle")}</p>
            </div>
            <span className="live-badge">
              <span className="live-dot" />
              {t("features.live")}
            </span>
          </header>

          {message && <div className="message-bar">{message}</div>}

          <div className="monitor-summary">
            <div className="monitor-summary-card">
              <span className="monitor-summary-label">{t("features.running")}</span>
              <strong>{instances.length}</strong>
              <span className="text-tertiary">{t("features.runningCount", { n: instances.length })}</span>
            </div>
            <div className="monitor-summary-card">
              <span className="monitor-summary-label">{t("features.systemCpu")}</span>
              <strong>{(metrics?.system_cpu_percent ?? 0).toFixed(1)}%</strong>
              <span className="text-tertiary">{t("features.lastSample")}</span>
            </div>
            <div className="monitor-summary-card">
              <span className="monitor-summary-label">{t("features.systemMemory")}</span>
              <strong>{formatBytes(metrics?.system_memory_used ?? 0)}</strong>
              <span className="text-tertiary">
                / {formatBytes(metrics?.system_memory_total ?? 0)}
              </span>
            </div>
          </div>

          {instances.length === 0 ? (
            <div className="empty-state monitor-empty">{t("features.noRunning")}</div>
          ) : (
            <div className="monitor-list">
              {instances.map((instance) => {
                const sample = history[instance.run_id] ?? { cpu: [], memory: [] };
                const cpuMax = Math.max(100, ...sample.cpu, instance.cpu_percent);
                const memMax = Math.max(512, ...sample.memory, instance.memory_bytes / 1024 / 1024);
                return (
                  <section className="monitor-card" key={instance.run_id}>
                    <div className="monitor-card-head">
                      <div className="monitor-card-title">
                        <span className="running-dot" />
                        <span>{instance.version_id}</span>
                        <span className="monitor-pid">PID {instance.pid}</span>
                      </div>
                      <span className="monitor-uptime">
                        {t("features.uptime", { time: formatUptime(instance.uptime_secs) })}
                      </span>
                    </div>
                    <div className="monitor-path text-tertiary">{instance.game_root}</div>
                    <div className="monitor-charts">
                      <div className="monitor-chart">
                        <div className="monitor-chart-head">
                          <span>{t("features.cpu")}</span>
                          <strong>{instance.cpu_percent.toFixed(1)}%</strong>
                        </div>
                        <Sparkline
                          values={sample.cpu}
                          max={cpuMax}
                          color="var(--warning, #e0a44a)"
                          fill="rgba(224, 164, 74, 0.16)"
                        />
                      </div>
                      <div className="monitor-chart">
                        <div className="monitor-chart-head">
                          <span>{t("features.memory")}</span>
                          <strong>{formatBytes(instance.memory_bytes)}</strong>
                        </div>
                        <Sparkline
                          values={sample.memory}
                          max={memMax}
                          color="var(--accent)"
                          fill="var(--accent-soft)"
                        />
                      </div>
                    </div>
                  </section>
                );
              })}
            </div>
          )}

          <div className="monitor-foot text-tertiary">
            <Icon name="info" size={13} />
            {t("features.monitorHint")}
          </div>
        </div>
      )}
    </div>
  );
}
