<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { onMount } from "svelte";
  import { FILE_FILTER } from "../lib/constants";
  import logoSvg from "../assets/logo.svg";

  interface Props {
    onFileSelected: (path: string) => void;
    onBatchSelected: (paths: string[]) => void;
  }

  let { onFileSelected, onBatchSelected }: Props = $props();
  let isDragOver = $state(false);

  onMount(() => {
    const webview = getCurrentWebviewWindow();
    const unlisten = webview.onDragDropEvent((event) => {
      if (event.payload.type === "over") {
        isDragOver = true;
      } else if (event.payload.type === "drop") {
        isDragOver = false;
        const paths = event.payload.paths;
        if (paths && paths.length > 1) {
          onBatchSelected(paths);
        } else if (paths && paths.length === 1) {
          onFileSelected(paths[0]);
        }
      } else if (event.payload.type === "leave") {
        isDragOver = false;
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  });

  async function handleBrowse() {
    try {
      const selected = await open({
        multiple: true,
        directory: false,
        filters: [FILE_FILTER],
      });
      if (selected && Array.isArray(selected) && selected.length > 1) {
        onBatchSelected(selected);
      } else if (selected) {
        const path = Array.isArray(selected) ? selected[0] : selected;
        onFileSelected(path as string);
      }
    } catch {
      // User cancelled
    }
  }

  async function handleBrowseFolder() {
    try {
      const selected = await open({
        multiple: false,
        directory: true,
      });
      if (selected) {
        onBatchSelected([selected as string]);
      }
    } catch {
      // User cancelled
    }
  }
</script>

<div class="welcome" class:drag-over={isDragOver}>
  <!-- Hero section -->
  <div class="hero">
    <img src={logoSvg} alt="Covername" class="logo" />
    <h1 class="brand">Covername</h1>
    <p class="tagline">Protect personal information in your documents</p>
  </div>

  <!-- Drop target -->
  <div
    class="drop-target"
    role="button"
    tabindex="0"
    onclick={handleBrowse}
    onkeydown={(e) => e.key === 'Enter' && handleBrowse()}
  >
    <svg class="drop-icon" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
      <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/>
      <polyline points="17 8 12 3 7 8"/>
      <line x1="12" y1="3" x2="12" y2="15"/>
    </svg>
    <span class="drop-label">Drop files or a folder, or <strong>click to browse</strong></span>
    <span class="drop-formats">PDF, Text, Excel, Images — single file or batch</span>
  </div>
  <button class="btn-folder" onclick={handleBrowseFolder}>
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/>
    </svg>
    Open Folder
  </button>

  <!-- Capabilities -->
  <div class="capabilities">
    <div class="capability">
      <div class="cap-icon">🔍</div>
      <div class="cap-text">
        <strong>Detect</strong>
        <span>Finds names, addresses, SSNs, accounts, and more</span>
      </div>
    </div>
    <div class="capability">
      <div class="cap-icon">🎭</div>
      <div class="cap-text">
        <strong>Replace</strong>
        <span>Swaps PII with consistent, realistic cover identities</span>
      </div>
    </div>
    <div class="capability">
      <div class="cap-icon">🔒</div>
      <div class="cap-text">
        <strong>Private</strong>
        <span>Everything stays on your computer — nothing is uploaded</span>
      </div>
    </div>
  </div>

  <!-- Quick tips -->
  <div class="tips">
    <p class="tip">
      <kbd>⌘O</kbd> Open file &nbsp;·&nbsp;
      <kbd>⚙</kbd> Settings to manage identities &nbsp;·&nbsp;
      Smart Detection available in Settings
    </p>
  </div>
</div>

<style>
  .welcome {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: var(--space-8);
    gap: var(--space-6);
    transition: background 0.2s;
  }

  .welcome.drag-over {
    background: color-mix(in srgb, var(--color-primary-500) 5%, transparent);
  }

  .hero {
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
  }

  .logo {
    width: 64px;
    height: 64px;
    margin-bottom: var(--space-2);
  }

  .brand {
    font-size: 1.75rem;
    font-weight: 700;
    color: var(--color-text);
    letter-spacing: -0.02em;
  }

  .tagline {
    font-size: var(--text-base);
    color: var(--color-text-secondary);
  }

  /* Drop target */
  .drop-target {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-6) var(--space-8);
    border: 2px dashed var(--color-border);
    border-radius: var(--radius-lg);
    cursor: pointer;
    transition: border-color 0.2s, background 0.2s;
    width: 100%;
    max-width: 480px;
  }

  .drop-target:hover,
  .drag-over .drop-target {
    border-color: var(--color-primary-500);
    background: color-mix(in srgb, var(--color-primary-500) 5%, transparent);
  }

  .drop-icon {
    color: var(--color-text-muted);
  }

  .drop-target:hover .drop-icon,
  .drag-over .drop-icon {
    color: var(--color-primary-500);
  }

  .drop-label {
    font-size: var(--text-base);
    color: var(--color-text-secondary);
  }

  .drop-label strong {
    color: var(--color-primary-600);
  }

  .drop-formats {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
  }

  /* Capabilities */
  .capabilities {
    display: flex;
    gap: var(--space-6);
    margin-top: var(--space-4);
  }

  .capability {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    max-width: 180px;
  }

  .cap-icon {
    font-size: 1.25rem;
    flex-shrink: 0;
    margin-top: 1px;
  }

  .cap-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .cap-text strong {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--color-text);
  }

  .cap-text span {
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    line-height: 1.4;
  }

  /* Tips */
  .tips {
    margin-top: var(--space-4);
  }

  .tip {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
  }

  .tip kbd {
    font-family: var(--font-sans);
    font-size: var(--text-xs);
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    padding: 1px 5px;
  }

  .btn-folder {
    display: flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-2) var(--space-4);
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }

  .btn-folder:hover {
    background: var(--color-bg-tertiary);
    border-color: var(--color-primary-500);
    color: var(--color-text);
  }
</style>
