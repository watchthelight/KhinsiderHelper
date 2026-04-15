use scraper::{Html, Selector};
use std::collections::HashMap;

use crate::models::*;
use crate::state::KhinsiderState;

const BASE_URL: &str = "https://downloads.khinsider.com";

fn anon_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .unwrap()
}

async fn get_client(state: &KhinsiderState) -> reqwest::Client {
    let guard = state.client.lock().await;
    guard.clone().unwrap_or_else(|| anon_client())
}

#[tauri::command]
pub async fn search_albums(query: String) -> Result<Vec<SearchResult>, String> {
    let client = anon_client();
    let url = format!("{}/search?search={}", BASE_URL, urlencoding::encode(&query));
    let resp = client.get(&url).send().await.map_err(|e| format!("search request: {e}"))?;
    let text = resp.text().await.map_err(|e| format!("read response: {e}"))?;

    let doc = Html::parse_document(&text);

    // Try table.albumList first
    let table_sel = Selector::parse("table.albumList").unwrap();
    let row_sel = Selector::parse("tr").unwrap();
    let link_sel = Selector::parse("td a[href*='/game-soundtracks/album/']").unwrap();
    let td_sel = Selector::parse("td").unwrap();

    let mut results = Vec::new();

    if let Some(table) = doc.select(&table_sel).next() {
        let rows: Vec<_> = table.select(&row_sel).collect();
        // Skip header row
        for row in rows.iter().skip(1) {
            let tds: Vec<_> = row.select(&td_sel).collect();
            if tds.is_empty() { continue; }

            // Find album link
            let link = match row.select(&link_sel).next() {
                Some(l) => l,
                None => continue,
            };

            let href = link.value().attr("href").unwrap_or("");
            let name = link.text().collect::<String>().trim().to_string();
            if name.is_empty() { continue; }

            let slug = href.rsplit("/game-soundtracks/album/").next().unwrap_or("")
                .trim_end_matches('/').to_string();

            // Try to get thumbnail
            let img_sel = Selector::parse("img").unwrap();
            let thumb_url = tds.first()
                .and_then(|td| td.select(&img_sel).next())
                .and_then(|img| img.value().attr("src").map(|s| s.to_string()));

            let platform = tds.get(1).map(|td| td.text().collect::<String>().trim().to_string()).unwrap_or_default();
            let album_type = tds.get(2).map(|td| td.text().collect::<String>().trim().to_string()).unwrap_or_default();
            let year = tds.get(3).map(|td| td.text().collect::<String>().trim().to_string()).unwrap_or_default();

            results.push(SearchResult {
                name,
                slug: slug.clone(),
                url: if href.starts_with("http") { href.to_string() } else { format!("{}{}", BASE_URL, href) },
                platform,
                album_type,
                year,
                thumb_url,
            });
        }
    } else {
        // Fallback: find any album links on the page
        let any_link_sel = Selector::parse("a[href*='/game-soundtracks/album/']").unwrap();
        for link in doc.select(&any_link_sel) {
            let href = link.value().attr("href").unwrap_or("");
            let name = link.text().collect::<String>().trim().to_string();
            if name.is_empty() { continue; }

            let slug = href.rsplit("/game-soundtracks/album/").next().unwrap_or("")
                .trim_end_matches('/').to_string();
            if slug.is_empty() { continue; }

            results.push(SearchResult {
                name,
                slug: slug.clone(),
                url: if href.starts_with("http") { href.to_string() } else { format!("{}{}", BASE_URL, href) },
                platform: String::new(),
                album_type: String::new(),
                year: String::new(),
                thumb_url: None,
            });
        }
    }

    Ok(results)
}

