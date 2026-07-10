<script lang="ts">
  interface Props {
    message: string;
    type?: "success" | "error" | "info";
    visible: boolean;
    onDismiss: () => void;
  }

  let { message, type = "success", visible, onDismiss }: Props = $props();

  // Auto-dismiss after 5 seconds
  $effect(() => {
    if (visible) {
      const timer = setTimeout(onDismiss, 5000);
      return () => clearTimeout(timer);
    }
  });
</script>

{#if visible}
  <div class="toast toast-{type}" role="alert">
    <span class="toast-icon">
      {#if type === "success"}✓{:else if type === "error"}✗{:else}i{/if}
    </span>
    <span class="toast-message">{message}</span>
    <button class="toast-dismiss" onclick={onDismiss} aria-label="Dismiss">×</button>
  </div>
{/if}

<style>
  .toast {
    position: fixed;
    bottom: var(--space-8);
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.15);
    z-index: 200;
    animation: toast-in 0.3s ease-out;
    max-width: 90%;
  }

  .toast-success {
    background: #065f46;
    color: #d1fae5;
  }

  .toast-error {
    background: #991b1b;
    color: #fecaca;
  }

  .toast-info {
    background: var(--color-neutral-800);
    color: var(--color-neutral-100);
  }

  .toast-icon {
    flex-shrink: 0;
  }

  .toast-message {
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .toast-dismiss {
    background: none;
    border: none;
    color: inherit;
    opacity: 0.7;
    cursor: pointer;
    font-size: 1.2rem;
    padding: 0 var(--space-1);
    line-height: 1;
  }

  .toast-dismiss:hover {
    opacity: 1;
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateX(-50%) translateY(10px);
    }
    to {
      opacity: 1;
      transform: translateX(-50%) translateY(0);
    }
  }
</style>
