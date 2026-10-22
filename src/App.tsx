import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type PointerEvent,
} from "react";
import { invoke, convertFileSrc, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  Plus,
  Download,
  SlidersHorizontal,
  UserRound,
  Settings2,
  Search,
  Pause,
  Play,
  Check,
  AlertCircle,
  Clock3,
  FolderOpen,
  FileVideo,
  Music2,
  MoreHorizontal,
  X,
  RefreshCw,
  LogOut,
  QrCode,
  ClipboardPaste,
  Upload,
  ArrowDownToLine,
  HardDrive,
  ChevronRight,
  Trash2,
  ExternalLink,
  Film,
  Scissors,
  Loader2,
  CheckCheck,
  ChevronDown,
  Minus,
  Square,
} from "lucide-react";
import QRCode from "qrcode";
import icon from "./assets/app-icon.png";
import {
  type Account,
  type MediaItem,
  type Options,
  type ParseState,
  type Settings,
  type Snapshot,
  type Task,
  type Watermark,
  defaults,
  optionsFrom,
  statusName,
  bytes,
  duration,
} from "./types";

type Page = "queue" | "add" | "presets" | "account" | "settings";
const emptyParse: ParseState = {
  running: false,
  items: [],
  errors: [],
  processed: 0,
  total: 0,
};
const active = ["resolving", "downloading", "retrying", "converting"];
const nav = [
  { id: "add" as Page, label: "新建下载", Icon: Plus },
  { id: "queue" as Page, label: "下载队列", Icon: Download },
  { id: "presets" as Page, label: "下载预设", Icon: SlidersHorizontal },
  { id: "account" as Page, label: "账号", Icon: UserRound },
];

