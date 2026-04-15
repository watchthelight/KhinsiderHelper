import { searchAlbums, fetchAlbumDetail, type SearchResult, type AlbumDetail } from "../lib/tauri.js";
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
      <h1>search</h1>
      <div class="sub">browse the khinsider catalog</div>
    </div>

    <div class="search-input-row">
      <input type="text" class="input" id="search-input" placeholder="search for an album..." />
      <button class="btn primary" id="btn-search">search</button>
    </div>

    <div id="search-results" class="album-list" style="display:none"></div>
    <div id="album-detail-view" style="display:none"></div>
  `;

  const input = el.querySelector<HTMLInputElement>("#search-input")!;
  const btn = el.querySelector<HTMLButtonElement>("#btn-search")!;
  const resultsEl = el.querySelector<HTMLElement>("#search-results")!;
  const detailEl = el.querySelector<HTMLElement>("#album-detail-view")!;

  let currentDetail: AlbumDetail | null = null;

  async function doSearch(): Promise<void> {
    const query = input.value.trim();
    if (!query) return;

    btn.classList.add("loading");
    btn.disabled = true;
    detailEl.style.display = "none";
    resultsEl.style.display = "";
    resultsEl.innerHTML = `<div style="padding:10px;color:var(--dim)">searching...</div>`;

    try {
      const results = await searchAlbums(query);
      if (results.length === 0) {
        resultsEl.innerHTML = `<div style="padding:10px;color:var(--dim)">no results found</div>`;
        return;
      }

      resultsEl.innerHTML = results.map(r => `
        <div class="album-row" data-slug="${escHtml(r.slug)}">
          <span class="artist">${escHtml(r.name)}</span>
          <span class="title">${escHtml(r.platform)}</span>
          <span class="date">${escHtml(r.year)}</span>
        </div>
      `).join("");

      for (const row of resultsEl.querySelectorAll<HTMLElement>(".album-row")) {
        row.addEventListener("click", () => showAlbumDetail(row.dataset.slug!));
      }
    } catch (e: any) {
      resultsEl.innerHTML = `<div style="padding:10px;color:var(--red)">${escHtml(String(e))}</div>`;
    } finally {
      btn.classList.remove("loading");
      btn.disabled = false;
    }
  }

  async function showAlbumDetail(slug: string): Promise<void> {
    resultsEl.style.display = "none";
    detailEl.style.display = "flex";
    detailEl.style.flexDirection = "column";
    detailEl.style.flex = "1";
    detailEl.style.minHeight = "0";
    detailEl.innerHTML = `<div style="padding:10px;color:var(--dim)">loading album...</div>`;

    try {
      const album = await fetchAlbumDetail(slug);
      currentDetail = album;

      const artUrl = album.images.length > 0 ? escHtml(album.images[0]) : "";
      const formatOptions = album.formats.map(f => `<option value="${escHtml(f)}">${escHtml(f)}</option>`).join("");

      detailEl.innerHTML = `
        <div style="margin-bottom:8px">
          <button class="btn" id="btn-back-search">&lt; back to results</button>
        </div>

        <div class="album-detail">
          <div class="album-detail-art" style="${artUrl ? `background-image:url(${artUrl})` : ""}"></div>
          <div class="album-detail-meta">
            <h2>${escHtml(album.name)}</h2>
            <div class="meta-row">platform: ${escHtml(album.platforms.join(", ") || "—")}</div>
            <div class="meta-row">year: ${escHtml(album.year || "—")}</div>
            <div class="meta-row">publisher: ${escHtml(album.publisher || "—")}</div>
            <div class="meta-row">developer: ${escHtml(album.developer || "—")}</div>
            <div class="meta-row">tracks: ${album.songs.length} &middot; ${escHtml(album.total_size_mp3 || "—")} MP3 &middot; ${escHtml(album.total_size_flac || "—")} FLAC</div>
          </div>
        </div>

        <div class="toolbar">
          <select class="select" id="format-select">
            <option value="BEST">BEST (highest quality)</option>
            ${formatOptions}
          </select>
          <div class="spacer"></div>
          <button class="btn primary" id="btn-dl-album">download album</button>
        </div>

        <div class="album-list" id="track-list" style="flex:1;min-height:0">
          <table class="track-table">
            <thead>
              <tr>
                <th>#</th>
                <th>name</th>
                <th>duration</th>
                ${album.formats.map(f => `<th class="track-size">${escHtml(f)}</th>`).join("")}
              </tr>
            </thead>
            <tbody>
              ${album.songs.map(s => `
                <tr>
                  <td class="track-num">${escHtml(s.track_number)}</td>
                  <td>${escHtml(s.name)}</td>
                  <td>${escHtml(s.duration)}</td>
                  ${album.formats.map(f => `<td class="track-size">${escHtml(s.sizes[f] || "—")}</td>`).join("")}
                </tr>
              `).join("")}
            </tbody>
          </table>
        </div>
      `;

      detailEl.querySelector("#btn-back-search")!.addEventListener("click", () => {
        detailEl.style.display = "none";
        resultsEl.style.display = "";
        currentDetail = null;
      });

      detailEl.querySelector("#btn-dl-album")!.addEventListener("click", () => {
        if (!currentDetail) return;
        const format = (detailEl.querySelector("#format-select") as HTMLSelectElement).value;
        (window as any).__DOWNLOAD_QUEUE__ = [currentDetail];
        (window as any).__DOWNLOAD_FORMAT__ = format;
        switchPage("downloads");
      });
    } catch (e: any) {
      detailEl.innerHTML = `
        <div style="margin-bottom:8px">
          <button class="btn" id="btn-back-search">&lt; back to results</button>
        </div>
        <div style="padding:10px;color:var(--red)">${escHtml(String(e))}</div>
      `;
      detailEl.querySelector("#btn-back-search")!.addEventListener("click", () => {
        detailEl.style.display = "none";
        resultsEl.style.display = "";
      });
    }
  }

  btn.addEventListener("click", doSearch);
  input.addEventListener("keydown", (e: KeyboardEvent) => {
    if (e.key === "Enter") doSearch();
  });
}