#[tauri::command]
pub async fn fetch_album_detail(
    slug: String,
    state: tauri::State<'_, KhinsiderState>,
) -> Result<AlbumDetail, String> {
    let client = get_client(&state).await;
    let url = format!("{}/game-soundtracks/album/{}", BASE_URL, slug);
    let resp = client.get(&url).send().await.map_err(|e| format!("fetch album: {e}"))?;
    let text = resp.text().await.map_err(|e| format!("read response: {e}"))?;

    let doc = Html::parse_document(&text);
    let content_sel = Selector::parse("#pageContent").unwrap();
    let content = doc.select(&content_sel).next().ok_or("page content not found")?;

    // Album name from h2
    let h2_sel = Selector::parse("h2").unwrap();
    let album_name = content.select(&h2_sel).next()
        .map(|h| h.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| slug.clone());

    // Album art images
    let img_sel = Selector::parse("img").unwrap();
    let mut images = Vec::new();
    for img in content.select(&img_sel) {
        if let Some(src) = img.value().attr("src") {
            if src.contains("vgmtreasurechest.com") || src.contains("/albums/") {
                images.push(src.to_string());
            }
        }
    }

    // Parse metadata from page text
    let page_text = content.inner_html();
    let platforms = extract_meta_field(&page_text, "Platforms:");
    let year = extract_meta_single(&page_text, "Year:");
    let publisher = extract_meta_single(&page_text, "Published by:");
    let developer = extract_meta_single(&page_text, "Developed by:");
    let album_type = extract_meta_single(&page_text, "Type:");

    // Parse songlist table
    let songlist_sel = Selector::parse("table#songlist").unwrap();
    let songlist = content.select(&songlist_sel).next()
        .or_else(|| {
            // Fallback: any table with song links
            let table_sel = Selector::parse("table").unwrap();
            content.select(&table_sel).find(|t| {
                let link_sel = Selector::parse("a[href*='/game-soundtracks/album/']").unwrap();
                t.select(&link_sel).next().is_some()
            })
        })
        .ok_or("songlist table not found")?;

    // Detect format columns from header
    let th_sel = Selector::parse("th").unwrap();
    let header_row_sel = Selector::parse("tr").unwrap();
    let mut formats = Vec::new();
    let mut format_col_indices = Vec::new();

    if let Some(header) = songlist.select(&header_row_sel).next() {
        let ths: Vec<_> = header.select(&th_sel).collect();
        for (i, th) in ths.iter().enumerate() {
            let text = th.text().collect::<String>().trim().to_uppercase();
            if matches!(text.as_str(), "MP3" | "FLAC" | "OGG" | "AAC" | "ALAC" | "M4A") {
                formats.push(text.clone());
                format_col_indices.push(i);
            }
        }
    }

    // Parse song rows
    let row_sel = Selector::parse("tr").unwrap();
    let td_sel = Selector::parse("td").unwrap();
    let link_sel = Selector::parse("a").unwrap();
    let mut songs = Vec::new();
    let mut total_size_mp3 = String::new();
    let mut total_size_flac = String::new();

    let rows: Vec<_> = songlist.select(&row_sel).collect();
    for row in rows.iter().skip(1) {
        let tds: Vec<_> = row.select(&td_sel).collect();
        if tds.len() < 3 { continue; }

        // Check if this is the totals row
        let first_text = tds[0].text().collect::<String>().trim().to_string();
        if first_text.to_lowercase().contains("total") {
            // Extract total sizes from format columns
            for (fi, &col_idx) in format_col_indices.iter().enumerate() {
                if let Some(td) = tds.get(col_idx) {
                    let size = td.text().collect::<String>().trim().to_string();
                    if fi < formats.len() {
                        match formats[fi].as_str() {
                            "MP3" => total_size_mp3 = size,
                            "FLAC" => total_size_flac = size,
                            _ => {}
                        }
                    }
                }
            }
            continue;
        }

        // Track number
        let track_number = first_text;

        // Song name and link (usually in the second column with a link)
        let mut song_name = String::new();
        let mut song_page_url = String::new();

        for td in &tds {
            if let Some(link) = td.select(&link_sel).next() {
                let href = link.value().attr("href").unwrap_or("");
                if href.contains("/game-soundtracks/album/") {
                    song_name = link.text().collect::<String>().trim().to_string();
                    song_page_url = href.to_string();
                    break;
                }
            }
        }

        if song_name.is_empty() { continue; }

        // Duration - typically a column with time format
        let mut duration = String::new();
        for td in &tds {
            let t = td.text().collect::<String>().trim().to_string();
            if t.contains(':') && t.len() <= 10 && !t.contains('/') {
                duration = t;
                break;
            }
        }

        // Sizes from format columns
        let mut sizes = HashMap::new();
        for (fi, &col_idx) in format_col_indices.iter().enumerate() {
            if let Some(td) = tds.get(col_idx) {
                let size = td.text().collect::<String>().trim().to_string();
                if !size.is_empty() && fi < formats.len() {
                    sizes.insert(formats[fi].clone(), size);
                }
            }
        }

        songs.push(SongInfo {
            track_number,
            name: song_name,
            duration,
            song_page_url,
            sizes,
        });
    }

    let file_count = songs.len();

    Ok(AlbumDetail {
        name: album_name,
        slug,
        url,
        platforms: if platforms.is_empty() { vec![] } else { platforms.split(", ").map(|s| s.to_string()).collect() },
        year,
        publisher,
        developer,
        album_type,
        images,
        formats,
        songs,
        total_size_mp3,
        total_size_flac,
        file_count,
        zip_urls: HashMap::new(),
    })
}

