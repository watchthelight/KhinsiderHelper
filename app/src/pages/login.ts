import { tryRestoreSession, login, checkAuth } from "../lib/tauri.js";
import { onLoginStatus } from "../lib/events.js";
import { switchPage } from "../main.js";

function escHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

export async function init(el: HTMLElement): Promise<void> {
  el.innerHTML = `
    <div class="header">
      <h1>khinsider helper</h1>
      <div class="sub">download video game soundtracks</div>
    </div>

    <div class="status-section">
      <div class="status-header">session</div>
      <div class="status-row">
        <div class="status-dot red" id="login-dot"></div>
        <span class="status-label">status</span>
        <span class="status-value" id="login-status">not authenticated</span>
      </div>
    </div>

    <div class="actions">
      <button class="btn primary" id="btn-login">log in to khinsider</button>
    </div>

    <div class="log-section">
      <div class="log-header">log</div>
      <div class="log" id="login-log"></div>
    </div>
  `;

  const dot = el.querySelector<HTMLElement>("#login-dot")!;
  const statusEl = el.querySelector<HTMLElement>("#login-status")!;
  const btn = el.querySelector<HTMLButtonElement>("#btn-login")!;
  const log = el.querySelector<HTMLElement>("#login-log")!;

  function addLog(msg: string, cls: string = ""): void {
    const line = document.createElement("div");
    line.className = `log-line ${cls}`;
    line.innerHTML = `<span class="prefix">&gt;</span>${escHtml(msg)}`;
    log.appendChild(line);
    log.scrollTop = log.scrollHeight;
  }

  function setAuthenticated(username: string): void {
    dot.className = "status-dot green";
    statusEl.textContent = username ? `authenticated as ${username}` : "authenticated";
    btn.textContent = "logged in";
    btn.disabled = true;
  }

  await onLoginStatus((msg: string) => addLog(msg, "info"));

  // Try restoring session
  addLog("checking for saved session...");
  try {
    const count = await tryRestoreSession();
    addLog(`restored ${count} cookies`, "success");

    try {
      const auth = await checkAuth();
      if (auth.authenticated) {
        setAuthenticated(auth.username);
        addLog("session valid", "success");
      } else {
        addLog("session expired — log in again", "warn");
      }
    } catch {
      addLog("session check failed — log in again", "warn");
    }
  } catch {
    addLog("no saved session", "");
  }

  btn.addEventListener("click", async () => {
    btn.classList.add("loading");
    btn.disabled = true;
    addLog("opening login window...");

    try {
      const count = await login();
      addLog(`extracted ${count} cookies`, "success");

      const auth = await checkAuth();
      if (auth.authenticated) {
        setAuthenticated(auth.username);
        addLog("login successful", "success");
      } else {
        addLog("login completed but session not valid", "error");
        btn.classList.remove("loading");
        btn.disabled = false;
        btn.textContent = "log in to khinsider";
      }
    } catch (e: any) {
      addLog(`login failed: ${e}`, "error");
      btn.classList.remove("loading");
      btn.disabled = false;
      btn.textContent = "log in to khinsider";
    }
  });
}
