import { parseVaultName } from './registrationState';
import { configuredAppleBroker, selectAppleBrokerEmail } from './appleEmailBroker';

type TokenResponse = { access_token?: string; error?: string };
type TokenClient = { requestAccessToken(options: { prompt: string }): void };
type ProviderWindow = Window & {
  google?: {
    accounts: {
      oauth2: {
        initTokenClient(options: {
          client_id: string;
          scope: string;
          callback: (response: TokenResponse) => void;
          error_callback: () => void;
        }): TokenClient;
      };
    };
  };
};
const providerWindow = () => window as ProviderWindow;
const googleClientId = (import.meta.env.VITE_GOOGLE_WEB_CLIENT_ID as string | undefined)?.trim();
const appleBroker = configuredAppleBroker(import.meta.env.VITE_APPLE_EMAIL_BROKER_URL as string | undefined);
const loaded = new Map<string, Promise<void>>();
function loadSdk(url: string) {
  if (!loaded.has(url))
    loaded.set(
      url,
      new Promise<void>((resolve, reject) => {
        const script = document.createElement('script');
        script.src = url;
        script.async = true;
        script.referrerPolicy = 'no-referrer';
        script.onload = () => resolve();
        script.onerror = () => {
          loaded.delete(url);
          script.remove();
          reject(new Error('Could not connect to provider.'));
        };
        document.head.append(script);
      }),
    );
  return loaded.get(url)!;
}
export function warmEmailProviders() {
  if (googleClientId) void loadSdk('https://accounts.google.com/gsi/client').catch(() => undefined);
}

export const emailProviderCapabilities = {
  google: Boolean(googleClientId),
  apple: Boolean(appleBroker),
};

export async function selectGoogleEmail(): Promise<string> {
  if (!googleClientId) throw new Error('Google sign-in is not configured.');
  await loadSdk('https://accounts.google.com/gsi/client');
  const oauth = providerWindow().google?.accounts.oauth2;
  if (!oauth) throw new Error('Google sign-in is unavailable.');
  return new Promise<string>((resolve, reject) => {
    const client = oauth.initTokenClient({
      client_id: googleClientId,
      scope: 'openid email',
      error_callback: () => reject(new Error('Google sign-in was closed.')),
      callback: async (response) => {
        if (!response.access_token || response.error) {
          reject(new Error('Google sign-in was not completed.'));
          return;
        }
        try {
          const result = await fetch('https://openidconnect.googleapis.com/v1/userinfo', {
            headers: { Authorization: `Bearer ${response.access_token}` },
            cache: 'no-store',
          });
          response.access_token = undefined;
          if (!result.ok) throw new Error('Could not read your email.');
          const info = (await result.json()) as { email?: unknown };
          if (typeof info.email !== 'string') throw new Error('No email was shared.');
          resolve(parseVaultName(info.email));
        } catch {
          reject(new Error('Could not read your email.'));
        } finally {
          response.access_token = undefined;
        }
      },
    });
    client.requestAccessToken({ prompt: 'select_account' });
  });
}

export async function selectAppleEmail(signal?: AbortSignal): Promise<string> {
  if (!appleBroker) throw new Error('Apple sign-in is not configured.');
  return selectAppleBrokerEmail(appleBroker, signal);
}
