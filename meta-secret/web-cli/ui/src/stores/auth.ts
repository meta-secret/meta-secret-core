import { defineStore } from 'pinia';
import { ref } from 'vue';

export const useAuthStore = defineStore('auth', () => {
  const isAuthenticated = ref(false);
  
  /**
   * Authenticate with passkey (biometric)
   * In a real implementation, this would use the WebAuthn API
   */
  async function authenticateWithPasskey(): Promise<void> {
    return new Promise((resolve, reject) => {
      try {
        // This is a mock implementation
        // In a real app, you would use the WebAuthn API here:
        // - navigator.credentials.get() for authentication
        
        // Mock successful authentication
        isAuthenticated.value = true;
        
        // Save authentication state to localStorage for persistence
        localStorage.setItem('auth_state', 'authenticated');
        
        resolve();
      } catch (error) {
        reject(error);
      }
    });
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
  }
  
  // Initialize auth state from localStorage when store is created
  checkPreviousAuth();
  
  return {
    isAuthenticated,
    authenticateWithPasskey,
    signOut,
    checkPreviousAuth
  };
}); 