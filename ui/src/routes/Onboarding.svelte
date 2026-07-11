<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import logoSvg from "../assets/logo.svg";

  interface Props {
    onComplete: () => void;
  }

  let { onComplete }: Props = $props();
  let step = $state(0);
  let modelDownloading = $state(false);
  let modelInstalled = $state(false);

  const steps = [
    { id: "welcome" },
    { id: "how-it-works" },
    { id: "privacy" },
    { id: "model" },
    { id: "ready" },
  ];

  async function finish() {
    await invoke("complete_onboarding");
    onComplete();
  }

  function next() {
    if (step < steps.length - 1) {
      step++;
    } else {
      finish();
    }
  }

  function skip() {
    finish();
  }

  async function downloadModel() {
    modelDownloading = true;
    try {
      await invoke("download_model");
      modelInstalled = true;
    } catch {
      // Will still work without model
    } finally {
      modelDownloading = false;
    }
  }
</script>

<div class="onboarding">
  <div class="wizard">
    {#if steps[step].id === "welcome"}
      <img src={logoSvg} alt="" class="wizard-logo" />
      <h1>Welcome to Covername</h1>
      <p class="wizard-desc">
        Protect personal information in your documents — entirely on your computer.
      </p>

    {:else if steps[step].id === "how-it-works"}
      <h2>How it works</h2>
      <div class="steps-list">
        <div class="step-item">
          <span class="step-num">1</span>
          <div>
            <strong>Open a document</strong>
            <p>Drop a file or folder — PDFs, text, spreadsheets, images.</p>
          </div>
        </div>
        <div class="step-item">
          <span class="step-num">2</span>
          <div>
            <strong>Review detections</strong>
            <p>Covername finds names, addresses, SSNs, and more. Accept or reject each one.</p>
          </div>
        </div>
        <div class="step-item">
          <span class="step-num">3</span>
          <div>
            <strong>Get a clean copy</strong>
            <p>A new file is generated with all PII replaced by consistent cover identities.</p>
          </div>
        </div>
      </div>

    {:else if steps[step].id === "privacy"}
      <h2>Your privacy</h2>
      <div class="privacy-points">
        <div class="privacy-item">
          <span class="privacy-icon">•</span>
          <p><strong>100% local.</strong> Your documents never leave this computer. Nothing is uploaded anywhere.</p>
        </div>
        <div class="privacy-item">
          <span class="privacy-icon">•</span>
          <p><strong>Data stored at:</strong> <code>~/.config/covername/</code> — config, mappings, and optional AI models.</p>
        </div>
        <div class="privacy-item">
          <span class="privacy-icon">•</span>
          <p><strong>Easy to remove.</strong> Help → Uninstall removes everything cleanly.</p>
        </div>
      </div>

    {:else if steps[step].id === "model"}
      <h2>Enhanced Detection (Optional)</h2>
      <p class="wizard-desc">
        Download a small AI model to significantly improve PII detection accuracy.
      </p>
      <div class="model-info">
        <div class="model-detail"><strong>Size:</strong> ~262 MB (one-time download)</div>
        <div class="model-detail"><strong>Benefit:</strong> 96% accuracy, detects names, dates, and 50+ entity types</div>
        <div class="model-detail"><strong>Without it:</strong> Regex patterns still catch SSN, phone, email, and addresses</div>
      </div>
      {#if modelInstalled}
        <p class="model-done">Model installed.</p>
      {:else}
        <button class="btn-download-onboarding" onclick={downloadModel} disabled={modelDownloading}>
          {modelDownloading ? "Downloading..." : "Download AI Model"}
        </button>
      {/if}

    {:else if steps[step].id === "ready"}
      <h2>You're all set!</h2>
      <p class="wizard-desc">
        Drop a file to get started, or open a folder to process multiple documents at once.
      </p>
      <div class="tips-box">
        <p><kbd>⌘O</kbd> Open a file</p>
        <p><kbd>⌘,</kbd> Settings</p>
        <p>Help menu → Check for Updates</p>
      </div>
    {/if}

    <div class="wizard-footer">
      <div class="dots">
        {#each steps as _, i}
          <span class="dot" class:active={i === step}></span>
        {/each}
      </div>
      <div class="wizard-buttons">
        {#if step < steps.length - 1}
          <button class="btn-skip" onclick={skip}>Skip</button>
          <button class="btn-next" onclick={next}>Next</button>
        {:else}
          <button class="btn-next" onclick={finish}>Get Started</button>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .onboarding {
    position: fixed;
    inset: 0;
    background: var(--color-bg);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 300;
  }

  .wizard {
    max-width: 480px;
    width: 90%;
    text-align: center;
    padding: var(--space-8);
  }

  .wizard-logo {
    width: 72px;
    height: 72px;
    margin-bottom: var(--space-4);
  }

  .wizard h1 {
    font-size: 1.75rem;
    font-weight: 700;
    margin-bottom: var(--space-3);
  }

  .wizard h2 {
    font-size: 1.25rem;
    font-weight: 600;
    margin-bottom: var(--space-4);
  }

  .wizard-desc {
    font-size: var(--text-base);
    color: var(--color-text-secondary);
    line-height: 1.6;
  }

  /* Steps list */
  .steps-list {
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .step-item {
    display: flex;
    gap: var(--space-3);
    align-items: flex-start;
  }

  .step-num {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: var(--color-primary-500);
    color: white;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: var(--text-sm);
    font-weight: 600;
    flex-shrink: 0;
  }

  .step-item strong {
    font-size: var(--text-sm);
    display: block;
    margin-bottom: 2px;
  }

  .step-item p {
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    margin: 0;
  }

  /* Privacy */
  .privacy-points {
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .privacy-item {
    display: flex;
    gap: var(--space-3);
    align-items: flex-start;
  }

  .privacy-icon {
    font-size: 1.25rem;
    flex-shrink: 0;
  }

  .privacy-item p {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    margin: 0;
    line-height: 1.5;
  }

  .privacy-item code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    background: var(--color-bg-tertiary);
    padding: 1px 4px;
    border-radius: 3px;
  }

  /* Tips */
  .tips-box {
    margin-top: var(--space-4);
    text-align: left;
    background: var(--color-bg-secondary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-3) var(--space-4);
  }

  .tips-box p {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    margin: var(--space-1) 0;
  }

  .tips-box kbd {
    font-family: var(--font-sans);
    font-size: var(--text-xs);
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    padding: 1px 5px;
    margin-right: 4px;
  }

  /* Footer */
  .wizard-footer {
    margin-top: var(--space-8);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-4);
  }

  .dots {
    display: flex;
    gap: var(--space-2);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--color-border);
    transition: background 0.2s;
  }

  .dot.active {
    background: var(--color-primary-500);
  }

  .wizard-buttons {
    display: flex;
    gap: var(--space-3);
  }

  .btn-next {
    background: var(--color-primary-500);
    color: white;
    border: none;
    border-radius: var(--radius-md);
    padding: var(--space-2) var(--space-6);
    font-size: var(--text-sm);
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-next:hover {
    background: var(--color-primary-600);
  }

  .model-info {
    text-align: left;
    margin: var(--space-4) 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .model-detail {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }

  .model-done {
    color: var(--color-success);
    font-weight: 500;
    margin-top: var(--space-4);
  }

  .btn-download-onboarding {
    background: var(--color-primary-500);
    color: white;
    border: none;
    border-radius: var(--radius-md);
    padding: var(--space-2) var(--space-6);
    font-size: var(--text-sm);
    font-weight: 600;
    cursor: pointer;
    margin-top: var(--space-4);
    transition: background 0.15s;
  }

  .btn-download-onboarding:hover:not(:disabled) {
    background: var(--color-primary-600);
  }

  .btn-download-onboarding:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .btn-skip {
    background: transparent;
    border: none;
    padding: var(--space-2) var(--space-4);
    font-size: var(--text-sm);
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .btn-skip:hover {
    color: var(--color-text-secondary);
  }
</style>
