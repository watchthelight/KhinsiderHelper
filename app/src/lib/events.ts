const { listen } = (window as any).__TAURI__.event;
import type { TrackProgress, AlbumProgress } from "./tauri.js";

export function onTrackProgress(cb: (p: TrackProgress) => void): Promise<() => void> {
  return listen("track-progress", (event: any) => cb(event.payload));
}

export function onTrackStatus(cb: (p: TrackProgress) => void): Promise<() => void> {
  return listen("track-status", (event: any) => cb(event.payload));
}

export function onAlbumProgress(cb: (p: AlbumProgress) => void): Promise<() => void> {
  return listen("album-progress", (event: any) => cb(event.payload));
}

export function onLoginStatus(cb: (msg: string) => void): Promise<() => void> {
  return listen("login-status", (event: any) => cb(event.payload));
}
