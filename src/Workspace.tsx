import { useEffect, useState, type ReactNode } from "react";
import {
  Film,
  Music2,
  Sparkles,
  Plus,
  Check,
  Trash2,
  RefreshCw,
  FolderOpen,
  Download,
  ExternalLink,
  HardDrive,
} from "lucide-react";
import { type Presets, type Options, type Call, bytes } from "./types";

export function PresetsPage({
  presets,
  save,
  renderOptions,
}: {
  presets: Presets;
  save: (p: Presets) => Promise<boolean>;
  renderOptions: (o: Options, set: (o: Options) => void) => ReactNode;
}) {
  const [form, setForm] = useState(() => structuredClone(presets));
  const [selected, select] = useState(presets.defaultId);
  const [saving, setSaving] = useState(false);
  const current = form.items.find((p) => p.id === selected) || form.items[0];
  const update = (patch: Partial<typeof current>) =>
    setForm((f) => ({
      ...f,
      items: f.items.map((p) => (p.id === current.id ? { ...p, ...patch } : p)),
    }));
  const persist = async () => {
    setSaving(true);
    try {
      await save(form);
    } finally {
      setSaving(false);
    }
  };
  return (
    <div className="page-scroll presets-page">
      <div className="page-heading">
        <h1>下载预设</h1>
        <button
          onClick={() => {
            const id = crypto.randomUUID();
            setForm((f) => ({
              ...f,
              items: [
                ...f.items,
                { ...structuredClone(current), id, name: "新预设" },
              ],
            }));
            select(id);
          }}
          disabled={form.items.length >= 32}
        >
          <Plus size={16} />
          新建预设
        </button>
      </div>
      <div className="preset-cards">
        {form.items.map((p) => {
          const Icon = ["mp3", "m4a", "flac", "wav"].includes(p.options.format)
            ? Music2
            : p.options.quality === 0
              ? Sparkles
              : Film;
          return (
            <button
              key={p.id}
              className={`preset-card ${current.id === p.id ? "selected" : ""}`}
              onClick={() => select(p.id)}
            >
              <div className="preset-top">
                <span className="preset-icon">
                  <Icon size={21} />
                </span>
                {form.defaultId === p.id && (
                  <span className="default-tag">
                    <Check size={12} />
                    默认
                  </span>
                )}
              </div>
              <strong>{p.name}</strong>
              <small>
                {p.options.format.toUpperCase()} ·{" "}
                {["mp3", "m4a"].includes(p.options.format)
                  ? `${p.options.bitrate} kbps`
                  : p.options.quality
                    ? `${p.options.quality}P`
                    : "最高可用"}
              </small>
            </button>
          );
        })}
      </div>
      <div className="preset-editor">
        <div className="editor-heading">
          <label>
            预设名称
            <input
              maxLength={32}
              value={current.name}
              onChange={(e) => update({ name: e.target.value })}
            />
          </label>
          <div className="actions">
            <button
              disabled={current.id === form.defaultId}
              onClick={() => setForm({ ...form, defaultId: current.id })}
            >
              <Check size={15} />
              设为默认
            </button>
            <button
              aria-label="删除预设"
              disabled={form.items.length === 1}
              onClick={() => {
                const items = form.items.filter((p) => p.id !== current.id);
                setForm({
                  items,
                  defaultId:
                    form.defaultId === current.id
                      ? items[0].id
                      : form.defaultId,
                });
                select(items[0].id);
              }}
            >
              <Trash2 size={15} />
            </button>
          </div>
        </div>
        {renderOptions(current.options, (o) => update({ options: o }))}
        <div className="filename-preview">
          <FolderOpen size={15} />
          <span>
            {current.options.template
              .replaceAll("{UP主}", "UP主")
              .replaceAll("{标题}", "视频标题")
              .replaceAll("{P序号}", "01")
              .replaceAll("{分P标题}", "分P标题")
              .replaceAll("{BV号}", "BV号")}
            .{current.options.format}
          </span>
        </div>
        <div className="save-row">
          <button
            className="primary"
            disabled={saving || !current.name.trim()}
            onClick={() => void persist()}
          >
            {saving ? (
              <RefreshCw className="spin" size={15} />
            ) : (
              <Check size={15} />
            )}
            保存预设
          </button>
        </div>
      </div>
    </div>
  );
}

