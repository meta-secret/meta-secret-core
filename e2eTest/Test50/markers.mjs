/** Exact native producer/consumer ledger; diagnostic markers are explicitly passive. */
export function deletionMarkerLedger(plan) {
  const ledger = [];
  const add = (block, platform, cycle, step, actions, diagnostics = true) => {
    for (const action of actions) ledger.push({
      key: `${block.runId}/${platform}/${cycle}/${step}/${action}`,
      marker: `E2E: ${platform.toUpperCase()}_DELETION_${action}_${cycle}_${block.runId}-${step}`,
      producer: `${platform}.handleSecretDeletionStep`, consumer: 'Test50.business',
    });
    if (platform === 'android' && diagnostics) ledger.push({
      key: `${block.runId}/${platform}/${cycle}/${step}/ID`,
      marker: `E2E: ANDROID_DELETION_ID_${cycle}_${block.runId}-${step}:<opaque-id>`,
      producer: 'android.cachedAppState', consumer: block.members.length === 1 && step === 'recreated-show-android' ? 'Test50.business identity metadata' : 'diagnostic identity metadata',
    });
  };
  for (const block of plan.blocks) {
    const initiator = block.members[0];
    if (block.members.includes('android')) ledger.push({
      key: `${block.runId}/android/f3/ACCESSIBILITY_READY`, marker: 'E2E: ANDROID_TEST50_ACCESSIBILITY_READY',
      producer: 'android.verifyTest50AccessibilityReady', consumer: 'Test50.business F3 pre-mutation readiness',
    });
    if (['android', 'ios'].includes(initiator)) for (const suffix of ['INITIATOR_SETUP_READY', 'SECRET1_READY']) ledger.push({
      key: `${block.runId}/${initiator}/setup/${suffix}`,
      marker: `E2E: ${initiator.toUpperCase()}_TEST50_${suffix}`,
      producer: `${initiator}.createVaultForTest50`, consumer: suffix === 'SECRET1_READY' ? 'diagnostic setup' : 'Test50.business',
    });
    for (const platform of block.members.slice(1).filter((platform) => ['android', 'ios'].includes(platform))) {
      for (const suffix of ['JOIN_REQUEST_SENT', 'JOIN_READY', 'SECRETS_READY']) ledger.push({
        key: `${block.runId}/${platform}/setup/${suffix}`, marker: `E2E: ${platform.toUpperCase()}_${suffix}`,
        producer: `${platform}.joinVault`, consumer: suffix === 'SECRETS_READY' ? 'diagnostic setup' : 'Test50.business',
      });
    }
    for (const cycle of block.cycles) {
      if (['android', 'ios'].includes(initiator)) for (const generation of ['create', ...(cycle.outcome === 'accepted' ? ['recreate'] : [])]) ledger.push({
        key: `${block.runId}/${initiator}/${cycle.number}/${generation}/CREATED`,
        marker: `E2E: ${initiator.toUpperCase()}_TEST50_SECRET_CREATED_${cycle.number}_${block.runId}-${generation}`,
        producer: `${initiator}.createSecretForTest50:${generation}`, consumer: 'Test50.business (fresh owned process)',
      });
      if (['android', 'ios'].includes(cycle.requester)) add(block, cycle.requester, cycle.number, 'cancel', ['READY', 'CANCELLED']);
      if (block.members.length <= 2) for (const platform of block.members.filter((platform) => ['android', 'ios'].includes(platform))) add(block, platform, cycle.number, `show-${platform}`, ['READY', 'SHOW_VALID']);
      for (const platform of block.members.filter((platform) => ['android', 'ios'].includes(platform))) {
        if (platform === cycle.offline) add(block, platform, cycle.number, 'main', ['READY', 'SYNC_BLOCKED']);
        else if (platform === cycle.requester) add(block, platform, cycle.number, 'main', ['READY', 'SUBMITTED', 'TERMINAL']);
        else if (platform === cycle.responder) add(block, platform, cycle.number, 'main', ['READY', 'REQUEST_VISIBLE', 'SUBMITTED', 'TERMINAL']);
        else add(block, platform, cycle.number, 'main', ['READY', ...(block.members.length === 3 ? ['REQUEST_VISIBLE'] : []), 'TERMINAL']);
      }
      if (cycle.outcome === 'accepted' && block.members.length <= 2 && ['android', 'ios'].includes(initiator)) add(block, initiator, cycle.number, `recreated-show-${initiator}`, ['READY', 'SHOW_VALID']);
      if (['android', 'ios'].includes(cycle.offline)) add(block, cycle.offline, cycle.number, 'startup', ['READY', 'TERMINAL'], false);
    }
  }
  const iosInvocations = new Set(ledger.filter(({ producer }) => producer.startsWith('ios.')).map(({ key }) => key.slice(0, key.lastIndexOf('/'))));
  for (const invocation of iosInvocations) ledger.push({ key: `${invocation}/CORE_ROUTING_VERIFIED`, marker: 'E2E: IOS_TEST50_CORE_ROUTING_VERIFIED', producer: 'ios.setUpWithError', consumer: 'Test50.business routing gate (fresh owned actor)' });
  if (new Set(ledger.map(({ key }) => key)).size !== ledger.length) throw new Error('Duplicate scoped marker producer');
  return { ledger, produced: ledger.length, consumed: ledger.filter(({ consumer }) => consumer.startsWith('Test50.business')).length,
    diagnostics: ledger.filter(({ consumer }) => !consumer.startsWith('Test50.business')).length };
}
