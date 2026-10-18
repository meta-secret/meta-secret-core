import { createHash } from 'node:crypto';

export const platforms = ['web', 'ios', 'android'];
export const providers = ['manual', 'apple', 'google'];
export const outcomes = ['accepted', 'declined-retry', 'back-retry'];

function syntheticVaultEmail(runId, caseId, stage) {
  const identity = createHash('sha256').update(JSON.stringify([runId, caseId, stage])).digest('hex').slice(0, 24);
  return `v${identity}@e2e.invalid`;
}

export function buildRegistrationPlan(runId) {
  if (!/^[a-zA-Z0-9_-]+$/.test(runId)) throw new Error('Registration run ID must be an opaque safe token');
  const cases = [];
  for (const platform of platforms) for (const provider of providers) {
    cases.push({ id: `create-${platform}-${provider}`, kind: 'create', platform, provider });
  }
  for (const outcome of outcomes) for (const platform of platforms) for (const provider of providers) {
    cases.push({ id: `join-${platform}-${provider}-${outcome}`, kind: 'join', platform, provider, outcome });
  }
  for (const platform of platforms) for (const outcome of outcomes) {
    cases.push({ id: `qr-${platform}-${outcome}`, kind: 'qr', platform, provider: 'manual', outcome });
  }
  return { testNumber: 51, runId, repetitions: 1, expectedCases: 45, cases: cases.map((entry, index) => {
    const position = platforms.indexOf(entry.platform);
    const approver = platforms[(position + 1) % platforms.length];
    const retryApprover = platforms[(position + 2) % platforms.length];
    return { ...entry, number: index + 1, repetition: 1, approver, retryApprover,
      vaultName: syntheticVaultEmail(runId, entry.id, 'primary'),
      retryVaultName: syntheticVaultEmail(runId, entry.id, 'retry'),
      cleanBoundary: 'fresh-server-container-and-app-sandboxes-and-browser-context',
      requires: [entry.provider !== 'manual' ? `real-${entry.platform}-${entry.provider}-account` : null,
        entry.kind === 'qr' ? `real-${entry.platform}-camera` : null].filter(Boolean),
    };
  }) };
}

export function registrationMarkers(plan, entry) {
  const scoped = (platform, action, producer, consumer = 'registration orchestrator') => ({
    key: `E2E: TEST51_${plan.runId}_${entry.id}_${platform.toUpperCase()}_${action}`,
    platform, action, producer, consumer,
  });
  const markers = [scoped('harness', 'CLEAN', 'fresh server/app reset and new browser context')];
  if (entry.kind === 'create') return [...markers,
    scoped(entry.platform, 'READY', 'registration01 rendered'),
    scoped(entry.platform, 'AVAILABLE', 'Core availability and enabled Create'),
    scoped(entry.platform, 'CREATED', 'Core membership and main UI'),
    scoped(entry.platform, 'QR_GENERATED', 'Add device exact Vault-name QR rendered and decoded'),
  ];
  markers.push(scoped(entry.approver, 'CREATED', 'primary creator main UI'),
    scoped(entry.approver, 'READY', 'responder Devices UI and no stale request'),
    scoped(entry.platform, 'READY', 'registration01 rendered'),
    scoped(entry.platform, 'TAKEN', 'Core existing-name result; disabled Create'),
    scoped(entry.platform, 'PENDING', 'Core persisted request and requester pending UI'));
  if (entry.kind === 'qr') markers.push(scoped(entry.approver, 'QR_GENERATED', 'Add device exact Vault-name QR'),
    scoped(entry.platform, 'QR_SCANNED', 'real scanner decoded and Core request persisted'));
  if (entry.outcome !== 'back-retry') markers.push(scoped(entry.approver, 'DECIDED', 'Core terminal response and pending row gone'));
  if (entry.outcome === 'declined-retry') markers.push(scoped(entry.platform, 'DECLINED', 'requester declined UI'));
  if (entry.outcome !== 'accepted' && entry.platform === 'ios') markers.push(scoped('ios', 'RESET_UI01', 'requester initial01 before host cleanup oracle'));
  if (entry.outcome !== 'accepted') markers.push(
    scoped(entry.retryApprover, 'CREATED', 'different pre-created Vault main UI'),
    scoped(entry.platform, 'RESET01', 'initial01, old incomplete identity absent and new Local bootstrap'),
    scoped(entry.retryApprover, 'READY', 'retry responder ready before second Request'),
    scoped(entry.platform, 'RETRY_PENDING', 'different Vault request persisted and pending UI'),
    scoped(entry.retryApprover, 'DECIDED', 'different Vault accepted and pending row gone'));
  markers.push(scoped(entry.platform, 'MAIN', 'requester main UI after sync'));
  return markers;
}

export function validateRegistrationPlan(plan) {
  if (plan.repetitions !== 1 || plan.cases.length !== 45) throw new Error('Expanded matrix has45 variants, one owner-requested repetition each');
  const expected = buildRegistrationPlan(plan.runId);
  if (JSON.stringify(plan) !== JSON.stringify(expected)) throw new Error('Registration matrix changed from owner-approved45 variants');
  const ledger = plan.cases.flatMap((entry) => registrationMarkers(plan, entry));
  if (new Set(ledger.map((entry) => entry.key)).size !== ledger.length) throw new Error('Duplicate registration marker');
  return { cases: plan.cases.length, create: 9, join: 27, qr: 9, markers: ledger.length, repetitions: 1 };
}

export function caseBlockers(entry, environment = process.env) {
  const blockers = [];
  if (entry.provider === 'apple' && entry.platform === 'android') blockers.push('Apple provider on Android is unsupported by the current application');
  if (entry.provider !== 'manual' && environment[`E2E_REAL_${entry.platform.toUpperCase()}_${entry.provider.toUpperCase()}_READY`] !== '1') {
    blockers.push(`Real ${entry.platform}/${entry.provider} provider account, consent and email fixture are not prepared`);
  }
  if (entry.provider !== 'manual' && !environment[`E2E_REAL_${entry.platform.toUpperCase()}_${entry.provider.toUpperCase()}_EMAIL`]) blockers.push('Real provider fixture must supply its exact account email or Apple relay address');
  if (entry.kind === 'qr') blockers.push(`Real ${entry.platform} camera scan needs physical-device/manual handoff; no mocked decode may count as UI E2E`);
  return blockers;
}
