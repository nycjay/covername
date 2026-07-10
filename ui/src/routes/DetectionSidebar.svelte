<script lang="ts">
  interface Detection {
    matched_text: string;
    entity_type: string;
    replacement: string;
    start: number;
    end: number;
    context: string;
    accepted: boolean | null;
  }

  interface Props {
    detections: Detection[];
    generating?: boolean;
    outputReady?: boolean;
    onAccept: (index: number) => void;
    onReject: (index: number) => void;
    onEdit: (index: number, replacement: string) => void;
    onAcceptAll: () => void;
    onGenerate: () => void;
    onReveal?: () => void;
  }

  let { detections, generating = false, outputReady = false, onAccept, onReject, onEdit, onAcceptAll, onGenerate, onReveal }: Props = $props();
  let editingIndex = $state<number | null>(null);
  let editValue = $state("");

  function startEdit(index: number) {
    editingIndex = index;
    editValue = detections[index].replacement;
  }

  function confirmEdit(index: number) {
    onEdit(index, editValue);
    editingIndex = null;
  }

  function cancelEdit() {
    editingIndex = null;
  }

  const pendingCount = $derived(detections.filter((d) => d.accepted === null).length);
  const acceptedCount = $derived(detections.filter((d) => d.accepted === true).length);
  const rejectedCount = $derived(detections.filter((d) => d.accepted === false).length);
</script>

<aside class="sidebar">
  <div class="sidebar-header">
    <h3 class="sidebar-title">Detections</h3>
    <div class="sidebar-stats">
      <span class="stat">{detections.length} found</span>
      {#if acceptedCount > 0}
        <span class="stat stat-accepted">✓ {acceptedCount}</span>
      {/if}
      {#if rejectedCount > 0}
        <span class="stat stat-rejected">✗ {rejectedCount}</span>
      {/if}
    </div>
  </div>

  <div class="detection-list">
    {#each detections as detection, i}
      <div
        class="detection-item"
        class:pending={detection.accepted === null}
        class:accepted={detection.accepted === true}
        class:rejected={detection.accepted === false}
      >
        <div class="detection-header">
          <span class="entity-badge">{detection.entity_type}</span>
        </div>

        <div class="detection-original">
          <span class="redaction-preview">█████</span>
          {detection.matched_text}
        </div>

        <div class="detection-replacement">
          → {detection.replacement}
        </div>

        {#if editingIndex === i}
          <div class="edit-row">
            <input
              class="edit-input"
              type="text"
              bind:value={editValue}
              onkeydown={(e) => e.key === 'Enter' && confirmEdit(i)}
            />
            <button class="btn-sm btn-accept" onclick={() => confirmEdit(i)}>✓</button>
            <button class="btn-sm btn-cancel" onclick={cancelEdit}>✗</button>
          </div>
        {:else}
          <div class="action-row">
            <button
              class="btn-sm btn-accept"
              onclick={() => onAccept(i)}
              disabled={detection.accepted === true}
            >✓ Accept</button>
            <button
              class="btn-sm btn-reject"
              onclick={() => onReject(i)}
              disabled={detection.accepted === false}
            >✗ Reject</button>
            <button class="btn-sm btn-edit" onclick={() => startEdit(i)}>✎ Edit</button>
          </div>
        {/if}
      </div>
    {/each}
  </div>

  <div class="sidebar-actions">
    {#if pendingCount > 0}
      <button class="btn-primary" onclick={onAcceptAll}>
        Accept All ({pendingCount} pending)
      </button>
    {/if}
    {#if acceptedCount > 0}
      <button class="btn-generate" onclick={onGenerate} disabled={generating}>
        {#if generating}
          Generating…
        {:else}
          Generate Output ({acceptedCount} replacements)
        {/if}
      </button>
    {/if}
    {#if outputReady && onReveal}
      <button class="btn-reveal" onclick={onReveal}>
        Reveal in Finder
      </button>
    {/if}
  </div>
</aside>

<style>
  .sidebar {
    flex: 1;
    max-width: 380px;
    min-width: 300px;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--color-border);
    background: var(--color-bg-secondary);
  }

  .sidebar-header {
    padding: var(--space-4) var(--space-4);
    border-bottom: 1px solid var(--color-border);
  }

  .sidebar-title {
    font-size: var(--text-lg);
    font-weight: 600;
    margin-bottom: var(--space-1);
  }

  .sidebar-stats {
    display: flex;
    gap: var(--space-3);
    font-size: var(--text-xs);
    color: var(--color-text-muted);
  }

  .stat-accepted { color: var(--color-success); }
  .stat-rejected { color: var(--color-error); }

  .detection-list {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .detection-item {
    padding: var(--space-3);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    transition: border-color 0.15s;
  }

  .detection-item.accepted {
    border-color: var(--color-success);
    background: color-mix(in srgb, var(--color-success) 5%, var(--color-bg));
  }

  .detection-item.rejected {
    border-color: var(--color-error);
    opacity: 0.6;
  }

  .detection-header {
    margin-bottom: var(--space-2);
  }

  .entity-badge {
    font-size: var(--text-xs);
    font-weight: 600;
    text-transform: uppercase;
    color: var(--color-primary-600);
    background: color-mix(in srgb, var(--color-primary-500) 10%, transparent);
    padding: 2px var(--space-2);
    border-radius: var(--radius-sm);
  }

  .detection-original {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    color: var(--color-text);
    margin-bottom: var(--space-1);
  }

  .redaction-preview {
    color: var(--color-redaction);
    margin-right: var(--space-1);
  }

  .detection-replacement {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    margin-bottom: var(--space-2);
  }

  .action-row, .edit-row {
    display: flex;
    gap: var(--space-2);
  }

  .btn-sm {
    font-size: var(--text-xs);
    padding: 2px var(--space-2);
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-border);
    background: transparent;
    cursor: pointer;
    transition: background 0.1s;
  }

  .btn-sm:hover { background: var(--color-bg-tertiary); }
  .btn-sm:disabled { opacity: 0.4; cursor: default; }
  .btn-sm.btn-accept:hover { background: var(--color-highlight-accepted); }
  .btn-sm.btn-reject:hover { background: var(--color-highlight-rejected); }

  .edit-input {
    flex: 1;
    font-size: var(--text-sm);
    padding: 2px var(--space-2);
    border: 1px solid var(--color-primary-500);
    border-radius: var(--radius-sm);
    outline: none;
    font-family: var(--font-mono);
  }

  .sidebar-actions {
    padding: var(--space-4);
    border-top: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .btn-primary {
    width: 100%;
    padding: var(--space-2) var(--space-4);
    background: var(--color-primary-600);
    color: white;
    border: none;
    border-radius: var(--radius-md);
    font-weight: 500;
    font-size: var(--text-sm);
    cursor: pointer;
    transition: background 0.1s;
  }

  .btn-primary:hover { background: var(--color-primary-700); }

  .btn-generate {
    width: 100%;
    padding: var(--space-2) var(--space-4);
    background: var(--color-redaction);
    color: var(--color-redaction-text);
    border: none;
    border-radius: var(--radius-md);
    font-weight: 600;
    font-size: var(--text-sm);
    cursor: pointer;
    transition: background 0.1s;
  }

  .btn-generate:hover { background: var(--color-neutral-800); }

  .btn-reveal {
    width: 100%;
    padding: var(--space-2) var(--space-4);
    background: var(--color-bg-tertiary);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
    cursor: pointer;
    margin-top: var(--space-2);
    transition: background 0.1s;
  }

  .btn-reveal:hover {
    background: var(--color-primary-500);
    color: white;
    border-color: var(--color-primary-500);
  }
</style>
