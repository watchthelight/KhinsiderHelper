use crate::models::*;
use crate::state::KhinsiderState;
use futures::StreamExt;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::Semaphore;

fn format_extension(format: &str) -> &'static str {
    match format.to_uppercase().as_str() {
        "FLAC" => ".flac",
        "MP3" => ".mp3",
        "OGG" => ".ogg",
        "AAC" | "ALAC" | "M4A" => ".m4a",
        _ => ".mp3",
    }
}

fn sanitize_path(s: &str) -> String {
    sanitize_filename::sanitize_with_options(s, sanitize_filename::Options {
        replacement: "-",
        ..Default::default()
    })
}

#[tauri::command]
pub async fn start_album_download(
    request: DownloadRequest,
    state: tauri::State<'_, KhinsiderState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let client = {
        let guard = state.client.lock().await;
        guard.clone().unwrap_or_else(|| {
            reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .build()
                .unwrap()
        })
    };

    state.cancel_flag.store(false, Ordering::SeqCst);
    let cancel_flag = state.cancel_flag.clone();

    // Determine output dir
    let output_dir = if request.output_dir.is_empty() {
        get_default_output_directory()
    } else {
        request.output_dir.clone()
    };

    let album_dir = PathBuf::from(&output_dir).join(sanitize_path(&request.album.name));
    std::fs::create_dir_all(&album_dir)
        .map_err(|e| format!("cannot create output dir: {e}"))?;

    #[cfg(target_os = "windows")]
    set_prevent_sleep(true);

    let sem = Arc::new(Semaphore::new(request.parallel.clamp(1, 8)));
    let total = request.album.songs.len();
    let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let failed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let skipped = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let album_slug = request.album.slug.clone();
    let album_name = request.album.name.clone();

    let mut handles = Vec::new();

    for song in request.album.songs.iter().cloned() {
        let sem = sem.clone();
        let client = client.clone();
        let app = app.clone();
        let format = request.format.clone();
        let album_dir = album_dir.clone();
        let skip_existing = request.skip_existing;
        let cancel = cancel_flag.clone();
        let completed = completed.clone();
        let failed = failed.clone();
        let skipped = skipped.clone();
        let slug = album_slug.clone();
        let a_name = album_name.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            if cancel.load(Ordering::SeqCst) { return; }

            let _ = app.emit("track-status", TrackProgress {
                album_slug: slug.clone(),
                track_name: song.name.clone(),
                status: "Resolving".to_string(),
                bytes_downloaded: 0, bytes_total: 0,
                error: None,
            });

            // Resolve CDN download link
            let song_url = if song.song_page_url.starts_with("http") {
                song.song_page_url.clone()
            } else {
                format!("https://downloads.khinsider.com{}", song.song_page_url)
            };

            let resolve_resp = match client.get(&song_url).send().await {
                Ok(r) => r,
                Err(e) => {
                    failed.fetch_add(1, Ordering::SeqCst);
                    let _ = app.emit("track-status", TrackProgress {
                        album_slug: slug.clone(), track_name: song.name.clone(),
                        status: "Error".to_string(), bytes_downloaded: 0, bytes_total: 0,
                        error: Some(format!("resolve: {e}")),
                    });
                    emit_album_progress(&app, &slug, &a_name, total, &completed, &failed, &skipped);
                    return;
                }
            };

            let page_text = match resolve_resp.text().await {
                Ok(t) => t,
                Err(e) => {
                    failed.fetch_add(1, Ordering::SeqCst);
                    let _ = app.emit("track-status", TrackProgress {
                        album_slug: slug.clone(), track_name: song.name.clone(),
                        status: "Error".to_string(), bytes_downloaded: 0, bytes_total: 0,
                        error: Some(format!("read page: {e}")),
                    });
                    emit_album_progress(&app, &slug, &a_name, total, &completed, &failed, &skipped);
                    return;
                }
            };

            // Parse CDN links from the page (scoped so `doc` drops before next await)
            let cdn_links = {
                let doc = scraper::Html::parse_document(&page_text);
                let link_sel = scraper::Selector::parse("#pageContent a").unwrap();
                let mut links: std::collections::HashMap<String, String> = std::collections::HashMap::new();

                for link in doc.select(&link_sel) {
                    let href = link.value().attr("href").unwrap_or("");
                    if href.contains("vgmtreasurechest.com") || href.contains("vgmdownloads.com") {
                        let fmt = if href.ends_with(".flac") { "FLAC" }
                            else if href.ends_with(".ogg") { "OGG" }
                            else if href.ends_with(".m4a") { "AAC" }
                            else { "MP3" };
                        links.insert(fmt.to_string(), href.to_string());
                    }
                }
                links
            };

            if cdn_links.is_empty() {
                failed.fetch_add(1, Ordering::SeqCst);
                let _ = app.emit("track-status", TrackProgress {
                    album_slug: slug.clone(), track_name: song.name.clone(),
                    status: "Error".to_string(), bytes_downloaded: 0, bytes_total: 0,
                    error: Some("no CDN links found".to_string()),
                });
                emit_album_progress(&app, &slug, &a_name, total, &completed, &failed, &skipped);
                return;
            }

            // Select format
            let chosen_format;
            let download_url;
            if format == "BEST" {
                let best = cdn_links.iter()
                    .max_by_key(|(f, _)| quality_rank(f))
                    .map(|(f, u)| (f.clone(), u.clone()));
                match best {
                    Some((f, u)) => { chosen_format = f; download_url = u; }
                    None => {
                        failed.fetch_add(1, Ordering::SeqCst);
                        let _ = app.emit("track-status", TrackProgress {
                            album_slug: slug.clone(), track_name: song.name.clone(),
                            status: "Error".to_string(), bytes_downloaded: 0, bytes_total: 0,
                            error: Some("no suitable format".to_string()),
                        });
                        emit_album_progress(&app, &slug, &a_name, total, &completed, &failed, &skipped);
                        return;
                    }
                }
            } else if let Some(url) = cdn_links.get(&format) {
                chosen_format = format.clone();
                download_url = url.clone();
            } else if let Some((f, u)) = cdn_links.iter().next() {
                // Fallback to any available format
                chosen_format = f.clone();
                download_url = u.clone();
            } else {
                failed.fetch_add(1, Ordering::SeqCst);
                emit_album_progress(&app, &slug, &a_name, total, &completed, &failed, &skipped);
                return;
            }

            let ext = format_extension(&chosen_format);
            let filename = format!("{} {}{}", sanitize_path(&song.track_number), sanitize_path(&song.name), ext);
            let file_path = album_dir.join(&filename);

            if skip_existing && file_path.exists() {
                skipped.fetch_add(1, Ordering::SeqCst);
                let _ = app.emit("track-status", TrackProgress {
                    album_slug: slug.clone(), track_name: song.name.clone(),
                    status: "Skipped".to_string(), bytes_downloaded: 0, bytes_total: 0,
                    error: None,
                });
                emit_album_progress(&app, &slug, &a_name, total, &completed, &failed, &skipped);
                return;
            }

            if cancel.load(Ordering::SeqCst) { return; }

            let _ = app.emit("track-status", TrackProgress {
                album_slug: slug.clone(), track_name: song.name.clone(),
                status: "Downloading".to_string(), bytes_downloaded: 0, bytes_total: 0,
                error: None,
            });

            // Stream download
            let resp = match client.get(&download_url).send().await {
                Ok(r) => r,
                Err(e) => {
                    failed.fetch_add(1, Ordering::SeqCst);
                    let _ = app.emit("track-status", TrackProgress {
                        album_slug: slug.clone(), track_name: song.name.clone(),
                        status: "Error".to_string(), bytes_downloaded: 0, bytes_total: 0,
                        error: Some(format!("download: {e}")),
                    });
                    emit_album_progress(&app, &slug, &a_name, total, &completed, &failed, &skipped);
                    return;
                }
            };

            if !resp.status().is_success() {
                failed.fetch_add(1, Ordering::SeqCst);
                let _ = app.emit("track-status", TrackProgress {
                    album_slug: slug.clone(), track_name: song.name.clone(),
                    status: "Error".to_string(), bytes_downloaded: 0, bytes_total: 0,
                    error: Some(format!("HTTP {}", resp.status())),
                });
                emit_album_progress(&app, &slug, &a_name, total, &completed, &failed, &skipped);
                return;
            }

            let total_size = resp.content_length().unwrap_or(0);
            let mut stream = resp.bytes_stream();
            let mut file = match tokio::fs::File::create(&file_path).await {
                Ok(f) => f,
                Err(e) => {
                    failed.fetch_add(1, Ordering::SeqCst);
                    let _ = app.emit("track-status", TrackProgress {
                        album_slug: slug.clone(), track_name: song.name.clone(),
                        status: "Error".to_string(), bytes_downloaded: 0, bytes_total: 0,
                        error: Some(format!("create file: {e}")),
                    });
                    emit_album_progress(&app, &slug, &a_name, total, &completed, &failed, &skipped);
                    return;
                }
            };

            let mut downloaded: u64 = 0;
            let mut last_emit = std::time::Instant::now();
            use tokio::io::AsyncWriteExt;

            let mut download_err = None;
            while let Some(chunk) = stream.next().await {
                if cancel.load(Ordering::SeqCst) {
                    drop(file);
                    let _ = tokio::fs::remove_file(&file_path).await;
                    return;
                }
                match chunk {
                    Ok(data) => {
                        if let Err(e) = file.write_all(&data).await {
                            download_err = Some(format!("write: {e}"));
                            break;
                        }
                        downloaded += data.len() as u64;

                        if last_emit.elapsed() >= std::time::Duration::from_millis(100) {
                            let _ = app.emit("track-progress", TrackProgress {
                                album_slug: slug.clone(),
                                track_name: song.name.clone(),
                                status: "Downloading".to_string(),
                                bytes_downloaded: downloaded,
                                bytes_total: total_size,
                                error: None,
                            });
                            last_emit = std::time::Instant::now();
                        }
                    }
                    Err(e) => {
                        download_err = Some(format!("stream: {e}"));
                        break;
                    }
                }
            }

            if let Some(err) = download_err {
                drop(file);
                let _ = tokio::fs::remove_file(&file_path).await;
                failed.fetch_add(1, Ordering::SeqCst);
                let _ = app.emit("track-status", TrackProgress {
                    album_slug: slug.clone(), track_name: song.name.clone(),
                    status: "Error".to_string(), bytes_downloaded: downloaded, bytes_total: total_size,
                    error: Some(err),
                });
            } else {
                let _ = file.flush().await;
                completed.fetch_add(1, Ordering::SeqCst);
                let _ = app.emit("track-status", TrackProgress {
                    album_slug: slug.clone(), track_name: song.name.clone(),
                    status: "Done".to_string(), bytes_downloaded: downloaded, bytes_total: total_size,
                    error: None,
                });
            }

            emit_album_progress(&app, &slug, &a_name, total, &completed, &failed, &skipped);

            // Small delay between downloads
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        });

        handles.push(handle);
    }

    for handle in handles { let _ = handle.await; }

    // Emit final progress
    let _ = app.emit("album-progress", AlbumProgress {
        album_slug: album_slug.clone(),
        album_name: album_name.clone(),
        total_tracks: total,
        completed: completed.load(Ordering::SeqCst),
        failed: failed.load(Ordering::SeqCst),
        skipped: skipped.load(Ordering::SeqCst),
        status: "Done".to_string(),
    });

    // Download album art
    if let Some(art_url) = request.album.images.first() {
        let art_path = album_dir.join("cover.jpg");
        if !art_path.exists() {
            if let Ok(resp) = client.get(art_url).send().await {
                if let Ok(bytes) = resp.bytes().await {
                    let _ = std::fs::write(&art_path, &bytes);
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    set_prevent_sleep(false);

    Ok(())
}

fn emit_album_progress(
    app: &tauri::AppHandle,
    slug: &str,
    name: &str,
    total: usize,
    completed: &std::sync::atomic::AtomicUsize,
    failed: &std::sync::atomic::AtomicUsize,
    skipped: &std::sync::atomic::AtomicUsize,
) {
    let c = completed.load(Ordering::SeqCst);
    let f = failed.load(Ordering::SeqCst);
    let s = skipped.load(Ordering::SeqCst);
    let _ = app.emit("album-progress", AlbumProgress {
        album_slug: slug.to_string(),
        album_name: name.to_string(),
        total_tracks: total,
        completed: c,
        failed: f,
        skipped: s,
        status: if c + f + s >= total { "Done".to_string() } else { "Downloading".to_string() },
    });
}

#[tauri::command]
pub async fn cancel_downloads(state: tauri::State<'_, KhinsiderState>) -> Result<(), String> {
    state.cancel_flag.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub fn get_default_output_directory() -> String {
    let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join("Music").join("Khinsider").display().to_string()
}

#[cfg(target_os = "windows")]
fn set_prevent_sleep(enable: bool) {
    use windows_sys::Win32::System::Power::SetThreadExecutionState;
    use windows_sys::Win32::System::Power::{ES_CONTINUOUS, ES_SYSTEM_REQUIRED};
    unsafe {
        if enable {
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
        } else {
            SetThreadExecutionState(ES_CONTINUOUS);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn set_prevent_sleep(_enable: bool) {}

#[tauri::command]
pub async fn start_library_download(
    request: LibraryDownloadRequest,
    state: tauri::State<'_, KhinsiderState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let client = {
        let guard = state.client.lock().await;
        guard.clone().ok_or("not authenticated")?
    };

    state.cancel_flag.store(false, Ordering::SeqCst);
    let cancel_flag = state.cancel_flag.clone();

    let output_dir = if request.output_dir.is_empty() {
        get_default_output_directory()
    } else {
        request.output_dir.clone()
    };

    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("cannot create output dir: {e}"))?;

    #[cfg(target_os = "windows")]
    set_prevent_sleep(true);

    let total = request.albums.len();
    let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let failed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let skipped = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    for album in &request.albums {
        if cancel_flag.load(Ordering::SeqCst) { break; }

        let download_url = match &album.download_url {
            Some(u) if !u.is_empty() => u.clone(),
            _ => {
                let _ = app.emit("track-status", TrackProgress {
                    album_slug: album.slug.clone(),
                    track_name: album.name.clone(),
                    status: "Error".to_string(),
                    bytes_downloaded: 0, bytes_total: 0,
                    error: Some("no download URL".to_string()),
                });
                failed.fetch_add(1, Ordering::SeqCst);
                continue;
            }
        };

        let _ = app.emit("track-status", TrackProgress {
            album_slug: album.slug.clone(),
            track_name: album.name.clone(),
            status: "Downloading".to_string(),
            bytes_downloaded: 0, bytes_total: 0,
            error: None,
        });

        let album_dir = PathBuf::from(&output_dir).join(sanitize_path(&album.name));
        std::fs::create_dir_all(&album_dir).ok();

        let zip_path = album_dir.join(format!("{}.zip", sanitize_path(&album.name)));

        match download_file(&client, &download_url, &zip_path, &album.slug, &album.name, &app, &cancel_flag).await {
            Ok(_) => {
                if request.extract {
                    let _ = app.emit("track-status", TrackProgress {
                        album_slug: album.slug.clone(),
                        track_name: album.name.clone(),
                        status: "Extracting".to_string(),
                        bytes_downloaded: 0, bytes_total: 0,
                        error: None,
                    });
                    match extract_zip(&zip_path, &album_dir) {
                        Ok(_) => { let _ = std::fs::remove_file(&zip_path); }
                        Err(e) => {
                            eprintln!("[dl] extract error: {}", e);
                        }
                    }
                }
                completed.fetch_add(1, Ordering::SeqCst);
                let _ = app.emit("track-status", TrackProgress {
                    album_slug: album.slug.clone(),
                    track_name: album.name.clone(),
                    status: "Done".to_string(),
                    bytes_downloaded: 0, bytes_total: 0,
                    error: None,
                });
            }
            Err(e) => {
                failed.fetch_add(1, Ordering::SeqCst);
                let _ = app.emit("track-status", TrackProgress {
                    album_slug: album.slug.clone(),
                    track_name: album.name.clone(),
                    status: "Error".to_string(),
                    bytes_downloaded: 0, bytes_total: 0,
                    error: Some(e),
                });
            }
        }

        let c = completed.load(Ordering::SeqCst);
        let f = failed.load(Ordering::SeqCst);
        let s = skipped.load(Ordering::SeqCst);
        let _ = app.emit("album-progress", AlbumProgress {
            album_slug: "library".to_string(),
            album_name: "library download".to_string(),
            total_tracks: total,
            completed: c, failed: f, skipped: s,
            status: if c + f + s >= total { "Done".to_string() } else { "Downloading".to_string() },
        });

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    #[cfg(target_os = "windows")]
    set_prevent_sleep(false);

    Ok(())
}

async fn download_file(
    client: &reqwest::Client,
    url: &str,
    dest: &std::path::Path,
    slug: &str,
    name: &str,
    app: &tauri::AppHandle,
    cancel: &Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), String> {
    let resp = client.get(url).send().await.map_err(|e| format!("request: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let total_size = resp.content_length().unwrap_or(0);
    let mut stream = resp.bytes_stream();
    let mut file = tokio::fs::File::create(dest).await.map_err(|e| format!("create: {e}"))?;

    let mut downloaded: u64 = 0;
    let mut last_emit = std::time::Instant::now();
    use tokio::io::AsyncWriteExt;

    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::SeqCst) {
            drop(file);
            let _ = tokio::fs::remove_file(dest).await;
            return Err("cancelled".to_string());
        }
        let data = chunk.map_err(|e| format!("stream: {e}"))?;
        file.write_all(&data).await.map_err(|e| format!("write: {e}"))?;
        downloaded += data.len() as u64;

        if last_emit.elapsed() >= std::time::Duration::from_millis(100) {
            let _ = app.emit("track-progress", TrackProgress {
                album_slug: slug.to_string(),
                track_name: name.to_string(),
                status: "Downloading".to_string(),
                bytes_downloaded: downloaded,
                bytes_total: total_size,
                error: None,
            });
            last_emit = std::time::Instant::now();
        }
    }

    file.flush().await.map_err(|e| format!("flush: {e}"))?;
    Ok(())
}

fn extract_zip(zip_path: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("open zip: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("read zip: {e}"))?;
    std::fs::create_dir_all(dest).map_err(|e| format!("create dir: {e}"))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("zip entry: {e}"))?;
        let outpath = dest.join(entry.enclosed_name().ok_or("invalid zip entry name")?);
        if entry.is_dir() {
            std::fs::create_dir_all(&outpath).map_err(|e| format!("mkdir: {e}"))?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
            }
            let mut outfile = std::fs::File::create(&outpath).map_err(|e| format!("create: {e}"))?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| format!("extract: {e}"))?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn check_local_albums(album_names: Vec<String>, track_counts: Vec<usize>, output_dir: String) -> Vec<LocalAlbumStatus> {
    let base = if output_dir.is_empty() {
        PathBuf::from(get_default_output_directory())
    } else {
        PathBuf::from(&output_dir)
    };

    album_names.iter().zip(track_counts.iter()).map(|(name, &expected)| {
        let dir = base.join(sanitize_path(name));
        if !dir.exists() {
            return LocalAlbumStatus {
                name: name.clone(), exists: false,
                total_tracks: expected, local_tracks: 0,
                missing_tracks: vec![],
            };
        }

        let local_files: Vec<String> = std::fs::read_dir(&dir)
            .map(|rd| rd.filter_map(|e| e.ok())
                .filter(|e| {
                    let p = e.path();
                    p.is_file() && matches!(
                        p.extension().and_then(|e| e.to_str()),
                        Some("mp3" | "flac" | "ogg" | "m4a" | "wav" | "aiff")
                    )
                })
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect())
            .unwrap_or_default();

        LocalAlbumStatus {
            name: name.clone(),
            exists: true,
            total_tracks: expected,
            local_tracks: local_files.len(),
            missing_tracks: vec![],
        }
    }).collect()
}
