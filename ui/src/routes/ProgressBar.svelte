<script lang="ts">
  interface Props {
    phase: string;
    current: number;
    total: number;
    message: string;
  }

  let { phase, current, total, message }: Props = $props();

  let percent = $derived(total > 0 ? Math.round((current / total) * 100) : 0);
  let indeterminate = $derived(total === 0);
</script>

<div class="progress-container" role="progressbar" aria-valuenow={indeterminate ? undefined : percent} aria-valuemin={0} aria-valuemax={100}>
  <div class="progress-header">
    <span class="progress-label">
      {message}
    </span>
    {#if !indeterminate}
      <span class="progress-percent">{percent}%</span>
    {/if}
  </div>
  <div class="progress-track">
    {#if indeterminate}
      <div class="progress-bar indeterminate"></div>
    {:else}
      <div class="progress-bar" style="width: {percent}%"></div>
    {/if}
  </div>
</div>

<style>
  .progress-container {
    padding: var(--space-4) var(--space-6);
    background: var(--color-bg-secondary);
    border-bottom: 1px solid var(--color-border);
  }

  .progress-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--space-2);
  }

  .progress-label {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }

  .progress-percent {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .progress-track {
    height: 4px;
    background: var(--color-bg-tertiary);
    border-radius: 2px;
    overflow: hidden;
  }

  .progress-bar {
    height: 100%;
    background: var(--color-primary-500);
    border-radius: 2px;
    transition: width 0.3s ease;
  }

  .progress-bar.indeterminate {
    width: 40%;
    animation: indeterminate 1.5s ease-in-out infinite;
  }

  @keyframes indeterminate {
    0% { transform: translateX(-100%); }
    100% { transform: translateX(350%); }
  }
</style>