function Button({
  children,
  onClick,
  primary = false,
  disabled = false,
  title,
  className = "",
}: {
  children: ReactNode;
  onClick?: () => void;
  primary?: boolean;
  disabled?: boolean;
  title?: string;
  className?: string;
}) {
  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      className={`${primary ? "primary" : ""} ${className}`}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
function Modal({
  title,
  children,
  onClose,
  wide = false,
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  wide?: boolean;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    ref.current?.showModal();
    return () => ref.current?.close();
  }, []);
  return (
    <dialog
      aria-label={title}
      className={wide ? "wide" : ""}
      ref={ref}
      onCancel={onClose}
    >
      <div className="modal-title">
        <h2>{title}</h2>
        <Button title="关闭" onClick={onClose}>
          <X size={19} />
        </Button>
      </div>
      {children}
    </dialog>
  );
}
function Empty({
  icon: Icon = Download,
  title,
  children,
}: {
  icon?: typeof Download;
  title: string;
  children?: ReactNode;
}) {
  return (
    <div className="empty">
      <div className="empty-icon">
        <Icon size={32} strokeWidth={1.4} />
      </div>
      <h3>{title}</h3>
      {children}
    </div>
  );
}
function VirtualList<T>({
  items,
  rowHeight,
  render,
}: {
  items: T[];
  rowHeight: number;
  render: (item: T, i: number) => ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [top, setTop] = useState(0);
  const [height, setHeight] = useState(500);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setHeight(el.clientHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const start = Math.max(0, Math.floor(top / rowHeight) - 3),
    end = Math.min(items.length, Math.ceil((top + height) / rowHeight) + 3);
  useEffect(() => {
    if (ref.current && top > items.length * rowHeight) {
      ref.current.scrollTop = 0;
      setTop(0);
    }
  }, [items.length, rowHeight, top]);
  return (
    <div
      className="virtual-list"
      ref={ref}
      onScroll={(e) => setTop(e.currentTarget.scrollTop)}
    >
      <div style={{ height: items.length * rowHeight, position: "relative" }}>
        {items.slice(start, end).map((t, i) => (
          <div
            key={start + i}
            style={{
              position: "absolute",
              top: (start + i) * rowHeight,
              height: rowHeight,
              width: "100%",
            }}
          >
            {render(t, start + i)}
          </div>
        ))}
      </div>
    </div>
  );
}

export default function App() {
  const [page, setPage] = useState<Page>("queue");
  const [snap, setSnap] = useState<Snapshot>({
    tasks: [],
    settings: defaults,
    parse: emptyParse,
    connections: 12,
    version: "0.1.1",
  });
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [connected, setConnected] = useState(false);
  const [busy, setBusy] = useState(false);
  const [draft, setDraft] = useState("");
  const [detail, setDetail] = useState<string | null>(null);
  const [processing, setProcessing] = useState<Task | null>(null);
  const [options, setOptions] = useState<Options>(optionsFrom(defaults));
  const initial = useRef(false);
  const refresh = useCallback(async () => {
    if (!isTauri()) return;
    try {
      const data = await invoke<Snapshot>("snapshot");
      setSnap(data);
      setConnected(true);
      if (!initial.current) {
        setOptions(optionsFrom(data.settings));
        initial.current = true;
      }
    } catch (e) {
      setError(String(e));
    }
  }, []);
  useEffect(() => {
    let done = false;
    async function poll() {
      await refresh();
      if (!done) timer = setTimeout(poll, 900);
    }
    let timer: ReturnType<typeof setTimeout>;
    void poll();
    return () => {
      done = true;
      clearTimeout(timer);
    };
  }, [refresh]);
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(""), 4000);
    return () => clearTimeout(timer);
  }, [notice]);
  const call = useCallback(
    async <T,>(
      command: string,
      args?: Record<string, unknown>,
    ): Promise<T | undefined> => {
      if (!isTauri()) {
        setError("请在桌面应用中操作");
        return;
      }
      try {
        setError("");
        const result = await invoke<T>(command, args);
        await refresh();
        return result;
      } catch (e) {
        setError(String(e));
        return undefined;
      }
    },
    [refresh],
  );
  const navigate = (next: Page, el?: HTMLButtonElement) => {
    setPage(next);
    if (!matchMedia("(prefers-reduced-motion: reduce)").matches) {
      el?.querySelector("svg")?.animate(
        [
          { transform: "scale(1)" },
          { transform: "scale(.84)" },
          { transform: "scale(1.12)" },
          { transform: "scale(1)" },
        ],
        { duration: 240, easing: "ease-out" },
      );
    }
  };
  const action = async (id: string | string[], action: string) => {
    await call(
      "task_action",
      Array.isArray(id) ? { ids: id, action } : { id, action },
    );
  };
  const task = snap.tasks.find((t) => t.id === detail);
  const windowAction = async (
    action: "minimize" | "toggleMaximize" | "close",
  ) => {
    if (!isTauri()) return;
    try {
      await getCurrentWindow()[action]();
    } catch (e) {
      setError(String(e));
    }
  };
  return (
    <div className="app">
      <header className="titlebar" data-tauri-drag-region>
        <img src={icon} alt="" draggable={false} data-tauri-drag-region />
        <span data-tauri-drag-region>BilibiliVideoDownloader</span>
        <div className="titlebar-space" data-tauri-drag-region />
        <div className="window-controls" aria-label="窗口控制">
          <Button title="最小化" onClick={() => void windowAction("minimize")}>
            <Minus size={16} />
          </Button>
          <Button
            title="最大化或还原"
            onClick={() => void windowAction("toggleMaximize")}
          >
            <Square size={13} />
          </Button>
          <Button
            title="关闭窗口"
            className="window-close"
            onClick={() => void windowAction("close")}
          >
            <X size={17} />
          </Button>
        </div>
      </header>
      <div className="shell">
        <aside>
          <nav>
            {nav.map(({ id, label, Icon }) => (
              <button
                className={page === id ? "nav-item active" : "nav-item"}
                key={id}
                aria-current={page === id ? "page" : undefined}
                onClick={(e) => navigate(id, e.currentTarget)}
              >
                <Icon size={19} />
                {label}
                {id === "queue" &&
                  snap.tasks.filter((t) => active.includes(t.status)).length >
                    0 && (
                    <span className="nav-count">
                      {
                        snap.tasks.filter((t) => active.includes(t.status))
                          .length
                      }
                    </span>
                  )}
              </button>
            ))}
          </nav>
          <button
            className={`nav-item bottom ${page === "settings" ? "active" : ""}`}
            aria-current={page === "settings" ? "page" : undefined}
            onClick={(e) => navigate("settings", e.currentTarget)}
          >
            <Settings2 size={19} />
            设置
          </button>
        </aside>
        <main>
          {error && (
            <div className="message error-message" role="alert">
              <AlertCircle size={18} />
              <span>{error}</span>
              <Button title="关闭提示" onClick={() => setError("")}>
                <X size={16} />
              </Button>
            </div>
          )}
          {notice && (
            <div className="toast" role="status">
              <Check size={16} />
              {notice}
            </div>
          )}
          {!connected && (
            <div className="message neutral">
              <Loader2 size={16} className={isTauri() ? "spin" : ""} />
              {isTauri() ? "正在连接下载引擎" : "桌面界面预览"}
            </div>
          )}
          {page === "queue" && (
            <Queue
              tasks={snap.tasks}
              onAdd={() => setPage("add")}
              onAction={action}
              onDetail={setDetail}
              onOpen={(id, folder) => void call("open_output", { id, folder })}
            />
          )}
          {page === "add" && (
            <Add
              input={draft}
              setInput={setDraft}
              parse={snap.parse}
              options={options}
              setOptions={setOptions}
              busy={busy}
              onParse={async (input) => {
                if ((await call("clear_parse")) === undefined) return;
                await call("parse_inputs", { input });
              }}
              error={error}
              onCancel={() => void call("cancel_parse")}
              onImport={async () => {
                const path = await open({
                  filters: [{ name: "链接列表", extensions: ["txt"] }],
                  multiple: false,
                });
                return path
                  ? await call<string>("read_text", { path })
                  : undefined;
              }}
              onEnqueue={async (items) => {
                setBusy(true);
                const n = await call<number>("enqueue", { items, options });
                setBusy(false);
                if (n !== undefined) {
                  setNotice(
                    `已添加 ${n} 个任务${n < items.length ? "，重复项已跳过" : ""}`,
                  );
                  setPage("queue");
                }
              }}
            />
          )}
          {page === "presets" && (
            <div className="page-scroll">
              <div className="page-heading">
                <h1>下载预设</h1>
                <Button
                  primary
                  onClick={async () => {
                    const next = {
                      ...snap.settings,
                      format: options.format,
                      quality: options.quality,
                      bitrate: options.bitrate,
                      cover: options.cover,
                      subtitles: options.subtitles,
                      danmaku: options.danmaku,
                      template: options.template,
                    };
                    if (
                      (await call("save_settings", { settings: next })) !==
                      undefined
                    )
                      setNotice("预设已保存");
                  }}
                >
                  保存预设
                </Button>
              </div>
              <OptionsForm options={options} onChange={setOptions} advanced />
              <div className="setting-section">
                <h3>
                  <Scissors size={18} />
                  画面处理
                </h3>
                <p className="muted">
                  从已下载任务的详情中打开画面处理，框选区域并预览。
                </p>
              </div>
            </div>
          )}
          {page === "account" && <AccountPage call={call} notify={setNotice} />}
          {page === "settings" && (
            <SettingsPage
              settings={snap.settings}
              onSave={async (settings) => {
                if ((await call("save_settings", { settings })) !== undefined)
                  setNotice("设置已保存");
              }}
              onExport={async () => {
                const path = await save({
                  defaultPath: "bvd-diagnostics.json",
                  filters: [{ name: "诊断信息", extensions: ["json"] }],
                });
                if (path) {
                  if ((await call("export_logs", { path })) !== undefined)
                    setNotice("诊断信息已导出");
                }
              }}
            />
          )}
        </main>
      </div>
      {task && (
        <Modal title="任务详情" onClose={() => setDetail(null)}>
          <div className="detail-title">{task.item.title}</div>
          <div className={`status ${task.status}`}>
            <StatusIcon status={task.status} />
            {statusName[task.status]}
            {task.status === "converting" && ` · ${task.stage}`}
          </div>
          <dl className="details">
            <dt>格式</dt>
            <dd>
              {task.options.format.toUpperCase()} ·{" "}
              {task.options.quality || "最高可用"}
              {task.options.quality ? "P" : ""}
            </dd>
            <dt>下载量</dt>
            <dd>
              {bytes(task.downloaded)} / {task.total ? bytes(task.total) : "—"}
            </dd>
            <dt>保存位置</dt>
            <dd>{task.output}</dd>
            <dt>来源</dt>
            <dd>{task.item.url}</dd>
          </dl>
          {task.error && <div className="error-box">{task.error}</div>}
          {task.warnings.map((w, i) => (
            <div className="warning-box" key={i}>
              {w}
            </div>
          ))}
          <div className="actions wrap">
            <Button
              onClick={() =>
                void call("open_output", { id: task.id, folder: true })
              }
            >
              <FolderOpen size={17} />
              打开目录
            </Button>
            {task.status === "completed" &&
              !["mp3", "m4a", "wav", "flac"].includes(task.options.format) && (
                <Button
                  onClick={() => {
                    setProcessing(task);
                    setDetail(null);
                  }}
                >
                  <Scissors size={17} />
                  画面处理
                </Button>
              )}
            {task.status === "completed" && (
              <Button
                onClick={() =>
                  void call("clear_cache", { id: task.id }).then((result) => {
                    if (result !== undefined) setNotice("缓存已清理");
                  })
                }
              >
                <Trash2 size={17} />
                清理缓存
              </Button>
            )}
            {!active.includes(task.status) && (
              <Button
                className="danger"
                onClick={() => {
                  void action(task.id, "remove");
                  setDetail(null);
                }}
              >
                <Trash2 size={17} />
                移除任务
              </Button>
            )}
          </div>
        </Modal>
      )}
      {processing && (
        <WatermarkEditor
          task={processing}
          call={call}
          onClose={() => setProcessing(null)}
          onApply={async (w) => {
            const result = await call("reprocess", {
              id: processing.id,
              options: { ...processing.options, watermark: w },
            });
            if (result !== undefined) {
              setProcessing(null);
              setPage("queue");
            }
          }}
        />
      )}
    </div>
  );
}

