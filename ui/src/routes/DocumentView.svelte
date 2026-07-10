<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import DetectionSidebar from "./DetectionSidebar.svelte";

  interface Detection {
    matched_text: string;
    entity_type: string;
    replacement: string;
    start: number;
    end: number;
    context: string;
    accepted: boolean | null; // null = pending
  }

  interface Props {
    filePath: string;
    onClose: () => void;
  }

  let { filePath, onClose }: Props = $props();
  let documentText = $state("");
  let detections: Detection[] = $state([]);
  let loading = $state(true);
  let generating = $state(false);
  let error = $state<string | null>(null);
  let lastOutputPath = $state<string | null>(null);

  onMount(() => {
    scanFile();
  });

  async function scanFile() {
    loading = true;
    error = null;
    try {
      const result = await invoke<{ text: string; detections: Detection[] }>("scan_file", {
        path: filePath,
      });
      documentText = result.text;
      detections = result.detections.map((d) => ({ ...d, accepted: null }));
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function handleAccept(index: number) {
    detections[index].accepted = true;
  }

  function handleReject(index: number) {
    detections[index].accepted = false;
  }

  function handleEdit(index: number, newReplacement: string) {
    detections[index].replacement = newReplacement;
    detections[index].accepted = true;
  }

  function handleAcceptAll() {
    detections = detections.map((d) => ({ ...d, accepted: d.accepted === false ? false : true }));
  }

  async function handleGenerate() {
    const accepted = detections.filter((d) => d.accepted === true);
    generating = true;
    error = null;
    try {
      const outputPath = await invoke<string>("generate_output", {
        path: filePath,
        replacements: accepted.map((d) => ({
          original: d.matched_text,
          replacement: d.replacement,
          start: d.start,
          end: d.end,
        })),
      });
      // Store output path for reveal action
      lastOutputPath = outputPath;
    } catch (e) {
      error = String(e);
    } finally {
      generating = false;
    }
  }

  async function revealOutput() {
    if (lastOutputPath) {
      await invoke("reveal_in_finder", { path: lastOutputPath });
    }
  }

  /**
   * Memoized highlighted segments — only recomputes when text or detections change.
   */
  let highlightedSegments = $derived.by(() => {
    if (!documentText || detections.length === 0) return [{ text: documentText, type: "normal" as const }];

    const sorted = [...detections].sort((a, b) => a.start - b.start);
    const segments: { text: string; type: string; detection?: Detection }[] = [];
    let lastEnd = 0;

    for (const det of sorted) {
      if (det.start > lastEnd) {
        segments.push({ text: documentText.slice(lastEnd, det.start), type: "normal" });
      }
      segments.push({ text: det.matched_text, type: "detection", detection: det });
      lastEnd = det.end;
    }

    if (lastEnd < documentText.length) {
      segments.push({ text: documentText.slice(lastEnd), type: "normal" });
    }

    return segments;
  });
</script>

<div class="document-view">
  <div class="viewer-panel">
    {#if loading}
      <div class="loading">
        <div class="loading-spinner"></div>
        <span>Scanning for personal information…</span>
      </div>
    {:else if error}
      <div class="error">{error}</div>
    {:else}
      <div class="document-content">
        <pre class="document-text">{#each highlightedSegments as segment}{#if segment.type === "detection"}<span
              class="redaction-mark"
              class:pending={segment.detection?.accepted === null}
              class:accepted={segment.detection?.accepted === true}
              class:rejected={segment.detection?.accepted === false}
            >{#if segment.detection?.accepted === true}{segment.detection.replacement}{:else}{segment.text}{/if}</span>{:else}{segment.text}{/if}{/each}</pre>
      </div>
    {/if}
  </div>

  <DetectionSidebar
    {detections}
    {generating}
    outputReady={lastOutputPath !== null}
    onAccept={handleAccept}
    onReject={handleReject}
    onEdit={handleEdit}
    onAcceptAll={handleAcceptAll}
    onGenerate={handleGenerate}
    onReveal={revealOutput}
  />
</div>

<style>
  .document-view {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  .viewer-panel {
    flex: 2;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    background: var(--color-bg);
  }

  .document-content {
    max-width: 72ch;
    margin: 0 auto;
    padding: var(--space-6);
    flex: 1;
  }

  .document-text {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    line-height: 1.7;
    white-space: pre-wrap;
    word-wrap: break-word;
  }

  .loading {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: var(--space-4);
    color: var(--color-text-secondary);
    font-size: var(--text-base);
  }

  .loading-spinner {
    width: 24px;
    height: 24px;
    border: 2.5px solid var(--color-border);
    border-top-color: var(--color-primary-500);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  .error {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--color-error);
    font-size: var(--text-lg);
    padding: var(--space-6);
  }

  /* Redaction marks */
  .redaction-mark {
    display: inline;
    border-radius: 2px;
    padding: 1px 2px;
    transition: background 0.15s;
  }

  .redaction-mark.pending {
    background: var(--color-highlight-pending);
    border-bottom: 2px solid var(--color-warning);
  }

  .redaction-mark.accepted {
    background: var(--color-redaction);
    color: var(--color-redaction-text);
    font-weight: 600;
    padding: 2px 4px;
  }

  .redaction-mark.rejected {
    background: var(--color-highlight-rejected);
    text-decoration: line-through;
    opacity: 0.6;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
