import { createHash } from 'node:crypto';

const hash = (value) => createHash('sha256').update(String(value)).digest('hex').slice(0, 12);

// Validate before writing XCTest configuration or launching an actor. Neither
// the host config nor launch environment may override a conflicting selection.
export function bindIosStepSecret({ secretName, secretValue, environment = {}, defaultSecret, valueForName }) {
  const explicitName = secretName || undefined;
  const environmentName = environment.E2E_SECRET_NAME || undefined;
  if (explicitName && environmentName && explicitName !== environmentName) {
    throw new Error(`iOS selected-secret conflict: explicitHash=${hash(explicitName)} environmentHash=${hash(environmentName)}`);
  }
  if (secretValue != null && environment.E2E_SECRET_VALUE != null && secretValue !== environment.E2E_SECRET_VALUE) {
    throw new Error('iOS selected-secret value conflict (payload omitted)');
  }
  const name = explicitName ?? environmentName ?? defaultSecret?.name ?? '';
  const expectedValue = valueForName(name);
  const suppliedValue = secretValue ?? environment.E2E_SECRET_VALUE;
  if (suppliedValue != null && expectedValue != null && suppliedValue !== expectedValue) {
    throw new Error('iOS selected-secret value does not match its scenario selection (payload omitted)');
  }
  const value = suppliedValue ?? expectedValue
    ?? (name === defaultSecret?.name ? defaultSecret?.value : undefined) ?? '';
  return { name, value, environment: { ...environment, E2E_SECRET_NAME: name, E2E_SECRET_VALUE: value } };
}

// Missing selection is intentionally absent. Injecting the default Secret's
// name here would conflict with an actor's explicit later-cycle selection.
export function createStepEnvironment({
  role, cycle, step, approvalPlatform, sender, secretName,
  action = '', expectedOutcome = '', verifyReceiverAlert = false, receiverRequestAlreadySent = false,
}) {
  return {
    E2E_ROLE: role, E2E_CYCLE: String(cycle), E2E_STEP: String(step),
    E2E_APPROVAL_PLATFORM: approvalPlatform, E2E_SENDER: sender,
    E2E_ACTION: action, E2E_EXPECTED_OUTCOME: expectedOutcome,
    E2E_VERIFY_RECEIVER_ALERT: verifyReceiverAlert ? '1' : '0',
    E2E_RECEIVER_REQUEST_ALREADY_SENT: receiverRequestAlreadySent ? '1' : '0',
    ...(secretName !== undefined ? { E2E_SECRET_NAME: secretName } : {}),
  };
}
