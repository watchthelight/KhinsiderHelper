const { invoke } = (window as any).__TAURI__.core;

export interface AuthStatus {
  authenticated: boolean;
  username: string;
}

export interface SearchResult {
  name: string;
  slug: string;
  url: string;
  platform: string;
  album_type: string;
  year: string;
  thumb_url: string | null;
}

export interface SongInfo {
  track_number: string;
  name: string;
  duration: string;
  song_page_url: string;
  sizes: Record<string, string>;
}

export interface AlbumDetail {
  name: string;
  slug: string;
  url: string;
  platforms: string[];
  year: string;
  publisher: string;
  developer: string;
  album_type: string;
  images: string[];
  formats: string[];
  songs: SongInfo[];
  total_size_mp3: string;
  total_size_flac: string;
  file_count: number;
  zip_urls: Record<string, string>;
}

export interface LocalAlbumStatus {
  name: string;
  exists: boolean;
  total_tracks: number;
  local_tracks: number;
  missing_tracks: string[];
}

export interface LibraryAlbum {
  name: string;
  slug: string;
  url: string;
  thumb_url: string | null;
  download_url: string | null;
}

export interface LibraryDownloadRequest {
  albums: LibraryAlbum[];
  output_dir: string;
  extract: boolean;
}

export interface DownloadRequest {
  album: AlbumDetail;
  format: string;
  output_dir: string;
  parallel: number;
  skip_existing: boolean;
}

export interface TrackProgress {
  album_slug: string;
  track_name: string;
  status: string;
  bytes_downloaded: number;
  bytes_total: number;
  error: string | null;
}

export interface AlbumProgress {
  album_slug: string;
  album_name: string;
  total_tracks: number;
  completed: number;
  failed: number;
  skipped: number;
  status: string;
}

export async function tryRestoreSession(): Promise<number> {
  return invoke("try_restore_session");
}

export async function login(): Promise<number> {
  return invoke("login");
}

export async function checkAuth(): Promise<AuthStatus> {
  return invoke("check_auth");
}

export async function fetchLibrary(): Promise<LibraryAlbum[]> {
  return invoke("fetch_library");
}

export async function searchAlbums(query: string): Promise<SearchResult[]> {
  return invoke("search_albums", { query });
}

export async function fetchAlbumDetail(slug: string): Promise<AlbumDetail> {
  return invoke("fetch_album_detail", { slug });
}

export async function startAlbumDownload(request: DownloadRequest): Promise<void> {
  return invoke("start_album_download", { request });
}

export async function startLibraryDownload(request: LibraryDownloadRequest): Promise<void> {
  return invoke("start_library_download", { request });
}

export async function checkLocalAlbums(albumNames: string[], trackCounts: number[], outputDir: string): Promise<LocalAlbumStatus[]> {
  return invoke("check_local_albums", { albumNames, trackCounts, outputDir });
}

export async function cancelDownloads(): Promise<void> {
  return invoke("cancel_downloads");
}

export async function getDefaultOutputDirectory(): Promise<string> {
  return invoke("get_default_output_directory");
}
