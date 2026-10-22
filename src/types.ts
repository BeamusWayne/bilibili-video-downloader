export interface Watermark {
  mode: string;
  x: number;
  y: number;
  width: number;
  height: number;
  start: number;
  end: number;
  keepOriginal: boolean;
}
export interface Options {
  resource: "media" | "extras";
  format: string;
  quality: number;
  bitrate: number;
  compatibility: boolean;
  cover: boolean;
  subtitles: boolean;
  danmaku: boolean;
  directory: string;
  template: string;
  watermark: Watermark;
}
export interface Settings {
  directory: string;
  tasks: number;
  connections: number;
  globalConnections: number;
  conversions: number;
  retries: number;
  resumeOnStart: boolean;
  tray: boolean;
  rateLimitKib: number;
  format: string;
  quality: number;
  bitrate: number;
  cover: boolean;
  subtitles: boolean;
  danmaku: boolean;
  template: string;
}
export interface MediaItem {
  id: string;
  url: string;
  title: string;
  albumTitle?: string;
  uploader: string;
  thumbnail: string;
  duration: number;
  index: number;
}
export interface Task {
  id: string;
  item: MediaItem;
  options: Options;
  status: string;
  stage: string;
  downloaded: number;
  total: number;
  speed: number;
  error: string;
  warnings: string[];
  output: string;
  created: number;
  updated: number;
}
export interface ParseState {
  running: boolean;
  items: MediaItem[];
  errors: string[];
  processed: number;
  total: number;
}
export interface Snapshot {
  tasks: Task[];
  settings: Settings;
  parse: ParseState;
  connections: number;
  version: string;
}
export interface Account {
  loggedIn: boolean;
  name: string;
  uid: number;
  vip: boolean;
  avatar?: string;
  cached?: boolean;
}
export const defaults: Settings = {
  directory: "",
  tasks: 3,
  connections: 4,
  globalConnections: 12,
  conversions: 1,
  retries: 5,
  resumeOnStart: true,
  tray: true,
  rateLimitKib: 0,
  format: "mp4",
  quality: 0,
  bitrate: 320,
  cover: true,
  subtitles: true,
  danmaku: false,
  template: "{UP主}/{标题}/{P序号}-{分P标题}",
};
export const watermark: Watermark = {
  mode: "none",
  x: 1,
  y: 1,
  width: 120,
  height: 40,
  start: 0,
  end: 0,
  keepOriginal: true,
};
export function optionsFrom(s: Settings): Options {
  return {
    resource: "media",
    format: s.format,
    quality: s.quality,
    bitrate: s.bitrate,
    compatibility: false,
    cover: s.cover,
    subtitles: s.subtitles,
    danmaku: s.danmaku,
    directory: s.directory,
    template: s.template,
    watermark: { ...watermark },
  };
}
export const statusName: Record<string, string> = {
  queued: "排队中",
  resolving: "解析中",
  downloading: "下载中",
  converting: "处理中",
  retrying: "重试中",
  paused: "暂停中",
  completed: "已完成",
  error: "出错",
  cancelled: "已取消",
};
export function bytes(n: number) {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n / 1024,
    i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 100 ? 0 : 1)} ${units[i]}`;
}
export function duration(n: number) {
  return `${Math.floor(n / 60)}:${Math.floor(n % 60)
    .toString()
    .padStart(2, "0")}`;
}
