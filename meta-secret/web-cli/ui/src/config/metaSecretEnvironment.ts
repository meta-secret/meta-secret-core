export type MetaSecretEnvironment = 'local' | 'remote';

const configuredEnvironment = (import.meta.env.VITE_META_SECRET_ENV as string | undefined)?.trim().toLowerCase();

export const META_SECRET_ENVIRONMENT: MetaSecretEnvironment =
  configuredEnvironment === 'local' ? 'local' : 'remote';

export function resolveMetaSecretStateEventsBaseUrl() {
  return META_SECRET_ENVIRONMENT === 'local'
    ? 'http://127.0.0.1:3000/state-events'
    : 'https://api.meta-secret.org/state-events';
}
