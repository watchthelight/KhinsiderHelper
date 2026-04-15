import { fetchLibrary, startLibraryDownload, checkLocalAlbums, type LibraryAlbum, type LibraryDownloadRequest, type LocalAlbumStatus } from "../lib/tauri.js";
import { onTrackProgress, onTrackStatus, onAlbumProgress } from "../lib/events.js";
import { switchPage } from "../main.js";

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
      <h1>your library</h1>
      <div class="sub" id="lib-count">loading...</div>
    </div>

    <div class="toolbar">
      <button class="btn" id="btn-refresh-lib">refresh</button>
      <button class="btn" id="btn-select-all">select all</button>
      <div class="spacer"></div>
      <button class="btn primary" id="btn-dl-selected" disabled>download selected</button>
    </div>

    <div class="album-list" id="lib-list"></div>

    <div class="log-section" style="max-height:120px;margin-top:8px;display:none" id="lib-log-section">
      <div class="log-header">log</div>
      <div class="log" id="lib-log" style="min-height:60px"></div>
    </div>
  `;

  const countEl = el.querySelector<HTMLElement>("#lib-count")!;
  const list = el.querySelector<HTMLElement>("#lib-list")!;
  const btnRefresh = el.querySelector<HTMLButtonElement>("#btn-refresh-lib")!;
  const btnSelectAll = el.querySelector<HTMLButtonElement>("#btn-select-all")!;
  const btnDl = el.querySelector<HTMLButtonElement>("#btn-dl-selected")!;
  const logSection = el.querySelector<HTMLElement>("#lib-log-section")!;
  const log = el.querySelector<HTMLElement>("#lib-log")!;

  let albums: LibraryAlbum[] = [];
  const selected = new Set<string>();
  const localStatus = new Map<string, LocalAlbumStatus>();
  let downloading = false;

  function addLog(msg: string, cls: string = ""): void {
    logSection.style.display = "";
    const line = document.createElement("div");
    line.className = `log-line ${cls}`;
    line.innerHTML = `<span class="prefix">&gt;</span>${escHtml(msg)}`;
    log.appendChild(line);
    log.scrollTop = log.scrollHeight;
  }

  function updateSelectAllLabel(): void {
    const allSelected = albums.length > 0 && selected.size === albums.length;
    btnSelectAll.textContent = allSelected ? "deselect all" : "select all";
  }

  const dlStatus = new Map<string, { status: string; bytes: number; total: number }>();

  function render(): void {
    const localCount = albums.filter(a => localStatus.get(a.name)?.exists).length;
    countEl.textContent = `${albums.length} albums` +
      (localCount > 0 ? ` (${localCount} local)` : "") +
      (selected.size > 0 ? ` (${selected.size} selected)` : "");

    if (downloading) {
      list.innerHTML = Array.from(dlStatus.entries()).map(([name, info]) => {
        const sizeText = info.status === "Downloading" && info.total > 0
          ? `${formatBytes(info.bytes)} / ${formatBytes(info.total)}`
          : info.status;
        const cls = info.status === "Done" ? "done" : info.status === "Error" ? "error" : info.status === "Downloading" || info.status === "Resolving" || info.status === "Extracting" ? "active" : info.status === "Skipped" ? "done" : "queued";
        return `
          <div class="download-row">
            <div class="status-dot ${info.status === "Done" || info.status === "Skipped" ? "green" : info.status === "Error" ? "red" : info.status === "Downloading" || info.status === "Resolving" || info.status === "Extracting" ? "yellow" : ""}"></div>
            <span class="dl-title">${escHtml(name)}</span>
            <span class="dl-status ${cls}">${escHtml(sizeText)}</span>
          </div>
        `;
      }).join("");
      return;
    }

    list.innerHTML = albums.map(a => {
      const ls = localStatus.get(a.name);
      const isLocal = ls?.exists;
      const trackInfo = isLocal && ls
        ? (ls.local_tracks >= ls.total_tracks && ls.total_tracks > 0
          ? `${ls.local_tracks} tracks`
          : `${ls.local_tracks}/${ls.total_tracks} tracks`)
        : "";
      const incomplete = isLocal && ls && ls.total_tracks > 0 && ls.local_tracks < ls.total_tracks;
      return `
        <div class="album-row ${selected.has(a.slug) ? "selected" : ""}" data-slug="${escHtml(a.slug)}">
          <div class="cb"></div>
          <span class="title" style="color:var(--text)">${escHtml(a.name)}</span>
          ${isLocal ? `<span class="date" style="font-size:11px;color:${incomplete ? "var(--yellow)" : "var(--green)"}">${escHtml(trackInfo)}</span>` : ""}
          ${a.download_url ? `<span class="date" style="font-size:11px;color:var(--faint)">zip</span>` : ""}
        </div>
      `;
    }).join("");

    for (const row of list.querySelectorAll<HTMLElement>(".album-row")) {
      row.addEventListener("click", () => {
        const slug = row.dataset.slug!;
        if (selected.has(slug)) selected.delete(slug); else selected.add(slug);
        btnDl.disabled = selected.size === 0;
        updateSelectAllLabel();
        render();
      });
    }
  }

  btnSelectAll.addEventListener("click", () => {
    if (albums.length > 0 && selected.size === albums.length) {
      selected.clear();
    } else {
      for (const a of albums) selected.add(a.slug);
    }
    btnDl.disabled = selected.size === 0;
    updateSelectAllLabel();
    render();
  });

  await onTrackStatus((p) => {
    dlStatus.set(p.track_name, { status: p.status, bytes: p.bytes_downloaded, total: p.bytes_total });
    if (p.status === "Error" && p.error) {
      addLog(`${p.track_name}: ${p.error}`, "error");
    }
    if (downloading) render();
  });

  await onTrackProgress((p) => {
    const entry = dlStatus.get(p.track_name);
    if (entry) {
      entry.bytes = p.bytes_downloaded;
      entry.total = p.bytes_total;
    }
    if (downloading) render();
  });

  await onAlbumProgress((p) => {
    if (p.status === "Done" && downloading) {
      downloading = false;
      btnDl.disabled = false;
      btnDl.classList.remove("loading");
      btnDl.textContent = "download selected";
      addLog(`finished: ${p.completed} done, ${p.failed} failed, ${p.skipped} skipped`, p.failed > 0 ? "warn" : "success");
      loadLibrary();
    }
  });

  btnDl.addEventListener("click", async () => {
    const toDownload = albums.filter(a => selected.has(a.slug));
    if (toDownload.length === 0) return;

    const outputDir = localStorage.getItem("kh_output_dir") || "";

    downloading = true;
    btnDl.classList.add("loading");
    btnDl.disabled = true;
    dlStatus.clear();

    for (const a of toDownload) {
      dlStatus.set(a.name, { status: "Queued", bytes: 0, total: 0 });
    }
    render();

    addLog(`downloading ${toDownload.length} albums`, "info");

    const request: LibraryDownloadRequest = {
      albums: toDownload,
      output_dir: outputDir,
      extract: true,
    };

    try {
      await startLibraryDownload(request);
    } catch (e: any) {
      addLog(`error: ${e}`, "error");
      downloading = false;
      btnDl.classList.remove("loading");
      btnDl.disabled = false;
    }
  });

  async function loadLibrary(): Promise<void> {
    btnRefresh.classList.add("loading");
    btnRefresh.disabled = true;
    countEl.textContent = "loading...";
    list.innerHTML = "";

    try {
      albums = await fetchLibrary();

      // Check local status
      const outputDir = localStorage.getItem("kh_output_dir") || "";
      try {
        const names = albums.map(a => a.name);
        const counts = albums.map(() => 0);
        const statuses = await checkLocalAlbums(names, counts, outputDir);
        for (const s of statuses) {
          localStatus.set(s.name, s);
        }
      } catch {}

      render();
    } catch (e: any) {
      countEl.textContent = `error: ${e}`;
    } finally {
      btnRefresh.classList.remove("loading");
      btnRefresh.disabled = false;
    }
  }

  btnRefresh.addEventListener("click", loadLibrary);

  await loadLibrary();
}
