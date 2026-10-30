<script lang="ts">
  import { app } from "../lib/state.svelte";
  import Icon from "./Icon.svelte";
</script>

<div class="toasts" aria-live="polite">
  {#each app.toasts as toast (toast.id)}
    <div class="toast glass {toast.kind}">
      {#if toast.kind === "error"}
        <Icon name="warning" size={16} />
      {:else if toast.kind === "success"}
        <Icon name="check" size={16} stroke={2.2} />
      {/if}
      <span class="text">{toast.text}</span>
      {#if toast.action}
        <button
          class="act"
          onclick={() => {
            toast.action!.run();
            app.dismissToast(toast.id);
          }}>{toast.action.label}</button
        >
      {/if}
      <button class="x" aria-label="Dismiss" onclick={() => app.dismissToast(toast.id)}>
        <Icon name="xmark" size={11} stroke={2.2} />
      </button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    z-index: 100;
    left: 50%;
    bottom: 22px;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    pointer-events: none;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 9px;
    max-width: min(560px, calc(100vw - 40px));
    padding: 9px 10px 9px 16px;
    border-radius: var(--r-pill);
    background: var(--glass-strong);
    box-shadow: var(--shadow-lg);
    font-size: 13px;
    pointer-events: auto;
    animation: toast-in 380ms var(--ease-out);
  }
  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(14px) scale(0.96);
    }
  }
  .error > :global(svg) {
    color: var(--red);
  }
  .success > :global(svg) {
    color: var(--green);
  }
  .text {
    line-height: 1.35;
  }
  .act {
    border: 0;
    padding: 4px 10px;
    border-radius: var(--r-pill);
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    font-size: 12px;
    white-space: nowrap;
  }
  .x {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--fill-2);
    color: var(--label-2);
    flex: none;
  }
</style>
