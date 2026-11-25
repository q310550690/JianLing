/** 版本更新检测：读取 GitHub 最新 Release 并与当前版本比较。 */
import { ElNotification } from "element-plus";
import { openUrl } from "@tauri-apps/plugin-opener";
import { APP_INFO } from "./app-info";

const REPO = "q310550690/JianLing";
const CHECK_KEY = "jianling.update.checkedAt";
/** 同一个客户端在此时长内不重复请求，避免频繁触发接口限流 */
const CHECK_INTERVAL = 6 * 60 * 60 * 1000;

export interface UpdateInfo {
  version: string;
  url: string;
  notes: string;
}

function parseVersion(raw: string): number[] {
  return raw
    .trim()
    .replace(/^v/i, "")
    .split(/[.\-+]/)
    .map(part => Number.parseInt(part, 10))
    .map(value => (Number.isFinite(value) ? value : 0));
}

/** 判断 a 是否比 b 新（按数字段比较） */
export function isNewerVersion(a: string, b: string): boolean {
  const left = parseVersion(a);
  const right = parseVersion(b);
  const length = Math.max(left.length, right.length);
  for (let i = 0; i < length; i += 1) {
    const x = left[i] ?? 0;
    const y = right[i] ?? 0;
    if (x !== y)
      return x > y;
  }
  return false;
}

/**
 * 查询最新版本。
 * @param currentVersion 当前版本号，如 0.1.0
 * @param force 跳过冷却期，并在请求失败时抛出错误（手动检查用）
 * @returns 有更新时返回版本信息；无更新返回 null
 */
export async function checkUpdate(
  currentVersion: string,
  force = false,
): Promise<UpdateInfo | null> {
  if (!force) {
    const last = Number(window.localStorage.getItem(CHECK_KEY) ?? 0);
    if (Number.isFinite(last) && Date.now() - last < CHECK_INTERVAL)
      return null;
  }
  window.localStorage.setItem(CHECK_KEY, String(Date.now()));
  let response: Response;
  try {
    response = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
      headers: { Accept: "application/vnd.github+json" },
    });
  }
  catch (error) {
    if (force)
      throw new Error(error instanceof Error ? error.message : String(error));
    return null;
  }
  if (!response.ok) {
    if (force)
      throw new Error(response.status === 404 ? "暂无可用版本" : `接口返回 ${response.status}`);
    return null;
  }
  const data = (await response.json()) as Record<string, unknown>;
  const version = String(data.tag_name ?? "").replace(/^v/i, "").trim();
  const url = String(data.html_url ?? "").trim();
  if (!version || !url || !isNewerVersion(version, currentVersion))
    return null;
  return { version, url, notes: String(data.body ?? "") };
}

/** 以常驻通知提示新版本，点击通知打开下载页面 */
export function notifyUpdate(info: UpdateInfo): void {
  ElNotification({
    title: `发现新版本 v${info.version}`,
    message: `当前版本 v${APP_INFO.version}，点击打开下载页面`,
    duration: 0,
    onClick: () => {
      void openUrl(info.url);
    },
  });
}
