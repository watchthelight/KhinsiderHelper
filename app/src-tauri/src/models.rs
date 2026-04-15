use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatus {
    pub authenticated: bool,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub name: String,
    pub slug: String,
    pub url: String,
    pub platform: String,
    pub album_type: String,
    pub year: String,
    pub thumb_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongInfo {
    pub track_number: String,
    pub name: String,
    pub duration: String,
    pub song_page_url: String,
    pub sizes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumDetail {
    pub name: String,
    pub slug: String,
    pub url: String,
    pub platforms: Vec<String>,
    pub year: String,
    pub publisher: String,
    pub developer: String,
    pub album_type: String,
    pub images: Vec<String>,
    pub formats: Vec<String>,
    pub songs: Vec<SongInfo>,
    pub total_size_mp3: String,
    pub total_size_flac: String,
    pub file_count: usize,
    pub zip_urls: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalAlbumStatus {
    pub name: String,
    pub exists: bool,
    pub total_tracks: usize,
    pub local_tracks: usize,
    pub missing_tracks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongDownloadLinks {
    pub song_name: String,
    pub links: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryAlbum {
    pub name: String,
    pub slug: String,
    pub url: String,
    pub thumb_url: Option<String>,
    pub download_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDownloadRequest {
    pub albums: Vec<LibraryAlbum>,
    pub output_dir: String,
    pub extract: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRequest {
    pub album: AlbumDetail,
    pub format: String,
    pub output_dir: String,
    pub parallel: usize,
    pub skip_existing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackProgress {
    pub album_slug: String,
    pub track_name: String,
    pub status: String,
    pub bytes_downloaded: u64,
    pub bytes_total: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumProgress {
    pub album_slug: String,
    pub album_name: String,
    pub total_tracks: usize,
    pub completed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub status: String,
}

pub fn quality_rank(format: &str) -> u8 {
    match format.to_uppercase().as_str() {
        "FLAC" => 4,
        "ALAC" => 3,
        "AAC" => 2,
        "OGG" => 2,
        "MP3" => 1,
        _ => 0,
    }
}
