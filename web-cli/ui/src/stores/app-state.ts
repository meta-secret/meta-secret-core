import {defineStore} from "pinia";
import init, {WasmApplicationStateManager} from "meta-secret-web-cli";
import {ApplicationState} from "@/model/ApplicationState";

class JsAppStateManager {
  appState: any
  
  constructor(appState) {
    this.appState = appState;
  }
  
  async updateJsState(newState) {
    this.appState.internalState = newState;
  }
}

export const AppState = defineStore("app_state", {
  state: () => {
    console.log("App state. Init");
    
    let internalState: ApplicationState = {
      joinComponent: false,
      metaVault: undefined,
      vault: undefined,
      metaPasswords: []
    };

    return {
      internalState: internalState,
      stateManager: undefined as WasmApplicationStateManager | undefined,
    };
  },

  actions: {
    async appStateInit() {
      console.log("Js: App state init");
      await init();
      
      let jsAppStateManager = new JsAppStateManager(this);
      
      let stateManager = await WasmApplicationStateManager.new(jsAppStateManager);
      this.stateManager = await stateManager.init();
      
      this.stateManager;
    }
  },
});
