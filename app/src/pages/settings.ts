import { getDefaultOutputDirectory } from "../lib/tauri.js";

export async function init(el: HTMLElement): Promise<void> {
  let defaultDir = "";
  try {
    defaultDir = await getDefaultOutputDirectory();
  } catch {
    defaultDir = "";
  }

  const savedDir = localStorage.getItem("kh_output_dir") || defaultDir;
  const savedParallel = localStorage.getItem("kh_parallel") || "3";
  const savedQuality = localStorage.getItem("kh_quality") || "BEST";
  const savedSkip = localStorage.getItem("kh_skip_existing") !== "false";

  // Persist default on first run
  if (!localStorage.getItem("kh_output_dir") && defaultDir) {
    localStorage.setItem("kh_output_dir", defaultDir);
  }

  el.innerHTML = `
    <div class="header">
      <h1>settings</h1>
      <div class="sub">configure download behavior</div>
    </div>

    <div class="status-section">
      <div class="status-header">output</div>
      <div style="display:flex;gap:8px;align-items:center;margin-bottom:12px">
        <input type="text" class="input" id="set-output-dir"
               value="${savedDir.replace(/"/g, "&quot;")}"
               placeholder="download directory" style="flex:1" />
      </div>
    </div>

    <div class="status-section">
      <div class="status-header">downloads</div>
      <div style="display:flex;gap:16px;align-items:center;margin-bottom:12px;flex-wrap:wrap">
        <label style="font-size:13px;color:var(--dim);display:flex;align-items:center;gap:8px">
          parallel
          <select class="select" id="set-parallel">
            ${[1,2,3,4,5,6,7,8].map(n => `<option value="${n}" ${String(n) === savedParallel ? "selected" : ""}>${n}</option>`).join("")}
          </select>
        </label>
        <label style="font-size:13px;color:var(--dim);display:flex;align-items:center;gap:8px">
          quality
          <select class="select" id="set-quality">
            <option value="BEST" ${savedQuality === "BEST" ? "selected" : ""}>BEST (highest available)</option>
            <option value="FLAC" ${savedQuality === "FLAC" ? "selected" : ""}>FLAC</option>
            <option value="MP3" ${savedQuality === "MP3" ? "selected" : ""}>MP3</option>
            <option value="OGG" ${savedQuality === "OGG" ? "selected" : ""}>OGG</option>
          </select>
        </label>
      </div>
      <div style="margin-bottom:12px">
        <label style="font-size:13px;color:var(--dim);display:flex;align-items:center;gap:8px;cursor:pointer">
          <input type="checkbox" id="set-skip" ${savedSkip ? "checked" : ""} />
          skip already downloaded files
        </label>
      </div>
    </div>

    <div class="status-section">
      <div class="status-header">about</div>
      <div style="font-size:12px;color:var(--dim);line-height:1.8">
        khinsider helper v1.0.0<br/>
        built with tauri + rust<br/>
        data from downloads.khinsider.com
      </div>
    </div>
  `;

  const outputDir = el.querySelector<HTMLInputElement>("#set-output-dir")!;
  const parallel = el.querySelector<HTMLSelectElement>("#set-parallel")!;
  const quality = el.querySelector<HTMLSelectElement>("#set-quality")!;
  const skip = el.querySelector<HTMLInputElement>("#set-skip")!;

  outputDir.addEventListener("change", () => localStorage.setItem("kh_output_dir", outputDir.value));
  parallel.addEventListener("change", () => localStorage.setItem("kh_parallel", parallel.value));
  quality.addEventListener("change", () => localStorage.setItem("kh_quality", quality.value));
  skip.addEventListener("change", () => localStorage.setItem("kh_skip_existing", String(skip.checked)));
}
