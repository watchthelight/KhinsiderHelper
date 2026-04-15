import { startAlbumDownload, cancelDownloads, type AlbumDetail, type DownloadRequest } from "../lib/tauri.js";
import { onTrackProgress, onTrackStatus, onAlbumProgress } from "../lib/events.js";

function escHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function formatBytes(b: number): string {
  if (b < 1024) return `${b} B`;
  if (b < 1048576) return `${(b / 1024).toFixed(1)} KB`;
  if (b < 1073741824) return `${(b / 1048576).toFixed(1)} MB`;
  return `${(b / 1073741824).toFixed(1)} GB`;
}

export async function init(el: HTMLElement): Promise<void> {
  el.innerHTML = `
    <div class="header">
      <h1 id="dl-header">downloads</h1>
      <div class="sub" id="dl-sub">waiting for downloads...</div>
    </div>

    <div class="progress-bar" id="dl-progress">
      <div class="fill" style="width:0%"></div>
    </div>

    <div class="toolbar">
      <span id="dl-stats" style="font-size:12px;color:var(--dim)"></span>
      <div class="spacer"></div>
      <button class="btn danger" id="btn-cancel" disabled>cancel</button>
    </div>

    <div class="download-list" id="dl-list"></div>

    <div class="log-section" style="max-height:120px;margin-top:8px">
      <div class="log-header">log</div>
      <div class="log" id="dl-log" style="min-height:60px"></div>
    </div>
  `;

  const header = el.querySelector<HTMLElement>("#dl-header")!;
  const sub = el.querySelector<HTMLElement>("#dl-sub")!;
  const progressFill = el.querySelector<HTMLElement>("#dl-progress .fill")!;
  const stats = el.querySelector<HTMLElement>("#dl-stats")!;
  const btnCancel = el.querySelector<HTMLButtonElement>("#btn-cancel")!;
  const list = el.querySelector<HTMLElement>("#dl-list")!;
  const log = el.querySelector<HTMLElement>("#dl-log")!;

  const trackStatus = new Map<string, { status: string; bytes: number; total: number }>();

  function addLog(msg: string, cls: string = ""): void {
    const line = document.createElement("div");
    line.className = `log-line ${cls}`;
    line.innerHTML = `<span class="prefix">&gt;</span>${escHtml(msg)}`;
    log.appendChild(line);
    log.scrollTop = log.scrollHeight;
  }

  function statusClass(status: string): string {
    switch (status) {
      case "Done": return "done";
      case "Downloading": case "Resolving": return "active";
      case "Error": return "error";
      case "Skipped": return "done";
      default: return "queued";
    }
  }

  function renderList(): void {
    const entries = Array.from(trackStatus.entries());
    list.innerHTML = entries.map(([name, info]) => {
      const pct = info.total > 0 ? Math.round((info.bytes / info.total) * 100) : 0;
      const sizeText = info.status === "Downloading" && info.total > 0
        ? `${formatBytes(info.bytes)} / ${formatBytes(info.total)}`
        : info.status;
      return `
        <div class="download-row">
          <div class="status-dot ${info.status === "Done" || info.status === "Skipped" ? "green" : info.status === "Error" ? "red" : info.status === "Downloading" || info.status === "Resolving" ? "yellow" : ""}"></div>
          <span class="dl-title">${escHtml(name)}</span>
          <span class="dl-status ${statusClass(info.status)}">${escHtml(sizeText)}</span>
        </div>
      `;
    }).join("");
  }

  await onTrackStatus((p) => {
    trackStatus.set(p.track_name, { status: p.status, bytes: p.bytes_downloaded, total: p.bytes_total });
    if (p.status === "Error" && p.error) {
      addLog(`${p.track_name}: ${p.error}`, "error");
    }
    renderList();
  });

  await onTrackProgress((p) => {
    const entry = trackStatus.get(p.track_name);
    if (entry) {
      entry.bytes = p.bytes_downloaded;
      entry.total = p.bytes_total;
    }
    renderList();
  });

  await onAlbumProgress((p) => {
    const total = p.total_tracks;
    const done = p.completed + p.failed + p.skipped;
    const pct = total > 0 ? Math.round((done / total) * 100) : 0;
    progressFill.style.width = `${pct}%`;
    stats.textContent = `${p.completed} done, ${p.failed} failed, ${p.skipped} skipped / ${total} total`;

    if (p.status === "Done" || (done >= total && total > 0)) {
      header.textContent = "downloads complete";
      sub.textContent = `${p.album_name}`;
      btnCancel.disabled = true;
      addLog(`finished: ${p.completed} done, ${p.failed} failed, ${p.skipped} skipped`, p.failed > 0 ? "warn" : "success");
    }
  });

  btnCancel.addEventListener("click", async () => {
    await cancelDownloads();
    addLog("downloads cancelled", "warn");
    btnCancel.disabled = true;
  });

  // Check for queued downloads
  async function processQueue(): Promise<void> {
    const queue = (window as any).__DOWNLOAD_QUEUE__ as AlbumDetail[] | undefined;
    if (!queue || queue.length === 0) return;

    const format = (window as any).__DOWNLOAD_FORMAT__ || "BEST";
    delete (window as any).__DOWNLOAD_QUEUE__;
    delete (window as any).__DOWNLOAD_FORMAT__;

    const settings = {
      output_dir: localStorage.getItem("kh_output_dir") || "",
      parallel: parseInt(localStorage.getItem("kh_parallel") || "3", 10),
      skip_existing: localStorage.getItem("kh_skip_existing") !== "false",
    };

    for (const album of queue) {
      header.textContent = `downloading: ${album.name}`;
      sub.textContent = `${album.songs.length} tracks`;
      btnCancel.disabled = false;
      trackStatus.clear();

      for (const s of album.songs) {
        trackStatus.set(s.name, { status: "Queued", bytes: 0, total: 0 });
      }
      renderList();

      addLog(`starting: ${album.name} (${format})`, "info");

      const request: DownloadRequest = {
        album,
        format,
        output_dir: settings.output_dir,
        parallel: settings.parallel,
        skip_existing: settings.skip_existing,
      };

      try {
        await startAlbumDownload(request);
      } catch (e: any) {
        addLog(`error: ${e}`, "error");
      }
    }
  }

  processQueue();
}
