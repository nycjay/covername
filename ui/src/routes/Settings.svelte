<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  let config = $state<Record<string, any>>({});
  let mappings = $state<any[]>([]);
  let activeTab = $state<"config" | "mappings" | "rules" | "about">("config");
  let appInfo = $state<{ version: string; git_hash: string; is_dev: boolean } | null>(null);

  onMount(async () => {
    try {
      config = await invoke<Record<string, any>>("get_config");
    } catch {
      config = {};
    }
    try {
      mappings = await invoke<any[]>("get_mappings");
    } catch {
      mappings = [];
    }
    try {
      appInfo = await invoke<{ version: string; git_hash: string; is_dev: boolean }>("get_app_info");
    } catch {
      appInfo = null;
    }
  });
</script>

<div class="settings-overlay" role="presentation">
  <button class="overlay-backdrop" onclick={onClose} aria-label="Close settings"></button>
  <div class="settings-panel" role="dialog" aria-label="Settings">
    <header class="settings-header">
      <h2>Settings</h2>
      <button class="btn-close" onclick={onClose}>✕</button>
    </header>

    <nav class="settings-tabs">
      <button class="tab" class:active={activeTab === 'config'} onclick={() => activeTab = 'config'}>
        Configuration
      </button>
      <button class="tab" class:active={activeTab === 'mappings'} onclick={() => activeTab = 'mappings'}>
        Mappings
      </button>
      <button class="tab" class:active={activeTab === 'rules'} onclick={() => activeTab = 'rules'}>
        Rules
      </button>
      <button class="tab" class:active={activeTab === 'about'} onclick={() => activeTab = 'about'}>
        About
      </button>
    </nav>

    <div class="settings-content">
      {#if activeTab === 'config'}
        <div class="section">
          <h3>Output Settings</h3>
          <div class="field">
            <label for="output-pattern">Output file pattern</label>
            <input id="output-pattern" type="text" value={config.output_pattern ?? '{name}-covered.{ext}'} readonly />
          </div>
          <div class="field">
            <label for="output-dir">Output directory</label>
            <input id="output-dir" type="text" value={config.output_directory ?? '(same as input)'} readonly />
          </div>
          <p class="hint">Edit via CLI: <code>covername config set &lt;key&gt; &lt;value&gt;</code></p>
        </div>

      {:else if activeTab === 'mappings'}
        <div class="section">
          <h3>Identity Mappings ({mappings.length})</h3>
          {#if mappings.length === 0}
            <p class="empty">No mappings yet. Process a document to create mappings.</p>
          {:else}
            <div class="mapping-list">
              {#each mappings as mapping}
                <div class="mapping-item">
                  <span class="mapping-original">█████ {mapping.original}</span>
                  <span class="mapping-arrow">→</span>
                  <span class="mapping-replacement">{mapping.replacement}</span>
                  <span class="mapping-type">{mapping.entity_type}</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>

      {:else if activeTab === 'rules'}
        <div class="section">
          <h3>Detection Rules</h3>
          <p class="hint">Built-in rules detect SSN, phone, email, credit cards, and account numbers.</p>
          <p class="hint">Add custom rules via CLI: <code>covername rules add --name "..." --pattern "..." --type ...</code></p>
        </div>

      {:else if activeTab === 'about'}
        <div class="section">
          <h3>Covername</h3>
          <p class="about-tagline">Local-first document anonymization.</p>
          {#if appInfo}
            <div class="about-info">
              <div class="info-row"><span>Version</span><span>{appInfo.version}</span></div>
              <div class="info-row"><span>Build</span><span>{appInfo.git_hash}</span></div>
              <div class="info-row"><span>Mode</span><span>{appInfo.is_dev ? 'Development' : 'Release'}</span></div>
            </div>
          {/if}
          <p class="hint">Your documents never leave this machine.</p>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .settings-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .overlay-backdrop {
    position: absolute;
    inset: 0;
    background: transparent;
    border: none;
    cursor: default;
  }

  .settings-panel {
    position: relative;
    background: var(--color-bg);
    border-radius: var(--radius-lg);
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.2);
    width: 560px;
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .settings-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--space-4) var(--space-6);
    border-bottom: 1px solid var(--color-border);
  }

  .settings-header h2 {
    font-size: var(--text-lg);
    font-weight: 600;
  }

  .btn-close {
    background: none;
    border: none;
    font-size: var(--text-lg);
    color: var(--color-text-muted);
    cursor: pointer;
    padding: var(--space-1);
    border-radius: var(--radius-sm);
  }

  .btn-close:hover {
    background: var(--color-bg-tertiary);
    color: var(--color-text);
  }

  .settings-tabs {
    display: flex;
    border-bottom: 1px solid var(--color-border);
    padding: 0 var(--space-6);
  }

  .tab {
    background: none;
    border: none;
    padding: var(--space-3) var(--space-4);
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    cursor: pointer;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    transition: color 0.15s, border-color 0.15s;
  }

  .tab:hover {
    color: var(--color-text);
  }

  .tab.active {
    color: var(--color-primary-600);
    border-bottom-color: var(--color-primary-500);
  }

  .settings-content {
    padding: var(--space-6);
    overflow-y: auto;
    flex: 1;
  }

  .section h3 {
    font-size: var(--text-base);
    font-weight: 600;
    margin-bottom: var(--space-4);
  }

  .field {
    margin-bottom: var(--space-4);
  }

  .field label {
    display: block;
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    margin-bottom: var(--space-1);
  }

  .field input {
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
    font-family: var(--font-mono);
    background: var(--color-bg-secondary);
    color: var(--color-text);
  }

  .hint {
    font-size: var(--text-sm);
    color: var(--color-text-muted);
    margin-top: var(--space-3);
  }

  .hint code {
    background: var(--color-bg-tertiary);
    padding: 1px var(--space-1);
    border-radius: 3px;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .empty {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .mapping-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-height: 300px;
    overflow-y: auto;
  }

  .mapping-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: var(--color-bg-secondary);
    border-radius: var(--radius-sm);
    font-size: var(--text-sm);
  }

  .mapping-original {
    font-family: var(--font-mono);
    color: var(--color-text);
  }

  .mapping-arrow {
    color: var(--color-text-muted);
  }

  .mapping-replacement {
    font-family: var(--font-mono);
    color: var(--color-primary-600);
  }

  .mapping-type {
    margin-left: auto;
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    text-transform: uppercase;
  }

  .about-tagline {
    color: var(--color-text-secondary);
    margin-bottom: var(--space-4);
  }

  .about-info {
    background: var(--color-bg-secondary);
    border-radius: var(--radius-md);
    padding: var(--space-3);
    margin-bottom: var(--space-4);
  }

  .info-row {
    display: flex;
    justify-content: space-between;
    padding: var(--space-1) 0;
    font-size: var(--text-sm);
  }

  .info-row span:first-child {
    color: var(--color-text-secondary);
  }

  .info-row span:last-child {
    font-family: var(--font-mono);
  }
</style>
