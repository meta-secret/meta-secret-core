import { describe, expect, it, vi } from 'vitest';
import { createSecretController } from '@/features/liveboard/secretController';
import { deferred, secret, secretPort, session } from './liveboardFixtures';

function setup() {
  const clipboard = deferred<void>();
  const port = secretPort({ copy: vi.fn(() => clipboard.promise) });
  const controller = createSecretController(port);
  controller.reconcile({ session: session(), secrets: [secret()] });
  controller.present({ active: true, visibleIds: ['pass-a'], query: '', scrollTop: 0 });
  return { controller, port, clipboard };
}

describe('Liveboard canonical lifecycle source preflight', () => {
  it('restores an actionable card after Core clears a declined canonical deletion request', async () => {
    const { controller, port } = setup();
    await controller.remove('pass-a');
    expect(port.deleteSecret).toHaveBeenCalledExactlyOnceWith('pass-a');
    controller.reconcile({
      session: session(),
      secrets: [secret('pass-a', { deletionRequestId: 'exact-delete', canDelete: false })],
    });
    expect(controller.state.value.cards['pass-a']?.phase).toBe('deleting');
    controller.reconcile({ session: session(), secrets: [secret('pass-a', { canDelete: true })] });
    expect(controller.state.value.cards['pass-a']?.phase).toBe('masked');
    await controller.primary('pass-a');
    expect(port.showLocal).toHaveBeenCalledExactlyOnceWith('pass-a');
    controller.dispose();
  });

  it('clears the removed card object held by an outstanding Copy when a canonical tombstone arrives', async () => {
    const { controller, port, clipboard } = setup();
    await controller.primary('pass-a');
    const retainedCard = controller.state.value.cards['pass-a']!;
    expect(retainedCard.value).toEqual({ kind: 'text', text: 'private value' });
    const copying = controller.copy('pass-a');
    expect(port.copy).toHaveBeenCalledExactlyOnceWith('private value');
    try {
      controller.reconcile({ session: session(), secrets: [] });
      expect(controller.state.value.cards['pass-a']).toBeUndefined();
      expect(retainedCard.value).toBeNull();
      expect(retainedCard.copied).toBe(false);
    } finally {
      clipboard.resolve();
      await copying;
      controller.dispose();
    }
  });
});