#[tauri::command]
pub async fn resolve_song_links(
    song_page_url: String,
    state: tauri::State<'_, KhinsiderState>,
) -> Result<SongDownloadLinks, String> {
    let client = get_client(&state).await;
    let full_url = if song_page_url.starts_with("http") {
        song_page_url.clone()
    } else {
        format!("{}{}", BASE_URL, song_page_url)
    };

    let resp = client.get(&full_url).send().await.map_err(|e| format!("fetch song page: {e}"))?;
    let text = resp.text().await.map_err(|e| format!("read: {e}"))?;

    let doc = Html::parse_document(&text);
    let content_sel = Selector::parse("#pageContent").unwrap();
    let content = doc.select(&content_sel).next().ok_or("page content not found")?;

    // Find download links — typically in <p><a> tags pointing to CDN
    let link_sel = Selector::parse("p a, span a, .songDownloadLink a, a").unwrap();
    let mut links = HashMap::new();
    let mut song_name = String::new();

    // Get song name from h2 or page title
    let h2_sel = Selector::parse("h2").unwrap();
    if let Some(h2) = content.select(&h2_sel).next() {
        song_name = h2.text().collect::<String>().trim().to_string();
    }

    for link in content.select(&link_sel) {
        let href = link.value().attr("href").unwrap_or("");

        // CDN links contain these domains
        if href.contains("vgmtreasurechest.com") || href.contains("vgmdownloads.com") {
            let link_text = link.text().collect::<String>().trim().to_string();

            // Determine format from URL extension or link text
            let format = if href.ends_with(".flac") || link_text.to_uppercase().contains("FLAC") {
                "FLAC"
            } else if href.ends_with(".ogg") || link_text.to_uppercase().contains("OGG") {
                "OGG"
            } else if href.ends_with(".m4a") || link_text.to_uppercase().contains("AAC") || link_text.to_uppercase().contains("ALAC") {
                "AAC"
            } else {
                "MP3"
            };

            links.insert(format.to_string(), href.to_string());
        }
    }

    if links.is_empty() {
        return Err("no download links found on song page".to_string());
    }

    Ok(SongDownloadLinks { song_name, links })
}

fn extract_meta_field(html: &str, label: &str) -> String {
    if let Some(pos) = html.find(label) {
        let after = &html[pos + label.len()..];
        // Look for text until the next <b> or </p> or <br
        let end = after.find('<').unwrap_or(after.len());
        let raw = &after[..end];
        // Strip HTML tags and trim
        html_to_text(raw).trim().to_string()
    } else {
        String::new()
    }
}

fn extract_meta_single(html: &str, label: &str) -> String {
    extract_meta_field(html, label)
}

fn html_to_text(html: &str) -> String {
    html.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}
