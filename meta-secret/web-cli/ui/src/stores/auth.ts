import { defineStore } from 'pinia';
import { ref } from 'vue';
import { MasterKeyManager } from '../../pkg';

export const useAuthStore = defineStore('auth', () => {
  const isAuthenticated = ref(false);
  const hasRegisteredPasskey = ref(false);

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
        
        // Extract and store credential ID
        const publicKeyCredential = credential as PublicKeyCredential;
        const credId = publicKeyCredential.id;


        // Mark that the user has registered a passkey
        hasRegisteredPasskey.value = true;
        localStorage.setItem('has_registered_passkey', 'true');
        localStorage.setItem('credential_id', credId);
        
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
    
    try {
      const challenge = new Uint8Array(32);
      crypto.getRandomValues(challenge);
      
      // Get the stored credential ID
      const storedCredentialId = localStorage.getItem('credential_id');
      const allowCredentials: PublicKeyCredentialDescriptor[] = storedCredentialId ? [{
        type: 'public-key' as const,
        id: Uint8Array.from(atob(storedCredentialId), c => c.charCodeAt(0)),
        transports: ['internal' as AuthenticatorTransport]
      }] : [];
      
      // Create the credential request options
      const publicKeyCredentialRequestOptions: PublicKeyCredentialRequestOptions = {
        challenge,
        timeout: 60000, // 1 minute
        userVerification: 'required', // Require biometric verification
        allowCredentials,
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
    
    // Optionally, also remove credential information when signing out
    // Uncomment if you want to require re-registration after sign out
    // hasRegisteredPasskey.value = false;
    // localStorage.removeItem('credential_id');
    // localStorage.removeItem('has_registered_passkey');
  }

  return {
    isAuthenticated,
    hasRegisteredPasskey,
    isWebAuthnSupported,
    authenticateWithPasskey,
    createPasskeyCredential,
  };
});
