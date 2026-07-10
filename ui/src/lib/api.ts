/**
 * Covername Tauri API — typed wrappers around invoke() calls.
 */
import { invoke } from "@tauri-apps/api/core";

export interface Detection {
  matched_text: string;
  entity_type: string;
  replacement: string;
  start: number;
  end: number;
  context: string;
}

export interface ScanResult {
  text: string;
  detections: Detection[];
}

export interface Replacement {
  original: string;
  replacement: string;
}

/** Scan a file for PII. Returns the text content and all detections. */
export async function scanFile(path: string): Promise<ScanResult> {
  return invoke<ScanResult>("scan_file", { path });
}

/** Generate an anonymized output file with the given replacements applied. */
export async function generateOutput(
  path: string,
  replacements: Replacement[]
): Promise<string> {
  return invoke<string>("generate_output", { path, replacements });
}
