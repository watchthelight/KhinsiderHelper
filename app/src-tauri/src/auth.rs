use std::sync::Arc;
use reqwest::cookie::Jar;
use tauri::{Emitter, Manager};

use crate::state::KhinsiderState;
use crate::models::{AuthStatus, LibraryAlbum};

#[tauri::command]
pub async fn try_restore_session(
    state: tauri::State<'_, KhinsiderState>,
    app: tauri::AppHandle,
) -> Result<usize, String> {
    let login_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?
        .join("LoginSession");

    let jar = Arc::new(Jar::default());
    let count = extract_webview2_cookies(&login_data_dir, &jar).unwrap_or(0);

    if count == 0 {
        return Err("no saved session".to_string());
    }

    let client = reqwest::Client::builder()
        .cookie_provider(jar.clone())
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .map_err(|e| format!("http client: {e}"))?;

    *state.cookies.lock().await = Some(jar);
    *state.client.lock().await = Some(client);

    Ok(count)
}

#[tauri::command]
pub async fn login(
    state: tauri::State<'_, KhinsiderState>,
    app: tauri::AppHandle,
) -> Result<usize, String> {
    use tauri::WebviewWindowBuilder;

    if let Some(w) = app.get_webview_window("kh-login") {
        let _ = w.destroy();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    let login_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?
        .join("LoginSession");

    let _ = app.emit("login-status", "opening khinsider login...");

    let _win = WebviewWindowBuilder::new(
        &app,
        "kh-login",
        tauri::WebviewUrl::External("https://downloads.khinsider.com/forums/login".parse().unwrap()),
    )
    .title("khinsider — log in")
    .inner_size(520.0, 640.0)
    .center()
    .data_directory(login_data_dir.clone())
    .build()
    .map_err(|e| format!("create login window: {e}"))?;

    let _ = app.emit("login-status", "log in to khinsider in the window...");
    eprintln!("[login] window opened, waiting for login...");

    loop {
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        let w = match app.get_webview_window("kh-login") {
            Some(w) => w,
            None => return Err("login window was closed".to_string()),
        };
        let url = w.url().map(|u| u.to_string()).unwrap_or_default();
        if !url.contains("/forums/login") && url.contains("khinsider.com") {
            eprintln!("[login] detected redirect: {}", url);
            let _ = app.emit("login-status", "logged in, closing window...");
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            break;
        }
    }

    if let Some(w) = app.get_webview_window("kh-login") {
        let _ = w.destroy();
    }
    let _ = app.emit("login-status", "extracting cookies...");
    eprintln!("[login] window closed, waiting for DB release...");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let jar = Arc::new(Jar::default());
    let count = extract_webview2_cookies(&login_data_dir, &jar)?;
    eprintln!("[login] extracted {} khinsider cookies", count);

    if count == 0 {
        return Err("no khinsider cookies found — login may have failed".to_string());
    }

    let client = reqwest::Client::builder()
        .cookie_provider(jar.clone())
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .map_err(|e| format!("http client: {e}"))?;

    *state.cookies.lock().await = Some(jar);
    *state.client.lock().await = Some(client);

    let _ = app.emit("login-status", format!("authenticated ({} cookies)", count));
    Ok(count)
}

#[tauri::command]
pub async fn check_auth(
    state: tauri::State<'_, KhinsiderState>,
) -> Result<AuthStatus, String> {
    let client = state.client.lock().await;
    let client = client.as_ref().ok_or("not logged in")?;

    let resp = client
        .get("https://downloads.khinsider.com/forums/index.php")
        .send().await
        .map_err(|e| format!("fetch: {e}"))?;
    let text = resp.text().await.map_err(|e| format!("read: {e}"))?;

    // Check if the page shows a logged-in user
    let doc = scraper::Html::parse_document(&text);
    let logged_in_sel = scraper::Selector::parse(".username, .LoggedIn, #userBar .username").unwrap();

    if let Some(el) = doc.select(&logged_in_sel).next() {
        let username = el.text().collect::<String>().trim().to_string();
        Ok(AuthStatus { authenticated: true, username })
    } else {
        // Fallback: check if "Log Out" link exists
        let logout_sel = scraper::Selector::parse("a[href*='logout']").unwrap();
        if doc.select(&logout_sel).next().is_some() {
            Ok(AuthStatus { authenticated: true, username: "user".to_string() })
        } else {
            Ok(AuthStatus { authenticated: false, username: String::new() })
        }
    }
}

#[tauri::command]
pub async fn fetch_library(
    state: tauri::State<'_, KhinsiderState>,
    app: tauri::AppHandle,
) -> Result<Vec<LibraryAlbum>, String> {
    let client = state.client.lock().await;
    let client = client.as_ref().ok_or("not logged in")?;

    let _ = app.emit("login-status", "fetching downloads page...");

    let resp = client
        .get("https://downloads.khinsider.com/cp/downloads")
        .send().await
        .map_err(|e| format!("fetch: {e}"))?;
    let text = resp.text().await.map_err(|e| format!("read: {e}"))?;

    eprintln!("[library] page length: {} bytes", text.len());

    let doc = scraper::Html::parse_document(&text);

    let mut albums = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Look for rows/sections containing album + download links
    let row_sel = scraper::Selector::parse("tr, li, div.download, div.row, .albumDownload").unwrap();
    let album_link_sel = scraper::Selector::parse("a[href*='/game-soundtracks/album/']").unwrap();
    let download_link_sel = scraper::Selector::parse("a[href*='download'], a[href*='.zip'], a[href*='vgmtreasurechest'], a[href*='vgmdownloads']").unwrap();

    for row in doc.select(&row_sel) {
        let album_el = match row.select(&album_link_sel).next() {
            Some(el) => el,
            None => continue,
        };

        let href = album_el.value().attr("href").unwrap_or("");
        let name = album_el.text().collect::<String>().trim().to_string();
        if name.is_empty() { continue; }

        let slug = href.rsplit("/game-soundtracks/album/").next().unwrap_or("")
            .trim_end_matches('/').to_string();
        if slug.is_empty() || !seen.insert(slug.clone()) { continue; }

        let download_url = row.select(&download_link_sel).next()
            .and_then(|dl| dl.value().attr("href"))
            .map(|u| if u.starts_with("http") { u.to_string() } else { format!("https://downloads.khinsider.com{}", u) });

        let img_sel = scraper::Selector::parse("img").unwrap();
        let thumb = row.select(&img_sel).next()
            .and_then(|img| img.value().attr("src").map(|s| s.to_string()));

        eprintln!("[library] found: {} | dl: {:?}", name, download_url);

        albums.push(LibraryAlbum {
            name,
            url: if href.starts_with("http") { href.to_string() } else { format!("https://downloads.khinsider.com{}", href) },
            slug,
            thumb_url: thumb,
            download_url,
        });
    }

    // Fallback: find any album links on the page
    if albums.is_empty() {
        let any_link_sel = scraper::Selector::parse("a[href*='/game-soundtracks/album/']").unwrap();
        for el in doc.select(&any_link_sel) {
            let href = el.value().attr("href").unwrap_or("");
            let name = el.text().collect::<String>().trim().to_string();
            if name.is_empty() { continue; }

            let slug = href.rsplit("/game-soundtracks/album/").next().unwrap_or("")
                .trim_end_matches('/').to_string();
            if slug.is_empty() || !seen.insert(slug.clone()) { continue; }

            albums.push(LibraryAlbum {
                name,
                url: if href.starts_with("http") { href.to_string() } else { format!("https://downloads.khinsider.com{}", href) },
                slug,
                thumb_url: None,
                download_url: None,
            });
        }
    }

    let _ = app.emit("login-status", format!("found {} albums", albums.len()));
    Ok(albums)
}

fn extract_webview2_cookies(
    login_data_dir: &std::path::Path,
    jar: &Arc<Jar>,
) -> Result<usize, String> {
    use rusqlite::Connection;

    let cookies_path = login_data_dir
        .join("EBWebView").join("Default").join("Network").join("Cookies");

    if !cookies_path.exists() {
        return Err(format!("cookie DB not found: {}", cookies_path.display()));
    }

    eprintln!("[cookies] reading: {}", cookies_path.display());

    let local_state_path = login_data_dir.join("EBWebView").join("Local State");
    let master_key = read_master_key(&local_state_path)?;
    eprintln!("[cookies] master key: {} bytes", master_key.len());

    let tmp = std::env::temp_dir().join("kh_login_cookies.sqlite");
    let mut copied = false;
    for attempt in 0..8 {
        match std::fs::copy(&cookies_path, &tmp) {
            Ok(_) => { copied = true; break; }
            Err(e) => {
                eprintln!("[cookies] copy attempt {}: {}", attempt, e);
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }
    if !copied {
        return Err("cookie DB still locked after 8 retries".to_string());
    }
    eprintln!("[cookies] DB copied");

    let conn = Connection::open_with_flags(
        &tmp,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("open DB: {e}"))?;

    let mut count = 0usize;
    {
        let mut stmt = conn
            .prepare("SELECT host_key, name, value, encrypted_value FROM cookies WHERE host_key LIKE '%khinsider.com%'")
            .map_err(|e| format!("query: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            })
            .map_err(|e| format!("query: {e}"))?;

        for row in rows {
            if let Ok((host, name, value, enc_value)) = row {
                let real_value = if !value.is_empty() {
                    value
                } else if enc_value.len() > 3 {
                    match decrypt_cookie(&master_key, &enc_value) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("[cookies] decrypt failed: {} — {}", name, e);
                            continue;
                        }
                    }
                } else {
                    continue;
                };

                if real_value.is_empty() { continue; }

                let url_str = format!("https://{}", host.trim_start_matches('.'));
                if let Ok(url) = url_str.parse::<url::Url>() {
                    let hex: String = real_value.bytes().take(16).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
                    eprintln!("[cookies] {} = [{}] {}...", name, hex, &real_value[..real_value.len().min(30)]);

                    jar.add_cookie_str(
                        &format!("{}={}; Domain=.downloads.khinsider.com; Path=/", name, real_value),
                        &url,
                    );
                    count += 1;
                }
            }
        }
    }

    drop(conn);
    let _ = std::fs::remove_file(&tmp);

    Ok(count)
}

fn read_master_key(local_state_path: &std::path::Path) -> Result<Vec<u8>, String> {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;

    let text = std::fs::read_to_string(local_state_path)
        .map_err(|e| format!("read Local State: {e}"))?;
    let data: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("parse Local State: {e}"))?;

    let enc_key_b64 = data["os_crypt"]["encrypted_key"]
        .as_str()
        .ok_or("no encrypted_key")?;
    let enc_key_raw = STANDARD.decode(enc_key_b64).map_err(|e| format!("base64: {e}"))?;

    let enc_key = if enc_key_raw.starts_with(b"DPAPI") { &enc_key_raw[5..] } else { &enc_key_raw };
    dpapi_decrypt(enc_key)
}

fn decrypt_cookie(master_key: &[u8], encrypted: &[u8]) -> Result<String, String> {
    use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
    use aes_gcm::Nonce;

    if encrypted.len() < 3 + 12 + 16 { return Err("too short".to_string()); }

    let prefix = &encrypted[..3];
    if prefix == b"v10" || prefix == b"v11" || prefix == b"v12" {
        let nonce = Nonce::from_slice(&encrypted[3..15]);
        let ciphertext = &encrypted[15..];
        let cipher = Aes256Gcm::new_from_slice(master_key).map_err(|e| format!("aes: {e}"))?;
        let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|e| format!("decrypt: {e}"))?;
        let value_bytes = if plaintext.len() > 32 {
            &plaintext[32..]
        } else {
            &plaintext
        };
        return Ok(String::from_utf8_lossy(value_bytes).to_string());
    }

    let decrypted = dpapi_decrypt(encrypted)?;
    Ok(String::from_utf8_lossy(&decrypted).to_string())
}

#[cfg(target_os = "windows")]
fn dpapi_decrypt(data: &[u8]) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    use windows_sys::Win32::Foundation::LocalFree;

    unsafe {
        let mut input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };

        if CryptUnprotectData(&mut input, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), 0, &mut output) == 0 {
            return Err("DPAPI failed".to_string());
        }

        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as _);
        Ok(result)
    }
}

#[cfg(not(target_os = "windows"))]
fn dpapi_decrypt(_data: &[u8]) -> Result<Vec<u8>, String> {
    Err("DPAPI not available".to_string())
}
