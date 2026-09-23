// 本版本由作者自行维护，不再指向原作者仓库。
export const GITHUB_OWNER = "";
export const GITHUB_REPO = "";
export const GITHUB_REPOSITORY_URL = "";
export const GITHUB_RELEASE_URL = "";

/** 在桌面端通过 Tauri opener 打开，在 webui 端打开新标签页。 */
export async function openReleaseUrl(url = GITHUB_RELEASE_URL): Promise<void> {
  if (!url) return;
  const isWebui = typeof window !== "undefined" && !("__TAURI_INTERNALS__" in window);
  if (isWebui) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}
