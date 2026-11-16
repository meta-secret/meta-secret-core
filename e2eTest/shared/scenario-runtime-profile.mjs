const participants = Object.freeze(['CLI/Core + local HTTP server', 'isolated local HTTP server']);
export function createScenarioRuntimeProfile({ testNumber, scenario }) {
  if (!Number.isInteger(testNumber)) throw new Error('A registered integer test number is required for a runtime profile');
  if (testNumber === 20) {
    if (Object.keys(scenario).some((key) => !['name', 'cli', 'server'].includes(key))
      || JSON.stringify(scenario.cli) !== JSON.stringify({ participant: 'Rust Core and HTTP integration' })
      || JSON.stringify(scenario.server) !== JSON.stringify({ participant: 'isolated local meta-server' })) throw new Error('Test20 runtime profile requires its exact participant-only scenario');
    return Object.freeze({ kind: 'core-http', testNumber, serverUrl: 'http://127.0.0.1:3000', stateReset: 'child-owned', requiredNonPlatformParticipants: participants });
  }
  assertUiScenario(scenario);
  return Object.freeze({ kind: 'ui', testNumber });
}
export function assertUiScenario(scenario) {
  if (!scenario.server?.url || !scenario.server?.container || !scenario.server?.port || !scenario.web?.url) throw new Error('UI runtime profile requires server URL/container/port and Web URL');
  for (const endpoint of [scenario.server.url, scenario.web.url]) if (!['127.0.0.1', 'localhost', '::1', '[::1]'].includes(new URL(endpoint).hostname)) throw new Error('UI runtime profile requires local loopback targets');
  if (scenario.server.environment && scenario.server.environment !== 'local') throw new Error('UI runtime profile requires local environment');
  if (scenario.android && !scenario.android.avdName || scenario.ios && !scenario.ios.simulatorName) throw new Error('UI runtime profile requires exact declared native selectors');
}
export function validateRuntimeProfiles(scenarios, profiles) {
  if (profiles === undefined) { scenarios.forEach(assertUiScenario); return scenarios.map(() => ({ kind: 'ui' })); }
  if (!Array.isArray(profiles) || profiles.length !== scenarios.length) throw new Error('Runtime profile count must match registered scenarios');
  return scenarios.map((scenario, index) => {
    const given = profiles[index]; const expected = createScenarioRuntimeProfile({ testNumber: given?.testNumber, scenario });
    if (JSON.stringify(given) !== JSON.stringify(expected)) throw new Error('Runtime profile conflicts with its registered scenario');
    return expected;
  });
}
export function coreHttpProofsReady(report, profile = report.runtimeProfile) {
  if (profile?.kind !== 'core-http') return false;
  if (JSON.stringify(report.requiredNonPlatformParticipants) !== JSON.stringify(participants)) return false;
  const records = report.nonPlatformParticipants ?? [];
  const cli = records.filter((entry) => entry.participant === participants[0]);
  const http = records.filter((entry) => entry.participant === participants[1]);
  const c = cli[0]?.checks; const h = http[0]?.checks;
  return cli.length === 1 && http.length === 1 && !c?.failed && !h?.failed
    && ['cargo', 'sqlite3', 'coreManifest', 'serverMigration'].every((key) => typeof c?.[key] === 'string' && c[key].trim())
    && h?.url === profile.serverUrl && h?.healthEndpoint === '/hi' && h?.healthy === true;
}
export function assertScenarioReadiness({ profile, report, outcome }) {
  if (outcome?.exitCode !== 0 || outcome.signal || outcome.error) throw new Error('Authoritative child result did not exit successfully');
  if (report.error || report.failures?.length || report.stateReset?.status === 'failed') throw new Error('Scenario readiness contains a failed proof');
  if (report.status !== 'ready' || !report.platforms?.every((entry) => entry.installation)) throw new Error('Scenario readiness proof is incomplete');
  if (profile.kind === 'core-http' && !coreHttpProofsReady(report, profile)) throw new Error('Test20 named participant/route readiness proof is incomplete');
  return report;
}