function StatusIcon({ status }: { status: string }) {
  if (status === "completed") return <Check size={14} />;
  if (status === "error") return <AlertCircle size={14} />;
  if (status === "paused") return <Pause size={13} fill="currentColor" />;
  if (status === "queued" || status === "cancelled")
    return <Clock3 size={14} />;
  if (status === "converting" || status === "resolving")
    return <Loader2 size={14} className="spin" />;
  return <ArrowDownToLine size={14} />;
}
function Queue({
  tasks,
  onAdd,
  onAction,
  onDetail,
  onOpen,
}: {
  tasks: Task[];
  onAdd: () => void;
  onAction: (id: string | string[], action: string) => void;
  onDetail: (id: string) => void;
  onOpen: (id: string, folder: boolean) => void;
}) {
  const [filter, setFilter] = useState("all");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const groups = [
    ["all", "全部"],
    ["active", "进行中"],
    ["queued", "排队"],
    ["paused", "暂停"],
    ["error", "出错"],
    ["completed", "完成"],
  ];
  useEffect(() => setSelected(new Set()), [filter, query]);
  const matches = (t: Task, key: string) =>
    key === "all" ||
    (key === "active" ? active.includes(t.status) : t.status === key);
  const visible = tasks.filter(
    (t) =>
      matches(t, filter) &&
      `${t.item.title} ${t.item.uploader}`
        .toLowerCase()
        .includes(query.toLowerCase()),
  );
  const speed = tasks.reduce((n, t) => n + t.speed, 0);
  const running = tasks.filter((t) => active.includes(t.status));
  const total = running.reduce((n, t) => n + t.total, 0),
    done = running.reduce((n, t) => n + t.downloaded, 0);
  const eta = speed ? Math.max(0, (total - done) / speed) : 0;
  const toggle = (id: string) =>
    setSelected((s) => {
      const n = new Set(s);
      n.has(id) ? n.delete(id) : n.add(id);
      return n;
    });
  const targets = (action: string) =>
    visible
      .filter(
        (t) =>
          (!selected.size || selected.has(t.id)) &&
          (action === "pause"
            ? [...active, "queued"].includes(t.status)
            : action === "retry"
              ? t.status === "error"
              : t.status === "paused"),
      )
      .map((t) => t.id);
  return (
    <div className="queue-page">
      <div className="page-heading">
        <div>
          <h1>下载队列</h1>
          <span className="muted">{tasks.length} 个任务</span>
        </div>
        <Button primary onClick={onAdd}>
          <Plus size={17} />
          新建下载
        </Button>
      </div>
      <div className="transfer-bar">
        <div>
          <ArrowDownToLine size={19} />
          <strong>{speed ? bytes(speed).split(" ")[0] : "0"}</strong>
          <span>{speed ? bytes(speed).split(" ")[1] : "MB"}/s</span>
        </div>
        <div>
          <HardDrive size={17} />
          {bytes(done)} / {bytes(total)}
        </div>
        <div>
          <Clock3 size={17} />
          {eta ? `约 ${Math.ceil(eta / 60)} 分钟` : "—"}
        </div>
      </div>
      <div className="filterbar">
        <div className="tabs">
          {groups.map(([id, label]) => (
            <button
              key={id}
              className={filter === id ? "selected" : ""}
              onClick={() => setFilter(id)}
            >
              {id !== "all" && <span className={`dot ${id}`} />}
              <span>{label}</span>
              <span className="count">
                {tasks.filter((t) => matches(t, id)).length}
              </span>
            </button>
          ))}
        </div>
        <div className="search">
          <Search size={16} />
          <input
            aria-label="搜索任务"
            placeholder="搜索任务"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
      </div>
      {tasks.length > 0 && (
        <div className="queue-actions">
          <label className="check">
            <input
              type="checkbox"
              aria-label="选择当前列表"
              checked={
                visible.length > 0 && visible.every((t) => selected.has(t.id))
              }
              onChange={(e) =>
                setSelected(
                  e.target.checked
                    ? new Set(visible.map((t) => t.id))
                    : new Set(),
                )
              }
            />
            {selected.size ? `已选 ${selected.size} 项` : "全选"}
          </label>
          <div className="actions">
            <Button
              title="暂停下载"
              disabled={!targets("pause").length}
              onClick={() => onAction(targets("pause"), "pause")}
            >
              <Pause size={15} />
            </Button>
            <Button
              title="继续下载"
              disabled={!targets("resume").length}
              onClick={() => onAction(targets("resume"), "resume")}
            >
              <Play size={15} />
            </Button>
            <Button
              title="重试出错任务"
              disabled={!targets("retry").length}
              onClick={() => onAction(targets("retry"), "retry")}
            >
              <RefreshCw size={15} />
            </Button>
          </div>
        </div>
      )}
      {!visible.length ? (
        <Empty
          title={tasks.length ? "没有符合条件的任务" : "开始你的第一次下载"}
        >
          {!tasks.length && (
            <Button primary onClick={onAdd}>
              <Plus size={17} />
              添加链接
            </Button>
          )}
        </Empty>
      ) : (
        <VirtualList
          items={visible}
          rowHeight={108}
          render={(t) => (
            <div className="task-row">
              <input
                type="checkbox"
                aria-label={`选择 ${t.item.title}`}
                checked={selected.has(t.id)}
                onChange={() => toggle(t.id)}
              />
              <div className="thumbnail">
                {t.item.thumbnail ? (
                  <img
                    src={t.item.thumbnail}
                    alt=""
                    referrerPolicy="no-referrer"
                    onError={(e) =>
                      (e.currentTarget.style.visibility = "hidden")
                    }
                  />
                ) : ["mp3", "m4a", "flac", "wav"].includes(t.options.format) ? (
                  <Music2 size={24} />
                ) : (
                  <FileVideo size={24} />
                )}
              </div>
              <div className="task-main">
                <div className="task-title" title={t.item.title}>
                  {t.item.title}
                </div>
                <div className="task-meta">
                  <span>
                    {t.options.format.toUpperCase()} ·{" "}
                    {t.options.quality ? `${t.options.quality}P` : "最高可用"}
                  </span>
                  <span className={`status ${t.status}`}>
                    <StatusIcon status={t.status} />
                    {t.status === "converting" ? t.stage : statusName[t.status]}
                    {t.status === "downloading" && t.total > 0
                      ? ` · ${Math.min(100, Math.floor((t.downloaded / t.total) * 100))}%`
                      : ""}
                  </span>
                </div>
                {["downloading", "paused", "retrying"].includes(t.status) && (
                  <div
                    className={`progress ${t.status}`}
                    role="progressbar"
                    aria-label="下载进度"
                    aria-valuenow={
                      t.total
                        ? Math.min(
                            100,
                            Math.round((t.downloaded / t.total) * 100),
                          )
                        : 0
                    }
                    aria-valuemin={0}
                    aria-valuemax={100}
                  >
                    <span
                      style={{
                        width: `${t.total ? Math.min(100, (t.downloaded / t.total) * 100) : 0}%`,
                      }}
                    />
                  </div>
                )}
                {t.status === "error" ? (
                  <div className="task-error" title={t.error}>
                    {t.error}
                  </div>
                ) : t.status === "downloading" ? (
                  <div className="task-meta small">
                    <span>
                      {bytes(t.downloaded)} / {t.total ? bytes(t.total) : "—"}
                    </span>
                    <span>{bytes(t.speed)}/s</span>
                  </div>
                ) : null}
              </div>
              <div className="task-actions">
                {t.status === "completed" ? (
                  <Button title="打开文件" onClick={() => onOpen(t.id, false)}>
                    <Play size={16} />
                  </Button>
                ) : active.includes(t.status) || t.status === "queued" ? (
                  <Button
                    title="暂停任务"
                    onClick={() => onAction(t.id, "pause")}
                  >
                    <Pause size={16} />
                  </Button>
                ) : (
                  <Button
                    title={t.status === "error" ? "重试任务" : "继续任务"}
                    onClick={() =>
                      onAction(t.id, t.status === "error" ? "retry" : "resume")
                    }
                  >
                    {t.status === "error" ? (
                      <RefreshCw size={16} />
                    ) : (
                      <Play size={16} />
                    )}
                  </Button>
                )}
                <Button title="任务详情" onClick={() => onDetail(t.id)}>
                  <MoreHorizontal size={18} />
                </Button>
              </div>
            </div>
          )}
        />
      )}
    </div>
  );
}

