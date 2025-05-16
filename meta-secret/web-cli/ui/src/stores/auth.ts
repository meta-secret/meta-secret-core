import { defineStore } from 'pinia';
import { ref } from 'vue';
import { MasterKeyManager } from '../../pkg';

export const useAuthStore = defineStore('auth', () => {
  const isAuthenticated = ref(false);
  const hasRegisteredPasskey = ref(false);
  const userId = ref<string>('');

  // Check if device supports WebAuthn
  const isWebAuthnSupported = typeof window !== 'undefined' && typeof window.PublicKeyCredential !== 'undefined';

  /**
   * Create a new passkey credential (registration)
   */
  async function createPasskeyCredential(): Promise<boolean> {
    if (!isWebAuthnSupported) {
      throw new Error('WebAuthn is not supported in this browser');
    }

    try {
      // Generate a random user ID if not already set
      if (!userId.value) {
        userId.value = crypto.randomUUID();
      }

      const challenge = new Uint8Array(32);
      crypto.getRandomValues(challenge);

      let masterKey = MasterKeyManager.generate_sk();

      // Create the credential options
      const publicKeyCredentialCreationOptions: PublicKeyCredentialCreationOptions = {
        challenge,
        rp: {
          name: 'Meta-Secret Vault',
          id: window.location.hostname,
        },
        user: {
          id: new TextEncoder().encode(masterKey),
          name: 'id0',
          displayName: 'id0 Meta Human',
        },
        pubKeyCredParams: [
          { type: 'public-key', alg: -7 }, // ES256
        ],
        authenticatorSelection: {
          authenticatorAttachment: 'platform', // Use built-in authenticator (TouchID, FaceID, Windows Hello)
          userVerification: 'required', // Require biometric verification
        },
        timeout: 60000, // 1 minute
        attestation: 'none',
      };

      // Create the credential
      const credential = await navigator.credentials.create({
        publicKey: publicKeyCredentialCreationOptions,
      });

      if (credential) {
        console.log('Credential created successfully:', credential);
        // Mark that the user has registered a passkey
        hasRegisteredPasskey.value = true;
        localStorage.setItem('has_registered_passkey', 'true');
        localStorage.setItem('user_id', userId.value);

        return true;
      }

      return false;
    } catch (error) {
      console.error('Error creating credential:', error);
      throw error;
    }
  }

  /**
   * Authenticate with passkey (biometric)
   * This uses the WebAuthn API
   */
  async function authenticateWithPasskey(): Promise<boolean> {
    if (!isWebAuthnSupported) {
      throw new Error('WebAuthn is not supported in this browser');
    }

    try {
      // In a real app, this challenge would come from the server
      const challenge = new Uint8Array(32);
      crypto.getRandomValues(challenge);

      // Create the credential request options
      const publicKeyCredentialRequestOptions: PublicKeyCredentialRequestOptions = {
        challenge,
        timeout: 60000, // 1 minute
        userVerification: 'required', // Require biometric verification
      };

      // Request the credential
      const credential = await navigator.credentials.get({
        publicKey: publicKeyCredentialRequestOptions,
      });

      if (credential) {
        console.log('User authenticated successfully:', credential);
        // In a real app, you would send the credential to the server for verification

        // Mark the user as authenticated
        isAuthenticated.value = true;
        localStorage.setItem('auth_state', 'authenticated');
        return true;
      }

      return false;
    } catch (error) {
      console.error('Error authenticating:', error);

      // For demo purposes, fall back to mock authentication if WebAuthn fails
      if (process.env.NODE_ENV === 'development') {
        console.warn('Falling back to mock authentication in development mode');
        isAuthenticated.value = true;
        localStorage.setItem('auth_state', 'authenticated');
        return true;
      }

      throw error;
    }
  }

  /**
   * Sign out the user
   */
  function signOut(): void {
    isAuthenticated.value = false;
    localStorage.removeItem('auth_state');
  }

  /**
   * Check if user was previously authenticated
   */
  function checkPreviousAuth(): void {
    const savedAuth = localStorage.getItem('auth_state');
    if (savedAuth === 'authenticated') {
      isAuthenticated.value = true;
    }

    const savedHasRegisteredPasskey = localStorage.getItem('has_registered_passkey');
    if (savedHasRegisteredPasskey === 'true') {
      hasRegisteredPasskey.value = true;
    }

    const savedUserId = localStorage.getItem('user_id');
    if (savedUserId) {
      userId.value = savedUserId;
    }
  }

  // Initialize auth state from localStorage when store is created
  checkPreviousAuth();

  return {
    isAuthenticated,
    hasRegisteredPasskey,
    isWebAuthnSupported,
    authenticateWithPasskey,
    createPasskeyCredential,
    signOut,
    checkPreviousAuth,
  };
});
