<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  interface BatchFileResult {
    path: string;
    status: string;
    detections: number;
    output_path: string | null;
    error: string | null;
  }

  interface Props {
    paths: string[];
    onClose: () => void;
  }

  let { paths, onClose }: Props = $props();
  let files = $state<string[]>([]);
  let processing = $state(false);
  let loading = $state(true);
  let results = $state<BatchFileResult[]>([]);
  let done = $state(false);

  // Resolve paths on mount: if a single directory, list its files
  onMount(() => {
    resolveFiles();
  });

  async function resolveFiles() {
    loading = true;
    if (paths.length === 1) {
      try {
        const listed = await invoke<string[]>("list_supported_files", { path: paths[0] });
        files = listed;
      } catch {
        // Not a folder — treat as a single file
        files = paths;
      }
    } else {
      files = paths;
    }
    loading = false;
  }

  async function handleProcess() {
    processing = true;
    try {
      results = await invoke<BatchFileResult[]>("batch_process", { paths: files });
      done = true;
    } catch (e) {
      results = [{
        path: "batch",
        status: "error",
        detections: 0,
        output_path: null,
        error: String(e),
      }];
      done = true;
    } finally {
      processing = false;
    }
  }

  let successCount = $derived(results.filter((r) => r.status === "success").length);
  let errorCount = $derived(results.filter((r) => r.status === "error").length);
  let skippedCount = $derived(results.filter((r) => r.status === "skipped").length);

  function filename(path: string): string {
    return path.split("/").pop() || path;
  }
</script>

<div class="batch-view">
  <div class="batch-panel">
    <div class="batch-header">
      <h2>{done ? "Batch Complete" : "Batch Processing"}</h2>
      <span class="file-count">{files.length} file{files.length === 1 ? "" : "s"}</span>
    </div>

    {#if loading}
      <div class="file-list" style="display:flex;align-items:center;justify-content:center;min-height:100px;">
        <span style="color:var(--color-text-muted);font-size:var(--text-sm);">Loading files…</span>
      </div>
    {:else if !done}
      <!-- File list before processing -->
      <div class="file-list">
        {#each files as file}
          <div class="file-item">
            <span class="file-icon">📄</span>
            <span class="file-name">{filename(file)}</span>
          </div>
        {/each}
      </div>

      <div class="batch-actions">
        <button class="btn-process" onclick={handleProcess} disabled={processing || files.length === 0}>
          {#if processing}
            Processing…
          {:else}
            Process All ({files.length} files)
          {/if}
        </button>
        <button class="btn-cancel" onclick={onClose}>Cancel</button>
      </div>
    {:else}
      <!-- Results after processing -->
      <div class="batch-summary">
        {#if successCount > 0}
          <span class="summary-success">✅ {successCount} processed</span>
        {/if}
        {#if skippedCount > 0}
          <span class="summary-skipped">⏭ {skippedCount} skipped (no PII found)</span>
        {/if}
        {#if errorCount > 0}
          <span class="summary-error">❌ {errorCount} failed</span>
        {/if}
      </div>

      <div class="file-list">
        {#each results as result}
          <div class="file-item" class:success={result.status === "success"} class:error={result.status === "error"} class:skipped={result.status === "skipped"}>
            <span class="file-status">
              {#if result.status === "success"}✅{:else if result.status === "error"}❌{:else}⏭{/if}
            </span>
            <div class="file-details">
              <span class="file-name">{filename(result.path)}</span>
              {#if result.status === "success"}
                <span class="file-meta">{result.detections} replacements → {filename(result.output_path || "")}</span>
              {:else if result.status === "error"}
                <span class="file-meta file-error">{result.error}</span>
              {:else}
                <span class="file-meta">No PII detected</span>
              {/if}
            </div>
          </div>
        {/each}
      </div>

      <div class="batch-actions">
        <button class="btn-process" onclick={onClose}>Done</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .batch-view {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-8);
  }

  .batch-panel {
    background: var(--color-bg);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    padding: var(--space-6);
    width: 100%;
    max-width: 600px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow-panel);
  }

  .batch-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--space-4);
  }

  .batch-header h2 {
    font-size: var(--text-lg);
    font-weight: 600;
  }

  .file-count {
    font-size: var(--text-sm);
    color: var(--color-text-muted);
  }

  .file-list {
    flex: 1;
    overflow-y: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    max-height: 360px;
  }

  .file-item {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--color-border);
    font-size: var(--text-sm);
  }

  .file-item:last-child {
    border-bottom: none;
  }

  .file-icon, .file-status {
    flex-shrink: 0;
  }

  .file-details {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .file-name {
    color: var(--color-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-meta {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
  }

  .file-error {
    color: var(--color-error);
  }

  .batch-summary {
    display: flex;
    gap: var(--space-4);
    margin-bottom: var(--space-4);
    font-size: var(--text-sm);
  }

  .batch-actions {
    display: flex;
    gap: var(--space-3);
    margin-top: var(--space-4);
  }

  .btn-process {
    flex: 1;
    background: var(--color-primary-500);
    color: white;
    border: none;
    border-radius: var(--radius-md);
    padding: var(--space-2) var(--space-4);
    font-size: var(--text-sm);
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-process:hover:not(:disabled) {
    background: var(--color-primary-600);
  }

  .btn-process:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-cancel {
    background: transparent;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-2) var(--space-4);
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-cancel:hover {
    background: var(--color-bg-tertiary);
  }
</style>
