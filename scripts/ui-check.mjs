import { chromium } from "@playwright/test";
import { mkdir, readFile } from "node:fs/promises";
import { createServer } from "vite";
const server = await createServer({
  server: { host: "127.0.0.1", port: 1430, strictPort: true },
});
await server.listen();
const browser = await chromium.launch({ channel: "msedge", headless: true });
try {
  const page = await browser.newPage({
    viewport: { width: 1180, height: 790 },
    deviceScaleFactor: 1,
  });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.addInitScript(() => {
    window.isTauri = true;
    const settings = {
      directory: "D:\\Downloads\\Bilibili",
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
    const options = {
      ...settings,
      resource: "media",
      compatibility: false,
      watermark: {
        mode: "none",
        x: 1,
        y: 1,
        width: 120,
        height: 40,
        start: 0,
        end: 0,
        keepOriginal: true,
      },
    };
    const names = [
      "城市夜景拍摄指南 · P01 基础设置",
      "雨天的钢琴练习",
      "剪辑入门 · 色彩与导出",
      "周末散步影像记录",
      "用一束光记录日常",
    ];
    const statuses = [
      "downloading",
      "converting",
      "paused",
      "error",
      "completed",
      "queued",
    ];
    const tasks = Array.from({ length: 500 }, (_, i) => ({
      id: `test-${i}`,
      item: {
        id: `BV${i}`,
        url: `https://www.bilibili.com/video/BV${i}`,
        title: names[i % 5] + ` · ${i + 1}`,
        uploader: "光影笔记",
        thumbnail: "",
        duration: 120,
        index: i + 1,
      },
      options,
      status: statuses[i % 6],
      stage: "处理中 · 42%",
      downloaded: 680000000,
      total: 1000000000,
      speed: i === 0 ? 5200000 : 0,
      error: i % 6 === 3 ? "登录已过期，请重新登录" : "",
      warnings: [],
      output: `D:\\Downloads\\Bilibili\\${i}.mp4`,
      created: 0,
      updated: 0,
    }));
    const state = {
      tasks,
      settings,
      parse: { running: false, items: [], errors: [], processed: 0, total: 0 },
      connections: 12,
      version: "0.1.0",
    };
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: "main" } },
      invoke: async (cmd, args) => {
        (window.__windowCalls ??= []).push(cmd);
        (window.__recordedCalls ??= []).push({ cmd, args });
        if (cmd === "snapshot") return structuredClone(state);
        if (cmd === "account")
          return { loggedIn: false, name: "", uid: 0, vip: false };
        if (cmd === "task_action") {
          for (const t of tasks.filter((t) =>
            args.ids ? args.ids.includes(t.id) : t.id === args.id,
          ))
            t.status = args.action === "pause" ? "paused" : "queued";
          return null;
        }
        if (cmd === "save_settings") {
          state.settings = args.settings;
          return null;
        }
        if (cmd === "parse_inputs") {
          state.parse.items = tasks.slice(0, 10).map((t) => t.item);
          return null;
        }
        if (cmd === "clear_parse") {
          state.parse.items = [];
          return null;
        }
        if (cmd === "enqueue") return args.items.length;
        return null;
      },
    };
  });
  await page.goto("http://127.0.0.1:1430/");
  await page.getByRole("heading", { name: "下载队列" }).waitFor();
  await page.getByText("500 个任务").waitFor();
  const nativeConfig = JSON.parse(
    await readFile("src-tauri/tauri.conf.json", "utf8"),
  );
  if (nativeConfig.app.windows[0].decorations !== false)
    throw Error("Native titlebar is still enabled");
  for (const name of ["最小化", "最大化或还原", "关闭窗口"])
    await page.getByRole("button", { name, exact: true }).click();
  const calls = await page.evaluate(() => window.__windowCalls);
  for (const command of [
    "plugin:window|minimize",
    "plugin:window|toggle_maximize",
    "plugin:window|close",
  ])
    if (!calls.includes(command))
      throw Error(`Window control did not invoke ${command}`);
  if ((await page.locator(".task-row").count()) > 20)
    throw Error("Queue is not virtualized");
  await mkdir("test-results", { recursive: true });
  await page.screenshot({ path: "test-results/queue.png", fullPage: true });
  await page
    .locator(".task-row")
    .first()
    .getByRole("button", { name: "暂停任务" })
    .click();
  await page.locator(".task-row").first().getByText("暂停中").waitFor();
  await page.getByRole("button", { name: /^暂停 \d/ }).click();
  await page.getByRole("button", { name: "继续下载", exact: true }).click();
  const batch = await page.evaluate(() =>
    window.__recordedCalls.filter(
      (c) => c.cmd === "task_action" && Array.isArray(c.args.ids),
    ),
  );
  if (batch.length !== 1 || batch[0].args.ids.length !== 84)
    throw Error("Batch action did not use one request for paused tasks");
  await page.getByRole("button", { name: /^全部 \d/ }).click();
  await page.locator(".virtual-list").evaluate((e) => (e.scrollTop = 50000));
  await page.waitForTimeout(150);
  if ((await page.locator(".task-row").count()) > 20)
    throw Error("Virtual scroll rendered too many rows");
  await page.getByRole("button", { name: "账号", exact: true }).click();
  await page.getByRole("heading", { name: "账号", exact: true }).waitFor();
  if (await page.getByRole("heading", { name: "并发与恢复" }).count())
    throw Error("Account still mixed with settings");
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await page.getByRole("heading", { name: "并发与恢复" }).waitFor();
  await page.screenshot({ path: "test-results/settings.png", fullPage: true });
  const bounds = await page
    .getByRole("button", { name: "设置", exact: true })
    .boundingBox();
  if (bounds.y < 650) throw Error("Settings is not bottom aligned");
  const shadow = await page
    .locator(".nav-item.active")
    .evaluate((e) => getComputedStyle(e).boxShadow);
  if (shadow !== "none") throw Error("Pink rail remains");
  await page.getByRole("button", { name: "新建下载", exact: true }).click();
  if (await page.locator(".parse-row, .task-row, .options-toggle").count())
    throw Error("New download page contains results or options before parsing");
  await page.screenshot({ path: "test-results/add.png", fullPage: true });
  await page
    .getByRole("textbox", { name: "视频链接" })
    .fill("https://www.bilibili.com/video/BV1SUnB61E8K/");
  await page.getByRole("button", { name: "账号", exact: true }).click();
  await page.getByRole("button", { name: "新建下载", exact: true }).click();
  if (
    (await page.getByRole("textbox", { name: "视频链接" }).inputValue()) !==
    "https://www.bilibili.com/video/BV1SUnB61E8K/"
  )
    throw Error("Link draft was lost");
  await page.getByRole("button", { name: "解析链接" }).click();
  await page.getByText("已选 10 / 10").waitFor();
  await page.getByRole("dialog").waitFor();
  const checkboxes = page.getByRole("dialog").getByRole("checkbox");
  await checkboxes.first().uncheck();
  if (await page.getByRole("button", { name: "开始下载 · 0" }).isEnabled())
    throw Error("Empty selection can start a download");
  await checkboxes.first().check();
  await page.screenshot({ path: "test-results/parse.png", fullPage: true });
  await page.getByRole("button", { name: "开始下载 · 10" }).click();
  await page.getByRole("heading", { name: "下载队列" }).waitFor();
  if (errors.length) throw Error(errors.join("\n"));
  console.log(
    "PASS: frameless window controls, 500-task virtualization, pause, navigation, settings placement, no rail, parse and enqueue, zero runtime errors.",
  );
} finally {
  await browser.close();
  await server.close();
}
