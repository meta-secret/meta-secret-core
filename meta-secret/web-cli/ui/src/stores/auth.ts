import { defineStore } from 'pinia';
import { ref } from 'vue';
import init, { MasterKeyManager } from 'meta-secret-web-cli';

export const useAuthStore = defineStore('auth', () => {
  const isAuthenticated = ref(false);

  const masterKey = ref<string | null>(null);

  const savedCredId = localStorage.getItem('credential_id');
  const hasRegisteredPasskey = ref(!!savedCredId);

  // Check if device supports WebAuthn
  const isWebAuthnSupported = typeof window !== 'undefined' && typeof window.PublicKeyCredential !== 'undefined';

  /**
   * Create a new passkey credential (registration)
   */
  async function createPasskeyCredential(): Promise<boolean> {
    if (!isWebAuthnSupported) {
      throw new Error('WebAuthn is not supported in this browser');
    }

    await init();

    try {
      const challenge = new Uint8Array(32);
      crypto.getRandomValues(challenge);

      let generatedMasterKey = MasterKeyManager.generate_sk();

      // Store masterKey for later use
      masterKey.value = generatedMasterKey;

      // Create the credential options
      const publicKeyCredentialCreationOptions: PublicKeyCredentialCreationOptions = {
        challenge,
        rp: {
          name: 'Meta-Secret Vault',
          id: window.location.hostname,
        },
        user: {
          id: new TextEncoder().encode(generatedMasterKey),
          name: 'id0',
          displayName: 'id0 Meta Human',
        },
        pubKeyCredParams: [
          {type: 'public-key', alg: -7}, // ES256
          {type: 'public-key', alg: -257}, // RS256
        ],
        authenticatorSelection: {
          authenticatorAttachment: 'platform', // Use built-in authenticator (TouchID, FaceID, Windows Hello)
          userVerification: 'required', // Require biometric verification
          residentKey: 'required', // Make this a discoverable credential (resident key)
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

        // Extract and store credential ID
        const publicKeyCredential = credential as PublicKeyCredential;
        const credId = publicKeyCredential.id; // Already base64url encoded

        // Mark that the user has registered a passkey
        hasRegisteredPasskey.value = true;
        localStorage.setItem('credential_id', credId);

        // Set as authenticated since we just registered
        isAuthenticated.value = true;

        return true;
      } else {
        return false;
      }
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

    let response: AuthenticatorAssertionResponse;
    try {
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

      if (!credential) {
        return false;
      }
      // Cast to PublicKeyCredential to access response property
      const publicKeyCredential = credential as PublicKeyCredential;
      response = publicKeyCredential.response as AuthenticatorAssertionResponse;
    } catch (error) {
      console.error('Authentication error:', error);
      throw error;
    }

    // Properly decode the userHandle that was originally encoded with TextEncoder
    if (response.userHandle) {
      // Decode the userHandle using TextDecoder (counterpart to TextEncoder used during registration)
      masterKey.value = new TextDecoder().decode(response.userHandle);
    } else {
      throw Error('Credential retrieved successfully but no user handle was returned');
    }

    // Mark the user as authenticated
    isAuthenticated.value = true;
    return true;
  }

  return {
    isAuthenticated,
    hasRegisteredPasskey,
    isWebAuthnSupported,
    authenticateWithPasskey,
    createPasskeyCredential,
    masterKey,
  };
});