function OptionsForm({
  options: o,
  onChange,
  advanced = false,
}: {
  options: Options;
  onChange: (o: Options) => void;
  advanced?: boolean;
}) {
  const set = <K extends keyof Options>(key: K, value: Options[K]) =>
    onChange({ ...o, [key]: value });
  return (
    <>
      <div className="setting-section">
        <h3>
          <Film size={18} />
          输出选项
        </h3>
        <div className="form-grid">
          <label className="span2">
            下载内容
            <select
              value={o.resource}
              onChange={(e) =>
                set("resource", e.target.value as Options["resource"])
              }
            >
              <option value="media">视频 / 音频与所选附件</option>
              <option value="extras">仅保存所选附件</option>
            </select>
          </label>
          <label>
            保存格式
            <select
              disabled={o.resource === "extras"}
              value={o.format}
              onChange={(e) => set("format", e.target.value)}
            >
              {["mp4", "mkv", "mp3", "m4a", "flac", "wav"].map((f) => (
                <option key={f} value={f}>
                  {f.toUpperCase()}
                </option>
              ))}
            </select>
          </label>
          <label>
            画质
            <select
              disabled={o.resource === "extras"}
              value={o.quality}
              onChange={(e) => set("quality", Number(e.target.value))}
            >
              {[
                [0, "最高可用"],
                [2160, "4K"],
                [1440, "2K"],
                [1080, "1080P"],
                [720, "720P"],
                [480, "480P"],
              ].map(([v, l]) => (
                <option key={v} value={v}>
                  {l}
                </option>
              ))}
            </select>
          </label>
          {["mp3", "m4a"].includes(o.format) && (
            <label>
              音频码率
              <select
                value={o.bitrate}
                onChange={(e) => set("bitrate", Number(e.target.value))}
              >
                {[128, 192, 256, 320].map((v) => (
                  <option key={v} value={v}>
                    {v} kbps
                  </option>
                ))}
              </select>
            </label>
          )}
          <label className="span2">
            保存目录
            <div className="input-action">
              <input
                value={o.directory}
                onChange={(e) => set("directory", e.target.value)}
                placeholder="选择下载目录"
              />
              <Button
                title="选择目录"
                onClick={async () => {
                  if (!isTauri()) return;
                  const p = await open({ directory: true, multiple: false });
                  if (p) set("directory", p);
                }}
              >
                <FolderOpen size={18} />
              </Button>
            </div>
          </label>
        </div>
        <div className="check-row">
          <label className="check">
            <input
              type="checkbox"
              checked={o.cover}
              onChange={(e) => set("cover", e.target.checked)}
            />
            封面
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={o.subtitles}
              onChange={(e) => set("subtitles", e.target.checked)}
            />
            字幕
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={o.danmaku}
              onChange={(e) => set("danmaku", e.target.checked)}
            />
            弹幕
          </label>
          {["mp4", "mkv"].includes(o.format) && (
            <label className="check">
              <input
                type="checkbox"
                checked={o.compatibility}
                onChange={(e) => set("compatibility", e.target.checked)}
              />
              兼容编码 H.264
            </label>
          )}
        </div>
      </div>
      {advanced && (
        <div className="setting-section">
          <h3>命名规则</h3>
          <input
            className="full"
            aria-label="命名模板"
            value={o.template}
            onChange={(e) => set("template", e.target.value)}
          />
          <div className="token-row">
            {["{UP主}", "{标题}", "{P序号}", "{分P标题}", "{BV号}"].map((t) => (
              <button key={t} onClick={() => set("template", o.template + t)}>
                {t}
              </button>
            ))}
          </div>
        </div>
      )}
    </>
  );
}
function Add({
  input,
  setInput,
  parse,
  options,
  setOptions,
  busy,
  onParse,
  onCancel,
  error,
  onImport,
  onEnqueue,
}: {
  input: string;
  setInput: (value: string | ((previous: string) => string)) => void;
  parse: ParseState;
  options: Options;
  setOptions: (o: Options) => void;
  busy: boolean;
  onParse: (s: string) => Promise<void>;
  onCancel: () => void;
  error: string;
  onImport: () => Promise<string | undefined>;
  onEnqueue: (items: MediaItem[]) => void;
}) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [search, setSearch] = useState("");
  const known = useRef(new Set<string>());
  const [expanded, setExpanded] = useState(false);
  const [showResults, setShowResults] = useState(false);
  const [starting, setStarting] = useState(false);
  const closeRequested = useRef(false);
  useEffect(() => {
    const newItems = parse.items.filter((i) => !known.current.has(i.url));
    if (newItems.length) {
      setSelected((s) => new Set([...s, ...newItems.map((i) => i.url)]));
      newItems.forEach((i) => known.current.add(i.url));
    }
    if (!parse.items.length) {
      known.current.clear();
      setSelected(new Set());
    }
  }, [parse.items]);
  const filtered = parse.items.filter((i) =>
    i.title.toLowerCase().includes(search.toLowerCase()),
  );
  return (
    <div className="add-page">
      <div className="page-heading">
        <h1>新建下载</h1>
      </div>
      <div className="link-input">
        <textarea
          aria-label="视频链接"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="粘贴链接或 BV / AV 号，每行一个"
        />
        <div className="link-actions">
          <Button
            onClick={async () => {
              const text = await onImport();
              if (text) setInput((s) => (s ? `${s}\n${text}` : text));
            }}
          >
            <Upload size={16} />
            导入 TXT
          </Button>
          <Button
            primary
            disabled={!input.trim() || parse.running || starting}
            onClick={async () => {
              setShowResults(true);
              closeRequested.current = false;
              setStarting(true);
              setSelected(new Set());
              known.current.clear();
              setSearch("");
              setExpanded(false);
              try {
                await onParse(input);
              } finally {
                if (closeRequested.current) onCancel();
                setStarting(false);
              }
            }}
          >
            <Search size={16} />
            解析链接
          </Button>
        </div>
      </div>
      {showResults && (
        <Modal
          title="选择下载内容"
          wide
          onClose={() => {
            closeRequested.current = true;
            if (parse.running) onCancel();
            setShowResults(false);
          }}
        >
          <div className="parse-dialog-body">
            {error && (
              <div className="message error-message" role="alert">
                <AlertCircle size={17} />
                <span>{error}</span>
              </div>
            )}
            <button
              className="options-toggle"
              onClick={() => setExpanded(!expanded)}
            >
              <SlidersHorizontal size={16} />
              {options.format.toUpperCase()} ·{" "}
              {options.quality ? `${options.quality}P` : "最高可用"}
              <span>{options.directory || "选择保存目录"}</span>
              <ChevronDown
                size={16}
                style={{ transform: expanded ? "rotate(180deg)" : undefined }}
              />
            </button>
            {expanded && (
              <div className="options-expanded">
                <OptionsForm options={options} onChange={setOptions} />
              </div>
            )}
            {(parse.running || starting) && (
              <div className="parse-progress">
                <Loader2 size={16} className="spin" />
                解析中 {parse.processed} / {parse.total} · 已找到{" "}
                {parse.items.length} 项
                {parse.running && (
                  <Button title="停止解析" onClick={onCancel}>
                    <X size={15} />
                  </Button>
                )}
              </div>
            )}
            {parse.errors.length > 0 && (
              <details className="parse-errors">
                <summary>{parse.errors.length} 项解析失败</summary>
                {parse.errors.map((e, i) => (
                  <p key={i}>{e}</p>
                ))}
              </details>
            )}
            {parse.items.length > 0 ? (
              <>
                <div className="results-heading">
                  <label className="check">
                    <input
                      type="checkbox"
                      checked={
                        filtered.length > 0 &&
                        filtered.every((i) => selected.has(i.url))
                      }
                      onChange={(e) =>
                        setSelected(
                          e.target.checked
                            ? new Set([
                                ...selected,
                                ...filtered.map((i) => i.url),
                              ])
                            : new Set(
                                [...selected].filter(
                                  (u) => !filtered.some((i) => i.url === u),
                                ),
                              ),
                        )
                      }
                    />
                    已选 {selected.size} / {parse.items.length}
                  </label>
                  <div className="search">
                    <Search size={15} />
                    <input
                      placeholder="筛选视频"
                      aria-label="筛选视频"
                      value={search}
                      onChange={(e) => setSearch(e.target.value)}
                    />
                  </div>
                </div>
                <VirtualList
                  items={filtered}
                  rowHeight={66}
                  render={(item) => (
                    <label className="parse-row">
                      <input
                        type="checkbox"
                        checked={selected.has(item.url)}
                        onChange={() =>
                          setSelected((s) => {
                            const n = new Set(s);
                            n.has(item.url)
                              ? n.delete(item.url)
                              : n.add(item.url);
                            return n;
                          })
                        }
                      />
                      <span className="index">
                        {item.index.toString().padStart(2, "0")}
                      </span>
                      <span className="parse-title">
                        <strong>{item.title}</strong>
                        <small>{item.uploader}</small>
                      </span>
                      <span className="muted">
                        {item.duration ? duration(item.duration) : ""}
                      </span>
                    </label>
                  )}
                />
                <div className="add-footer">
                  <span className="muted">{options.format.toUpperCase()}</span>
                  <Button
                    primary
                    disabled={
                      !selected.size ||
                      busy ||
                      parse.running ||
                      starting ||
                      !options.directory
                    }
                    onClick={() =>
                      onEnqueue(parse.items.filter((i) => selected.has(i.url)))
                    }
                  >
                    {busy ? (
                      <Loader2 className="spin" size={17} />
                    ) : (
                      <Plus size={17} />
                    )}
                    开始下载 · {selected.size}
                  </Button>
                </div>
              </>
            ) : !parse.running && !starting && !error ? (
              <Empty icon={Search} title="没有找到可下载内容" />
            ) : null}
          </div>
        </Modal>
      )}
    </div>
  );
}