type Maintenance = {
  cacheBytes: number;
  cacheFree: number;
  outputFree: number;
  parserVersion: string;
  parserUpdated: boolean;
  updating: boolean;
};
export function MaintenancePanel({
  call,
  notify,
  version,
}: {
  call: Call;
  notify: (s: string) => void;
  version: string;
}) {
  const [data, setData] = useState<Maintenance>();
  const [updating, setUpdating] = useState(false);
  const [cleaning, setCleaning] = useState(false);
  const refresh = async () => {
    const r = await call<Maintenance>("maintenance_status");
    if (r) {
      setData(r);
      setUpdating(r.updating);
    }
  };
  useEffect(() => {
    void refresh();
  }, []);
  return (
    <>
      <div className="settings-grid maintenance-grid">
        <section className="setting-section">
          <h3>
            <HardDrive size={17} />
            存储与缓存
          </h3>
          <div className="maintenance-row">
            <span>下载磁盘可用</span>
            <strong>{data ? bytes(data.outputFree) : "—"}</strong>
          </div>
          <div className="maintenance-row">
            <span>缓存磁盘可用</span>
            <strong>{data ? bytes(data.cacheFree) : "—"}</strong>
          </div>
          <div className="maintenance-row">
            <span>下载缓存</span>
            <span>{data ? bytes(data.cacheBytes) : "—"}</span>
          </div>
          <button
            disabled={cleaning || !data?.cacheBytes}
            onClick={async () => {
              setCleaning(true);
              try {
                const n = await call<number>("clear_finished_cache");
                if (n !== undefined) {
                  notify(`已清理 ${n} 个任务缓存`);
                  await refresh();
                }
              } finally {
                setCleaning(false);
              }
            }}
          >
            <Trash2 size={15} />
            清理已完成缓存
          </button>
        </section>
        <section className="setting-section">
          <h3>
            <RefreshCw size={17} />
            解析器
          </h3>
          <div className="maintenance-row">
            <span>yt-dlp</span>
            <strong>{data?.parserVersion || "检测中"}</strong>
          </div>
          <div className="actions wrap">
            <button
              disabled={updating}
              onClick={async () => {
                setUpdating(true);
                try {
                  const v = await call<string>("update_parser");
                  if (v) notify(`解析器已更新至 ${v}`);
                } finally {
                  setUpdating(false);
                  await refresh();
                }
              }}
            >
              {updating ? (
                <RefreshCw className="spin" size={15} />
              ) : (
                <Download size={15} />
              )}
              更新解析器
            </button>
            {updating && (
              <button onClick={() => void call("cancel_parser_update")}>
                取消更新
              </button>
            )}
            {data?.parserUpdated && (
              <button
                disabled={updating}
                onClick={async () => {
                  if ((await call("reset_parser")) !== undefined) {
                    notify("已恢复内置解析器");
                    await refresh();
                  }
                }}
              >
                恢复内置版本
              </button>
            )}
          </div>
        </section>
      </div>
      <section className="setting-section companion-card">
        <span className="companion-mark">
          <Sparkles size={24} />
        </span>
        <div>
          <h3>Bili Live Companion</h3>
          <p className="muted">在掉宝助手中发送回放链接，在这里确认下载。</p>
        </div>
        <button onClick={() => void call("open_companion")}>
          <ExternalLink size={15} />
          打开项目
        </button>
      </section>
      <div className="app-version">BilibiliVideoDownloader · {version}</div>
    </>
  );
}

export function HistoryList({ call }: { call: Call }) {
  const [items, setItems] = useState<
    {
      key: string;
      title: string;
      url: string;
      output: string;
      completed: number;
      format: string;
    }[]
  >([]);
  const [query, setQuery] = useState("");
  useEffect(() => {
    void call<typeof items>("download_history").then((v) => v && setItems(v));
  }, []);
  return (
    <div className="history">
      <input
        className="full"
        aria-label="搜索下载历史"
        placeholder="搜索标题"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <div className="history-list">
        {items
          .filter((i) => i.title.toLowerCase().includes(query.toLowerCase()))
          .map((i) => (
            <div className="history-row" key={i.key}>
              <Check size={16} />
              <div>
                <strong>{i.title}</strong>
                <small>
                  {i.format.toUpperCase()} ·{" "}
                  {new Date(i.completed * 1000).toLocaleDateString()}
                </small>
                <small title={i.output}>{i.output}</small>
              </div>
            </div>
          ))}
        {!items.length && <div className="empty">还没有下载记录</div>}
      </div>
    </div>
  );
}
