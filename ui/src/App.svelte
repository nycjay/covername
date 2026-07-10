<script lang="ts">
  import "./app.css";
  import { open } from "@tauri-apps/plugin-dialog";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { check } from "@tauri-apps/plugin-updater";
  import { onMount } from "svelte";
  import { FILE_FILTER } from "./lib/constants";
  import DropZone from "./routes/DropZone.svelte";
  import DocumentView from "./routes/DocumentView.svelte";
  import BatchView from "./routes/BatchView.svelte";
  import Settings from "./routes/Settings.svelte";
  import Onboarding from "./routes/Onboarding.svelte";
  import ProgressBar from "./routes/ProgressBar.svelte";
  import Toast from "./routes/Toast.svelte";
  import logoSvg from "./assets/logo.svg";

  interface ProgressEvent {
    phase: string;
    current: number;
    total: number;
    message: string;
  }

  let currentFile: string | null = $state(null);
  let batchPaths: string[] | null = $state(null);
  let showSettings = $state(false);
  let showAbout = $state(false);
  let showOnboarding = $state(false);

  // Global progress state
  let progress = $state<ProgressEvent | null>(null);

  // Global toast state
  let toastVisible = $state(false);
  let toastMessage = $state("");
  let toastType = $state<"success" | "error" | "info">("success");

  // Update state
  let updateAvailable = $state(false);
  let updateVersion = $state("");
  let updateInstalling = $state(false);

  // Uninstall state
  let showUninstallConfirm = $state(false);
  let uninstalling = $state(false);

  // App version (fetched from backend)
  let appVersion = $state("0.1.0");

  async function checkForUpdates(silent = true) {
    try {
      const update = await check();
      if (update) {
        updateAvailable = true;
        updateVersion = update.version;
        if (!silent) {
          toastMessage = `Version ${update.version} is available`;
          toastType = "info";
          toastVisible = true;
        }
      } else if (!silent) {
        toastMessage = "You're on the latest version";
        toastType = "info";
        toastVisible = true;
      }
    } catch {
      if (!silent) {
        toastMessage = "Could not check for updates";
        toastType = "error";
        toastVisible = true;
      }
    }
  }

  async function installUpdate() {
    updateInstalling = true;
    try {
      const update = await check();
      if (update) {
        await update.downloadAndInstall();
        // App will restart automatically
      }
    } catch (e) {
      updateInstalling = false;
      toastMessage = `Update failed: ${e}`;
      toastType = "error";
      toastVisible = true;
    }
  }

  // Listen for progress events from backend
  onMount(() => {
    const unlistenProgress = listen<ProgressEvent>("progress", (event) => {
      const payload = event.payload;

      if (payload.phase === "complete") {
        progress = null;
        toastMessage = payload.message;
        toastType = "success";
        toastVisible = true;
      } else if (payload.phase === "generate" || payload.phase === "batch") {
        progress = payload;
      }
      // "scan" phase is handled by DocumentView's own loading spinner
    });

    // Listen for native menu events
    const unlistenMenu = listen<string>("menu-event", (event) => {
      switch (event.payload) {
        case "about":
          showAbout = true;
          break;
        case "check_update":
          checkForUpdates(false);
          break;
        case "uninstall":
          showAbout = true;
          showUninstallConfirm = true;
          break;
        case "open":
          handleOpen();
          break;
        case "debug_logs":
          gatherDebugLogs();
          break;
      }
    });

    // Check for updates silently on launch
    checkForUpdates(true);

    // Fetch app version
    invoke<{ version: string }>("get_app_info").then((info) => {
      appVersion = info.version;
    });

    // Check if first run
    invoke<boolean>("is_first_run").then((first) => {
      showOnboarding = first;
    });

    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenMenu.then((fn) => fn());
    };
  });

  function handleFileSelected(path: string) {
    currentFile = path;
    batchPaths = null;
  }

  function handleBatchSelected(paths: string[]) {
    batchPaths = paths;
    currentFile = null;
  }

  function handleClose() {
    currentFile = null;
    batchPaths = null;
    progress = null;
  }

  async function handleOpen() {
    try {
      const selected = await open({
        multiple: false,
        filters: [FILE_FILTER],
      });
      if (selected) {
        currentFile = selected as string;
      }
    } catch {
      // User cancelled
    }
  }

  async function performUninstall(removeModels: boolean) {
    uninstalling = true;
    try {
      const result = await invoke<string>("uninstall", { removeModels });
      showAbout = false;
      showUninstallConfirm = false;
      toastMessage = "Covername has been uninstalled. You can close this window.";
      toastType = "success";
      toastVisible = true;
    } catch (e) {
      toastMessage = `Uninstall failed: ${e}`;
      toastType = "error";
      toastVisible = true;
    } finally {
      uninstalling = false;
    }
  }

  async function gatherDebugLogs() {
    try {
      const path = await invoke<string>("gather_debug_logs");
      toastMessage = `Debug logs saved to ${path.split('/').pop()}`;
      toastType = "success";
      toastVisible = true;
      // Reveal the zip in Finder
      await invoke("reveal_in_finder", { path });
    } catch (e) {
      toastMessage = `Failed to gather logs: ${e}`;
      toastType = "error";
      toastVisible = true;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.metaKey || event.ctrlKey) {
      if (event.key === "o") {
        event.preventDefault();
        handleOpen();
      } else if (event.key === ",") {
        event.preventDefault();
        showSettings = true;
      }
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<main class="app-shell">
  <header class="toolbar">
    <div class="toolbar-left">
      <button class="btn-toolbar" onclick={handleOpen} title="Open file (⌘O)">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/>
        </svg>
        Open
      </button>
      {#if currentFile}
        <button class="btn-toolbar" onclick={handleClose} title="Close file">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
          Close
        </button>
      {/if}
    </div>
    <div class="toolbar-right">
      <button class="btn-toolbar" onclick={() => showAbout = true} title="About Covername">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 015.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/>
        </svg>
        Help
      </button>
      <button class="btn-toolbar" onclick={() => showSettings = true} title="Settings (⌘,)">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09a1.65 1.65 0 00-1-1.51 1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06a1.65 1.65 0 00.33-1.82 1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09a1.65 1.65 0 001.51-1 1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06a1.65 1.65 0 001.82.33H9a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06a1.65 1.65 0 00-.33 1.82V9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z"/>
        </svg>
        Settings
      </button>
    </div>
  </header>

  {#if updateAvailable && !updateInstalling}
    <div class="update-banner">
      <span>Version {updateVersion} is available</span>
      <button class="btn-update" onclick={installUpdate}>Update now</button>
      <button class="btn-dismiss-update" onclick={() => updateAvailable = false} aria-label="Dismiss">×</button>
    </div>
  {/if}

  {#if updateInstalling}
    <ProgressBar phase="generate" current={0} total={0} message="Installing update…" />
  {:else if progress}
    <ProgressBar
      phase={progress.phase}
      current={progress.current}
      total={progress.total}
      message={progress.message}
    />
  {/if}

  <div class="content">
    {#if currentFile}
      <DocumentView filePath={currentFile} onClose={handleClose} />
    {:else if batchPaths}
      <BatchView paths={batchPaths} onClose={handleClose} />
    {:else}
      <DropZone onFileSelected={handleFileSelected} onBatchSelected={handleBatchSelected} />
    {/if}
  </div>

  {#if showSettings}
    <Settings onClose={() => showSettings = false} />
  {/if}

  {#if showAbout}
    <div class="modal-backdrop">
      <button class="backdrop-dismiss" onclick={() => showAbout = false} aria-label="Close"></button>
      <div class="about-panel" role="dialog" aria-labelledby="about-title">
        <img src={logoSvg} alt="" class="about-logo" />
        <h2 id="about-title">Covername</h2>
        <p class="about-version">Version {appVersion}</p>
        <p class="about-desc">
          A local-first document anonymization tool. Detects personal information
          in your documents and replaces it with consistent cover identities.
        </p>
        <div class="about-features">
          <h3>How it works</h3>
          <ol>
            <li><strong>Open</strong> a document (PDF, text, spreadsheet, or image)</li>
            <li><strong>Review</strong> detected PII — accept, reject, or edit replacements</li>
            <li><strong>Export</strong> a clean copy with all PII replaced</li>
          </ol>
        </div>
        <div class="about-features">
          <h3>What it detects</h3>
          <ul>
            <li>Names and addresses</li>
            <li>Social Security numbers</li>
            <li>Account and routing numbers</li>
            <li>Dates of birth, phone numbers, emails</li>
            <li>Driver's license, passport, Medicare IDs</li>
          </ul>
        </div>
        <div class="about-features">
          <h3>Keyboard shortcuts</h3>
          <div class="shortcuts">
            <span><kbd>⌘O</kbd> Open file</span>
            <span><kbd>⌘,</kbd> Settings</span>
          </div>
        </div>
        <button class="btn-check-update" onclick={() => checkForUpdates(false)}>Check for Updates</button>
        <button class="btn-close-about" onclick={() => showAbout = false}>Close</button>

        {#if !showUninstallConfirm}
          <button class="btn-uninstall" onclick={() => showUninstallConfirm = true}>Uninstall Covername…</button>
        {:else}
          <div class="uninstall-confirm">
            <p class="uninstall-warning">This will remove Covername and its data from your computer.</p>
            <div class="uninstall-options">
              <button class="btn-uninstall-action" onclick={() => performUninstall(false)} disabled={uninstalling}>
                {uninstalling ? "Removing…" : "Keep models, remove app + config"}
              </button>
              <button class="btn-uninstall-action btn-uninstall-all" onclick={() => performUninstall(true)} disabled={uninstalling}>
                {uninstalling ? "Removing…" : "Remove everything (~1 GB)"}
              </button>
              <button class="btn-uninstall-cancel" onclick={() => showUninstallConfirm = false}>Cancel</button>
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}

  <footer class="status-bar">
    {#if currentFile}
      <span class="status-file">{currentFile.split('/').pop()}</span>
    {:else if batchPaths}
      <span class="status-file">Batch mode</span>
    {:else}
      <span>Ready</span>
    {/if}
    <span class="status-privacy">All processing is local</span>
  </footer>

  <Toast message={toastMessage} type={toastType} visible={toastVisible} onDismiss={() => toastVisible = false} />

  {#if showOnboarding}
    <Onboarding onComplete={() => showOnboarding = false} />
  {/if}
</main>

<style>
  .app-shell {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-secondary);
    -webkit-app-region: drag;
  }

  .toolbar-left, .toolbar-right {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    -webkit-app-region: no-drag;
  }

  .toolbar-logo {
    width: 22px;
    height: 22px;
  }

  .app-title {
    font-weight: 700;
    font-size: var(--text-lg);
    color: var(--color-text);
    letter-spacing: -0.01em;
  }

  .toolbar-separator {
    width: 1px;
    height: 18px;
    background: var(--color-border);
    margin: 0 var(--space-2);
  }

  .btn-toolbar {
    display: flex;
    align-items: center;
    gap: 5px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    padding: var(--space-1) var(--space-3);
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: background 0.1s, border-color 0.1s, color 0.1s;
  }

  .btn-toolbar:hover {
    background: var(--color-bg-tertiary);
    border-color: var(--color-border);
    color: var(--color-text);
  }

  .content {
    flex: 1;
    overflow: hidden;
    display: flex;
  }

  .status-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--space-1) var(--space-4);
    border-top: 1px solid var(--color-border);
    background: var(--color-bg-secondary);
    font-size: var(--text-xs);
    color: var(--color-text-muted);
  }

  .status-file {
    color: var(--color-text-secondary);
    font-weight: 500;
  }

  .status-privacy {
    opacity: 0.7;
  }

  /* About / Help modal */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .backdrop-dismiss {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    background: transparent;
    border: none;
    cursor: default;
  }

  .about-panel {
    position: relative;
    background: var(--color-bg);
    border-radius: var(--radius-lg);
    padding: var(--space-8);
    max-width: 420px;
    width: 90%;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.2);
    text-align: center;
    max-height: 85vh;
    overflow-y: auto;
  }

  .about-logo {
    width: 56px;
    height: 56px;
    margin-bottom: var(--space-3);
  }

  .about-panel h2 {
    font-size: var(--text-xl);
    font-weight: 700;
    margin-bottom: var(--space-1);
  }

  .about-version {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    margin-bottom: var(--space-4);
  }

  .about-desc {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    line-height: 1.6;
    margin-bottom: var(--space-6);
  }

  .about-features {
    text-align: left;
    margin-bottom: var(--space-4);
  }

  .about-features h3 {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: var(--space-2);
  }

  .about-features ol,
  .about-features ul {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    padding-left: var(--space-6);
    line-height: 1.8;
  }

  .about-features li strong {
    color: var(--color-primary-600);
  }

  .shortcuts {
    display: flex;
    gap: var(--space-4);
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }

  .shortcuts kbd {
    font-family: var(--font-sans);
    font-size: var(--text-xs);
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    padding: 1px 5px;
    margin-right: 4px;
  }

  .btn-close-about {
    margin-top: var(--space-6);
    background: var(--color-primary-500);
    color: white;
    border: none;
    border-radius: var(--radius-md);
    padding: var(--space-2) var(--space-6);
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-close-about:hover {
    background: var(--color-primary-600);
  }

  /* Update banner */
  .update-banner {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    background: #065f46;
    color: #d1fae5;
    font-size: var(--text-sm);
  }

  .btn-update {
    background: #10b981;
    color: white;
    border: none;
    border-radius: var(--radius-sm);
    padding: var(--space-1) var(--space-3);
    font-size: var(--text-xs);
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-update:hover {
    background: #059669;
  }

  .btn-dismiss-update {
    margin-left: auto;
    background: none;
    border: none;
    color: #d1fae5;
    opacity: 0.7;
    cursor: pointer;
    font-size: 1.1rem;
    padding: 0 var(--space-1);
  }

  .btn-dismiss-update:hover {
    opacity: 1;
  }

  .btn-check-update {
    background: transparent;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-2) var(--space-4);
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    cursor: pointer;
    margin-top: var(--space-4);
    transition: background 0.15s, border-color 0.15s;
  }

  .btn-check-update:hover {
    background: var(--color-bg-tertiary);
    border-color: var(--color-primary-500);
    color: var(--color-text);
  }

  /* Uninstall */
  .btn-uninstall {
    background: transparent;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-2) var(--space-4);
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    cursor: pointer;
    margin-top: var(--space-2);
    transition: background 0.15s, border-color 0.15s;
  }

  .btn-uninstall:hover {
    background: var(--color-bg-tertiary);
    border-color: var(--color-error);
    color: var(--color-error);
  }

  .uninstall-confirm {
    margin-top: var(--space-4);
    padding: var(--space-4);
    border: 1px solid var(--color-error);
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--color-error) 5%, transparent);
  }

  .uninstall-warning {
    font-size: var(--text-sm);
    color: var(--color-error);
    margin-bottom: var(--space-3);
    text-align: center;
  }

  .uninstall-options {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .btn-uninstall-action {
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-2) var(--space-3);
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-uninstall-action:hover:not(:disabled) {
    background: var(--color-error);
    color: white;
    border-color: var(--color-error);
  }

  .btn-uninstall-action:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-uninstall-all {
    color: var(--color-error);
  }

  .btn-uninstall-cancel {
    background: transparent;
    border: none;
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    cursor: pointer;
    padding: var(--space-1);
  }

  .btn-uninstall-cancel:hover {
    color: var(--color-text);
  }
</style>