type Call = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T | undefined>;
function AccountPage({
  call,
  notify,
}: {
  call: Call;
  notify: (s: string) => void;
}) {
  const [user, setUser] = useState<Account | null>(null);
  const [method, setMethod] = useState<"qr" | "cookie" | null>(null);
  const [qr, setQr] = useState("");
  const [key, setKey] = useState("");
  const [qrStatus, setQrStatus] = useState("");
  const [cookie, setCookie] = useState("");
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    let stopped = false;
    void (async () => {
      const saved = await call<Account | null>("cached_account");
      if (saved && !stopped) setUser(saved);
      const current = await call<Account>("account");
      if (current && !stopped) setUser(current);
    })();
    return () => {
      stopped = true;
    };
  }, [call]);
  useEffect(() => {
    if (!key) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      const result = await call<{ code: number; account?: Account }>(
        "poll_qr",
        { key },
      );
      if (stopped) return;
      if (result?.code === 0) {
        setUser(result.account!);
        setMethod(null);
        setKey("");
        setQr("");
        notify("登录成功");
        return;
      }
      if (result?.code === 86038) {
        setQrStatus("二维码已过期");
        setKey("");
        return;
      }
      setQrStatus(
        result?.code === 86090
          ? "已扫码，请在手机上确认"
          : "使用哔哩哔哩 App 扫码",
      );
      timer = setTimeout(poll, 2500);
    };
    timer = setTimeout(poll, 2000);
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [key, call, notify]);
  const generate = async () => {
    setMethod("qr");
    setLoading(true);
    setQr("");
    setKey("");
    const result = await call<{ url: string; qrcode_key: string }>("qr_login");
    if (result) {
      setQr(
        await QRCode.toDataURL(result.url, {
          margin: 1,
          width: 224,
          color: { dark: "#222532", light: "#ffffff" },
        }),
      );
      setKey(result.qrcode_key);
      setQrStatus("使用哔哩哔哩 App 扫码");
    }
    setLoading(false);
  };
  return (
    <div className="page-scroll">
      <div className="page-heading">
        <h1>账号</h1>
        <Button
          title="刷新登录状态"
          onClick={() =>
            void call<Account>("account").then((v) => v && setUser(v))
          }
        >
          <RefreshCw size={17} />
        </Button>
      </div>
      <div className="account-card">
        <div className="avatar">
          {user?.avatar ? (
            <img
              src={convertFileSrc(user.avatar)}
              alt="账号头像"
              onError={(e) => {
                e.currentTarget.style.display = "none";
              }}
            />
          ) : (
            <UserRound size={29} />
          )}
        </div>
        <div>
          <h2>{user?.loggedIn ? user.name : "哔哩哔哩账号"}</h2>
          {user?.loggedIn ? (
            <span className={`status ${user.cached ? "paused" : "completed"}`}>
              <Check size={14} />
              {user.cached ? "登录状态待验证" : "已登录"}
              {user.vip ? " · 大会员" : ""}
            </span>
          ) : (
            <span className="muted">未登录</span>
          )}
        </div>
        {user?.loggedIn && (
          <Button
            className="account-logout"
            onClick={() =>
              void call("logout").then((result) => {
                if (result !== undefined) {
                  setUser(null);
                  notify("已退出登录");
                }
              })
            }
          >
            <LogOut size={17} />
            退出
          </Button>
        )}
      </div>
      {(!user?.loggedIn || method) && (
        <div className="actions">
          <Button primary onClick={() => void generate()}>
            <QrCode size={17} />
            扫码登录
          </Button>
          <Button
            onClick={() => {
              setMethod("cookie");
              setKey("");
            }}
          >
            <ClipboardPaste size={17} />
            Cookie 导入
          </Button>
        </div>
      )}
      {method === "qr" && (
        <div className="login-panel">
          <div className="qr-frame">
            {loading ? (
              <Loader2 className="spin" size={26} />
            ) : qr ? (
              <img src={qr} alt="B站登录二维码" />
            ) : (
              <QrCode size={80} />
            )}
          </div>
          <p className="muted">{qrStatus}</p>
          <Button onClick={() => void generate()} disabled={loading}>
            <RefreshCw size={15} />
            刷新二维码
          </Button>
        </div>
      )}
      {method === "cookie" && (
        <div className="setting-section">
          <h3>导入 Cookie</h3>
          <textarea
            className="cookie-input"
            aria-label="Cookie 内容"
            autoComplete="off"
            spellCheck={false}
            placeholder="Cookie 请求头、Netscape 文件或 JSON"
            value={cookie}
            onChange={(e) => setCookie(e.target.value)}
          />
          <div className="actions">
            <Button
              onClick={async () => {
                const path = await open({
                  filters: [
                    { name: "Cookie", extensions: ["txt", "json", "cookies"] },
                  ],
                  multiple: false,
                });
                if (path) {
                  const s = await call<string>("read_text", { path });
                  if (s) setCookie(s);
                }
              }}
            >
              <Upload size={16} />
              导入文件
            </Button>
            <Button
              primary
              disabled={!cookie.trim() || loading}
              onClick={async () => {
                setLoading(true);
                const account = await call<Account>("import_cookie", {
                  cookie,
                });
                setLoading(false);
                if (account) {
                  setUser(account);
                  setCookie("");
                  setMethod(null);
                  notify("登录成功");
                }
              }}
            >
              {loading ? (
                <Loader2 className="spin" size={16} />
              ) : (
                <Check size={16} />
              )}
              验证并保存
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

function SettingsPage({
  settings,
  onSave,
  onExport,
}: {
  settings: Settings;
  onSave: (s: Settings) => void;
  onExport: () => void;
}) {
  const [form, setForm] = useState(settings);
  const set = <K extends keyof Settings>(key: K, value: Settings[K]) =>
    setForm((f) => ({ ...f, [key]: value }));
  return (
    <div className="page-scroll">
      <div className="page-heading">
        <h1>设置</h1>
        <Button primary onClick={() => onSave(form)}>
          保存设置
        </Button>
      </div>
      <div className="setting-section">
        <h3>并发与恢复</h3>
        <div className="form-grid">
          {(
            [
              ["tasks", "同时下载任务", 1, 16],
              ["connections", "单资源连接", 1, 8],
              ["globalConnections", "全局下载连接", 1, 64],
              ["conversions", "同时转换任务", 1, 4],
              ["retries", "失败重试次数", 0, 10],
              ["rateLimitKib", "下载限速 · KiB/s，0 不限速", 0, 1048576],
            ] as const
          ).map(([k, l, min, max]) => (
            <label key={k}>
              {l}
              <input
                type="number"
                min={min}
                max={max}
                value={form[k]}
                onChange={(e) => set(k, Number(e.target.value))}
              />
            </label>
          ))}
        </div>
        <div className="check-row">
          <label className="check">
            <input
              type="checkbox"
              checked={form.resumeOnStart}
              onChange={(e) => set("resumeOnStart", e.target.checked)}
            />
            启动后恢复任务
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={form.tray}
              onChange={(e) => set("tray", e.target.checked)}
            />
            关闭窗口后留在托盘
          </label>
        </div>
      </div>
      <div className="setting-section">
        <h3>默认保存位置</h3>
        <div className="input-action">
          <input
            aria-label="默认保存目录"
            value={form.directory}
            onChange={(e) => set("directory", e.target.value)}
          />
          <Button
            title="选择目录"
            onClick={async () => {
              const p = await open({ directory: true, multiple: false });
              if (p) set("directory", p);
            }}
          >
            <FolderOpen size={18} />
          </Button>
        </div>
      </div>
      <div className="setting-section diagnostics">
        <div>
          <h3>诊断信息</h3>
          <span className="muted">v0.1.0</span>
        </div>
        <Button onClick={onExport}>
          <Upload size={16} />
          导出
        </Button>
      </div>
    </div>
  );
}

function WatermarkEditor({
  task,
  call,
  onClose,
  onApply,
}: {
  task: Task;
  call: Call;
  onClose: () => void;
  onApply: (w: Watermark) => void;
}) {
  const [w, setW] = useState<Watermark>({
    ...task.options.watermark,
    mode: "delogo",
  });
  const [frame, setFrame] = useState<{
    path: string;
    width: number;
    height: number;
  } | null>(null);
  const [second, setSecond] = useState(1);
  const [preview, setPreview] = useState("");
  const [loading, setLoading] = useState(false);
  const surface = useRef<HTMLDivElement>(null);
  const start = useRef<{ x: number; y: number } | null>(null);
  const load = async () => {
    setLoading(true);
    const result = await call<{ path: string; width: number; height: number }>(
      "preview_frame",
      { id: task.id, second, watermark: null, clip: false },
    );
    if (result) setFrame(result);
    setLoading(false);
  };
  useEffect(() => {
    void load();
  }, []);
  const point = (e: PointerEvent) => {
    const r = surface.current!.getBoundingClientRect();
    return {
      x: Math.max(
        1,
        Math.min(
          frame!.width - 1,
          Math.round(((e.clientX - r.left) / r.width) * frame!.width),
        ),
      ),
      y: Math.max(
        1,
        Math.min(
          frame!.height - 1,
          Math.round(((e.clientY - r.top) / r.height) * frame!.height),
        ),
      ),
    };
  };
  return (
    <Modal title="画面处理" onClose={onClose} wide>
      <div className="watermark-layout">
        <div>
          <div
            className="frame"
            ref={surface}
            style={
              frame
                ? { aspectRatio: `${frame.width}/${frame.height}` }
                : { aspectRatio: "16/9" }
            }
            onPointerDown={(e) => {
              if (!frame) return;
              start.current = point(e);
              e.currentTarget.setPointerCapture(e.pointerId);
            }}
            onPointerMove={(e) => {
              if (!start.current || !frame) return;
              const p = point(e),
                s = start.current;
              setW((v) => ({
                ...v,
                x: Math.min(p.x, s.x),
                y: Math.min(p.y, s.y),
                width: Math.max(1, Math.abs(p.x - s.x)),
                height: Math.max(1, Math.abs(p.y - s.y)),
              }));
            }}
            onPointerUp={() => (start.current = null)}
            onPointerCancel={() => (start.current = null)}
          >
            {frame ? (
              <>
                <img
                  draggable={false}
                  src={convertFileSrc(frame.path)}
                  alt="拖动框选水印区域"
                />
                <div
                  className="selection"
                  style={{
                    left: `${(w.x / frame.width) * 100}%`,
                    top: `${(w.y / frame.height) * 100}%`,
                    width: `${(w.width / frame.width) * 100}%`,
                    height: `${(w.height / frame.height) * 100}%`,
                  }}
                />
              </>
            ) : (
              <Loader2 className="spin" />
            )}
          </div>
          <div className="actions">
            <input
              type="number"
              aria-label="预览时间秒"
              min={0}
              value={second}
              onChange={(e) => setSecond(Number(e.target.value))}
            />
            <Button onClick={() => void load()} disabled={loading}>
              加载画面
            </Button>
          </div>
          {preview && (
            <video controls src={convertFileSrc(preview)} className="clip" />
          )}
        </div>
        <div className="watermark-controls">
          <label>
            处理方式
            <select
              value={w.mode}
              onChange={(e) => setW({ ...w, mode: e.target.value })}
            >
              <option value="delogo">插值修补</option>
              <option value="blur">模糊遮盖</option>
              <option value="crop">裁剪保留选区</option>
            </select>
          </label>
          <div className="form-grid compact">
            {(["x", "y", "width", "height", "start", "end"] as const).map(
              (k, i) => (
                <label key={k}>
                  {["X", "Y", "宽度", "高度", "开始秒", "结束秒 · 0 到结尾"][i]}
                  <input
                    type="number"
                    min={0}
                    value={w[k]}
                    onChange={(e) =>
                      setW({ ...w, [k]: Number(e.target.value) })
                    }
                  />
                </label>
              ),
            )}
          </div>
          <label className="check">
            <input
              type="checkbox"
              checked={w.keepOriginal}
              onChange={(e) => setW({ ...w, keepOriginal: e.target.checked })}
            />
            保留原始文件
          </label>
          <p className="muted small">
            拖动框选区域。修补可能有痕迹，请先预览。
          </p>
          <Button
            disabled={!frame || loading}
            onClick={async () => {
              setLoading(true);
              const p = await call<{ path: string }>("preview_frame", {
                id: task.id,
                second,
                watermark: w,
                clip: true,
              });
              if (p) setPreview(p.path);
              setLoading(false);
            }}
          >
            {loading ? (
              <Loader2 size={16} className="spin" />
            ) : (
              <Play size={16} />
            )}
            预览 5 秒
          </Button>
          <Button
            primary
            disabled={!frame || loading}
            onClick={() => onApply(w)}
          >
            应用并另存
          </Button>
        </div>
      </div>
    </Modal>
  );
}
