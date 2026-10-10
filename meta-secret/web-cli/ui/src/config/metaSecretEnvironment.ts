export type MetaSecretEnvironment = 'local' | 'remote';

const configuredEnvironment = (import.meta.env.VITE_META_SECRET_ENV as string | undefined)?.trim().toLowerCase();

export const META_SECRET_ENVIRONMENT: MetaSecretEnvironment = configuredEnvironment === 'local' ? 'local' : 'remote';

export function resolveMetaSecretStateEventsBaseUrl() {
  return META_SECRET_ENVIRONMENT === 'local'
    ? 'http://127.0.0.1:3000/state-events'
    : 'https://api.meta-secret.org/state-events';
}

export function resolveMetaSecretJoinEventsBaseUrl() {
  const configuredUrl = import.meta.env.VITE_JOIN_EVENTS_URL as string | undefined;
  if (configuredUrl) return configuredUrl;
  return META_SECRET_ENVIRONMENT === 'local'
    ? 'http://127.0.0.1:3000/join-events'
    : 'https://api.meta-secret.org/join-events';
}
