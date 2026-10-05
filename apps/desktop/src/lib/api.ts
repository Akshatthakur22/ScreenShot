import { invoke } from "@tauri-apps/api/core";

export type CoreConnection = {
  applicationName: string;
  coreConnected: boolean;
};

export type Stats = {
  screenshotCount: number;
  lineCount: number;
};

export type Shot = {
  id: number;
  mtime: number;
  width: number;
  height: number;
  ocrSnippet: string;
};

export type ShotDetail = {
  shot: Shot;
  ocrLines: string[];
};

export type ShotImage = {
  mimeType: string;
  dataUrl: string;
};

export type ApiError = {
  code: string;
  message: string;
};

export const api = {
  checkCoreConnection: () => invoke<CoreConnection>("check_core_connection"),
  getStats: () => invoke<Stats>("get_stats"),
  getRecentShots: (limit = 12) => invoke<Shot[]>("get_recent_shots", { limit }),
  searchShots: (query: string, limit = 50) =>
    invoke<Shot[]>("search_shots", { query, limit }),
  getShot: (id: number) => invoke<Shot>("get_shot", { id }),
  getShotDetail: (id: number) => invoke<ShotDetail>("get_shot_detail", { id }),
  getShotImage: (id: number) => invoke<ShotImage>("get_shot_image", { id }),
  getShotThumbnail: (id: number) => invoke<ShotImage>("get_shot_thumbnail", { id }),
};

export function errorMessage(cause: unknown): string {
  if (typeof cause === "object" && cause !== null && "message" in cause) {
    return String((cause as ApiError).message);
  }
  return cause instanceof Error ? cause.message : String(cause);
}
